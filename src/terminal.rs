//! Terminal engine - PTY-based terminal with grid, parser, and input handling.

use std::collections::VecDeque;

use crate::surface::Color;
use crate::SurfaceId;

/// TextSource abstraction - any source that produces text content.
/// Terminal PTY, command output, file tail, and plugin-provided sources all
/// implement this trait, making them interchangeable live text surfaces.
pub trait TextSource {
    fn read(&mut self) -> Option<String>;
    fn write(&mut self, text: &str) -> Result<(), String>;
    fn is_active(&self) -> bool;
}

/// Terminal configuration
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    pub scrollback_lines: usize,
    pub copy_on_select: bool,
    pub ambiguous_width: u8,
    pub font_family: String,
    pub font_size: u32,
    pub profile: String,
}

impl TerminalConfig {
    pub fn new() -> Self {
        Self {
            scrollback_lines: 10000,
            copy_on_select: false,
            ambiguous_width: 1,
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            profile: "default".to_string(),
        }
    }

    pub fn profile(name: &str, font_size: u32) -> Self {
        Self {
            profile: name.to_string(),
            font_size,
            ..Self::default()
        }
    }
}

/// Terminal grid cell
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCell {
    pub character: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub width: u8,
}

impl TerminalCell {
    pub fn new() -> Self {
        Self {
            character: ' ',
            fg: Color::rgb(220, 220, 220),
            bg: Color::from_hex("#0b0d12"),
            bold: false,
            italic: false,
            underline: false,
            reverse: false,
            width: 1,
        }
    }
}

/// Terminal grid - stores terminal buffer
pub struct TerminalGrid {
    pub rows: u32,
    pub cols: u32,
    pub cells: Vec<Vec<TerminalCell>>,
    /// Capped at `scrollback_lines`; a `VecDeque` so evicting the oldest
    /// line is O(1) — `Vec::remove(0)` memmoved up to 10k rows per line
    /// during heavy output.
    pub scrollback: VecDeque<Vec<TerminalCell>>,
}

impl TerminalGrid {
    pub fn new(rows: u32, cols: u32) -> Self {
        Self {
            cells: vec![vec![TerminalCell::default(); cols as usize]; rows as usize],
            scrollback: VecDeque::new(),
            rows,
            cols,
        }
    }

    pub fn get(&self, row: u32, col: u32) -> Option<&TerminalCell> {
        self.cells.get(row as usize)?.get(col as usize)
    }

    pub fn set(&mut self, row: u32, col: u32, cell: TerminalCell) {
        if let Some(r) = self.cells.get_mut(row as usize) {
            if let Some(c) = r.get_mut(col as usize) {
                *c = cell;
            }
        }
    }

    pub fn resize(&mut self, new_rows: u32, new_cols: u32) {
        if new_rows == self.rows && new_cols == self.cols {
            return;
        }
        let new_rows = new_rows.max(1);
        let new_cols = new_cols.max(1);
        // Preserve overlapping content: window resizes must never wipe the
        // visible terminal.
        let mut cells = vec![vec![TerminalCell::default(); new_cols as usize]; new_rows as usize];
        for (r, row) in cells
            .iter_mut()
            .enumerate()
            .take(self.rows.min(new_rows) as usize)
        {
            for (c, cell) in row
                .iter_mut()
                .enumerate()
                .take(self.cols.min(new_cols) as usize)
            {
                *cell = self.cells[r][c].clone();
            }
        }
        self.cells = cells;
        self.rows = new_rows;
        self.cols = new_cols;
    }
}

/// Terminal state
pub struct Terminal {
    pub id: SurfaceId,
    pub grid: TerminalGrid,
    pub config: TerminalConfig,
    pub cursor_row: u32,
    pub cursor_col: u32,
    pub scroll_offset: i32,
    pub dirty: bool,
}

impl Terminal {
    pub fn new(id: SurfaceId, rows: u32, cols: u32) -> Self {
        Self {
            id,
            grid: TerminalGrid::new(rows, cols),
            config: TerminalConfig::default(),
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            dirty: false,
        }
    }

