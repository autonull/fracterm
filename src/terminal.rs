//! Terminal engine - PTY-based terminal with grid, parser, and input handling.

use super::*;

/// Terminal configuration
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// Scrollback lines
    pub scrollback_lines: usize,
    /// Copy on select
    pub copy_on_select: bool,
    /// Ambiguous width
    pub ambiguous_width: u8,
    /// Font family
    pub font_family: String,
    /// Font size
    pub font_size: u32,
    /// Profile name
    pub profile: String,
}

impl TerminalConfig {
    /// Create default terminal configuration
    pub fn default() -> Self {
        Self {
            scrollback_lines: 10000,
            copy_on_select: false,
            ambiguous_width: 1,
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            profile: "default".to_string(),
        }
    }

    /// Create a profile configuration
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
    pub fn default() -> Self {
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
    pub scrollback: Vec<Vec<TerminalCell>>,
}

impl TerminalGrid {
    pub fn new(rows: u32, cols: u32) -> Self {
        Self {
            cells: vec![vec![TerminalCell::default(); cols as usize]; rows as usize],
            scrollback: Vec::new(),
            rows,
            cols,
        }
    }

    /// Get a cell by row/col
    pub fn get(&self, row: u32, col: u32) -> Option<&TerminalCell> {
        self.cells.get(row as usize)?.get(col as usize)
    }

    /// Set a cell
    pub fn set(&mut self, row: u32, col: u32, cell: TerminalCell) {
        if let Some(r) = self.cells.get_mut(row as usize) {
            if let Some(c) = r.get_mut(col as usize) {
                *c = cell;
            }
        }
    }

    /// Resize the grid
    pub fn resize(&mut self, new_rows: u32, new_cols: u32) {
        self.rows = new_rows;
        self.cols = new_cols;
        self.cells = vec![vec![TerminalCell::default(); new_cols as usize]; new_rows as usize];
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

    /// Scroll the terminal by N lines
    pub fn scroll(&mut self, lines: i32) {
        self.scroll_offset = (self.scroll_offset + lines).max(0);
    }

    /// Get the scrollback content
    pub fn scrollback_content(&self) -> &[Vec<TerminalCell>] {
        &self.scrollback
    }

    /// Write content to the terminal
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
                self.scrollback.push(
                    std::mem::take(&mut self.cells),
                );
                if self.scrollback.len() > self.config.scrollback_lines {
                    self.scrollback.remove(0);
                }
                self.cursor_row = self.grid.rows - 1;
            }
        }
        self.dirty = true;
    }
}