    pub fn scroll(&mut self, lines: i32) {
        let max = self.grid.scrollback.len() as i32;
        self.scroll_offset = (self.scroll_offset + lines).clamp(0, max);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn visible_line(&self, display_row: u32) -> Option<&[TerminalCell]> {
        let rows = self.grid.rows as usize;
        let sb = self.grid.scrollback.len();
        let off = (self.scroll_offset as usize).min(sb);
        let idx = sb - off + display_row as usize;
        if idx < sb {
            Some(&self.grid.scrollback[idx])
        } else {
            self.grid
                .cells
                .get(idx - sb)
                .map(|v| v.as_slice())
                .filter(|_| (display_row as usize) < rows)
        }
    }

    pub fn selected_text(&self, a: (u32, u32), b: (u32, u32)) -> String {
        let ((c0, r0), (c1, r1)) = if (a.1, a.0) <= (b.1, b.0) {
            (a, b)
        } else {
            (b, a)
        };
        let mut out = String::new();
        for r in r0..=r1.min(self.grid.rows.saturating_sub(1)) {
            let c_start = if r == r0 { c0 } else { 0 };
            let c_end = if r == r1 {
                c1
            } else {
                self.grid.cols.saturating_sub(1)
            };
            let mut line = String::new();
            for c in c_start..=c_end.min(self.grid.cols.saturating_sub(1)) {
                if let Some(cell) = self.grid.get(r, c) {
                    if cell.width == 0 {
                        continue;
                    }
                    line.push(cell.character);
                }
            }
            out.push_str(line.trim_end());
            if r != r1 {
                out.push('\n');
            }
        }
        out
    }

    pub fn scrollback_content(&self) -> &VecDeque<Vec<TerminalCell>> {
        &self.grid.scrollback
    }

    pub fn write(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                self.cursor_row += 1;
                self.cursor_col = 0;
            } else {
                if self.cursor_col < self.grid.cols {
                    if self.cursor_row < self.grid.rows {
                        self.grid.set(
                            self.cursor_row,
                            self.cursor_col,
                            TerminalCell {
                                character: ch,
                                ..TerminalCell::default()
                            },
                        );
                    }
                    self.cursor_col += 1;
                }
            }
            if self.cursor_col >= self.grid.cols {
                self.cursor_col = 0;
                self.cursor_row += 1;
            }
            if self.cursor_row >= self.grid.rows {
                if self.grid.rows > 0 {
                    let first_row = self.grid.cells.remove(0);
                    self.grid.scrollback.push_back(first_row);
                    if self.grid.scrollback.len() > self.config.scrollback_lines {
                        self.grid.scrollback.pop_front();
                    }
                    self.grid
                        .cells
                        .push(vec![TerminalCell::default(); self.grid.cols as usize]);
                }
                self.cursor_row = self.grid.rows.saturating_sub(1);
            }
        }
        self.dirty = true;
    }
}

/// TerminalTextSource - implements TextSource for terminal PTY
pub struct TerminalTextSource {
    terminal: Terminal,
}

impl TerminalTextSource {
    pub fn new(terminal: Terminal) -> Self {
        Self { terminal }
    }
}

impl TextSource for TerminalTextSource {
    fn read(&mut self) -> Option<String> {
        let mut result = String::new();
        for row in 0..self.terminal.grid.rows {
            let mut line = String::new();
            for col in 0..self.terminal.grid.cols {
                if let Some(cell) = self.terminal.grid.get(row, col) {
                    line.push(cell.character);
                }
            }
            result.push_str(line.trim_end());
            if row < self.terminal.grid.rows - 1 {
                result.push('\n');
            }
        }
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    fn write(&mut self, text: &str) -> Result<(), String> {
        self.terminal.write(text);
        Ok(())
    }

    fn is_active(&self) -> bool {
        true
    }
}

/// CommandOutputSource - implements TextSource for command output
pub struct CommandOutputSource {
    output: String,
    finished: bool,
}

impl CommandOutputSource {
    pub fn new(output: String) -> Self {
        Self {
            output,
            finished: false,
        }
    }
}

impl TextSource for CommandOutputSource {
    fn read(&mut self) -> Option<String> {
        if self.finished {
            None
        } else {
            self.finished = true;
            Some(self.output.clone())
        }
    }

    fn write(&mut self, _text: &str) -> Result<(), String> {
        Err("Cannot write to command output source".to_string())
    }

    fn is_active(&self) -> bool {
        !self.finished
    }
}

/// FileTailSource - implements TextSource for tailing a file
pub struct FileTailSource {
    path: std::path::PathBuf,
    position: u64,
}

impl FileTailSource {
    pub fn new(path: &str) -> Result<Self, std::io::Error> {
        let path = std::path::PathBuf::from(path);
        let position = std::fs::metadata(&path)?.len();
        Ok(Self { path, position })
    }
}

impl TextSource for FileTailSource {
    fn read(&mut self) -> Option<String> {
        use std::io::{BufRead, BufReader, Seek, SeekFrom};
        let mut file = std::fs::File::open(&self.path).ok()?;
        file.seek(SeekFrom::Start(self.position)).ok()?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).ok()?;
        self.position += bytes_read as u64;
        if line.is_empty() {
            None
        } else {
            Some(line)
        }
    }

    fn write(&mut self, _text: &str) -> Result<(), String> {
        Err("Cannot write to file tail source".to_string())
    }

    fn is_active(&self) -> bool {
        true
    }
}
impl Default for TerminalConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_text_source() {
        let mut terminal = Terminal::new(SurfaceId(1), 10, 10);
        terminal.write("Hello");
        let mut source = TerminalTextSource::new(terminal);
        let text = source.read();
        assert!(text.is_some());
        assert!(text.unwrap().contains("Hello"));
    }

    #[test]
    fn test_command_output_source() {
        let mut source = CommandOutputSource::new("test output".to_string());
        assert!(source.is_active());
        let text = source.read();
        assert_eq!(text.unwrap(), "test output");
        assert!(!source.is_active());
        assert!(source.read().is_none());
    }

    #[test]
    fn test_file_tail_source() {
        let result = FileTailSource::new("Cargo.toml");
        assert!(result.is_ok());
        let mut source = result.unwrap();
        // Reset to start for test
        source.position = 0;
        assert!(source.is_active());
        let text = source.read();
        assert!(text.is_some());
    }

    #[test]
    fn test_scrollback_cap_evicts_oldest_first() {
        let mut term = Terminal::new(SurfaceId(1), 2, 4);
        term.config.scrollback_lines = 3;
        // 2-row grid: each newline past the bottom scrolls one line.
        for ch in ['a', 'b', 'c', 'd', 'e', 'f', 'g'] {
            term.write(&format!("{ch}\n"));
        }
        assert_eq!(term.grid.scrollback.len(), 3);
        // Oldest retained line holds 'd', newest 'f': front-to-back order.
        assert_eq!(term.grid.scrollback[0][0].character, 'd');
        assert_eq!(term.grid.scrollback[2][0].character, 'f');
    }

    #[test]
    fn test_resize_preserves_content() {
        let mut grid = TerminalGrid::new(4, 4);
        grid.set(
            0,
            0,
            TerminalCell {
                character: 'A',
                ..TerminalCell::default()
            },
        );
        grid.set(
            3,
            3,
            TerminalCell {
                character: 'Z',
                ..TerminalCell::default()
            },
        );
        grid.resize(6, 6);
        assert_eq!(grid.rows, 6);
        assert_eq!(grid.cols, 6);
        assert_eq!(grid.get(0, 0).unwrap().character, 'A');
        assert_eq!(grid.get(3, 3).unwrap().character, 'Z');
        assert_eq!(grid.get(5, 5).unwrap().character, ' ');
        grid.resize(2, 2);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.cols, 2);
        assert_eq!(grid.get(0, 0).unwrap().character, 'A');
    }
}
