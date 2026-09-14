//! VT/ANSI parser engine (README M3): feeds PTY bytes through `vte` into the
//! terminal grid with SGR color, cursor control, scrollback, and alt screen.

use crate::surface::Color;
use crate::terminal::Terminal;

/// 16-color ANSI palette.
pub const PALETTE: [Color; 16] = [
    Color::rgb(0, 0, 0),
    Color::rgb(205, 49, 49),
    Color::rgb(13, 188, 121),
    Color::rgb(229, 229, 16),
    Color::rgb(36, 114, 200),
    Color::rgb(188, 63, 188),
    Color::rgb(17, 168, 205),
    Color::rgb(229, 229, 229),
    Color::rgb(102, 102, 102),
    Color::rgb(241, 76, 76),
    Color::rgb(35, 209, 139),
    Color::rgb(245, 245, 67),
    Color::rgb(59, 142, 234),
    Color::rgb(214, 112, 214),
    Color::rgb(41, 184, 219),
    Color::rgb(229, 229, 229),
];

const DEFAULT_FG: Color = Color::rgb(0xdf, 0xe3, 0xee);
const DEFAULT_BG: Color = Color::rgb(0x0b, 0x0d, 0x12);

/// Active pen state applied to newly written cells.
#[derive(Debug, Clone, Default)]
pub struct Pen {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

/// `vte::Perform` implementation driving a [`Terminal`] grid.
pub struct VtEngine {
    pub term: Terminal,
    pub pen: Pen,
    /// Alternate screen buffer (None = main screen active).
    pub alt_cells: Option<Vec<Vec<crate::terminal::TerminalCell>>>,
    /// Position of the spacer cell for the last wide char, filled lazily.
    wide_spacer: Option<(u32, u32)>,
    /// Window title from OSC 0/2 sequences.
    pub title: String,
}

impl VtEngine {
    pub fn new(term: Terminal) -> Self {
        Self {
            term,
            pen: Pen::default(),
            alt_cells: None,
            wide_spacer: None,
            title: String::new(),
        }
    }

    /// Feed raw PTY output through the parser.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut parser = vte::Parser::new();
        for &byte in bytes {
            parser.advance(self, byte);
        }
        self.term.dirty = true;
    }

    fn cell_colors(&self) -> (Color, Color) {
        let mut fg = self.pen.fg.unwrap_or(DEFAULT_FG);
        let mut bg = self.pen.bg.unwrap_or(DEFAULT_BG);
        if self.pen.reverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        (fg, bg)
    }

    fn linefeed(&mut self) {
        self.term.cursor_row += 1;
        if self.term.cursor_row >= self.term.grid.rows {
            self.scroll_up(1);
            self.term.cursor_row = self.term.grid.rows.saturating_sub(1);
        }
    }

    fn scroll_up(&mut self, n: usize) {
        let cols = self.term.grid.cols as usize;
        for _ in 0..n {
            let first = self.term.grid.cells.remove(0);
            self.term.grid.scrollback.push(first);
            if self.term.grid.scrollback.len() > self.term.config.scrollback_lines {
                self.term.grid.scrollback.remove(0);
            }
            self.term
                .grid
                .cells
                .push(vec![crate::terminal::TerminalCell::default(); cols]);
        }
    }

    fn clear_row_from(&mut self, row: u32, col: u32) {
        let (fg, bg) = self.cell_colors();
        for c in col..self.term.grid.cols {
            self.term.grid.set(
                row,
                c,
                crate::terminal::TerminalCell {
                    character: ' ',
                    fg,
                    bg,
                    ..crate::terminal::TerminalCell::default()
                },
            );
        }
    }

    fn put_char(&mut self, ch: char) {
        let (fg, bg) = self.cell_colors();
        let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1) as u8;
        if self.term.cursor_col >= self.term.grid.cols {
            self.term.cursor_col = 0;
            self.linefeed();
        }
        // Fill the spacer cell left of the current char if a wide char
        // advanced the cursor past it earlier.
        if let Some((sr, sc)) = self.wide_spacer.take() {
            if (sr, sc) != (self.term.cursor_row, self.term.cursor_col) {
                self.term.grid.set(
                    sr,
                    sc,
                    crate::terminal::TerminalCell {
                        character: ' ',
                        fg,
                        bg,
                        width: 0,
                        ..crate::terminal::TerminalCell::default()
                    },
                );
            }
        }
        let (row, col) = (self.term.cursor_row, self.term.cursor_col);
        self.term.grid.set(
            row,
            col,
            crate::terminal::TerminalCell {
                character: ch,
                fg,
                bg,
                bold: self.pen.bold,
                italic: self.pen.italic,
                underline: self.pen.underline,
                reverse: self.pen.reverse,
                width,
            },
        );
        self.term.cursor_col += width as u32;
        if width == 2 {
            self.wide_spacer = Some((row, col + 1));
        }
    }

    fn apply_sgr(&mut self, params: &[u16]) {
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.pen = Pen::default(),
                1 => self.pen.bold = true,
                3 => self.pen.italic = true,
                4 => self.pen.underline = true,
                7 => self.pen.reverse = true,
                22 => {
                    self.pen.bold = false;
                }
                23 => self.pen.italic = false,
                24 => self.pen.underline = false,
                27 => self.pen.reverse = false,
                30..=37 => self.pen.fg = Some(PALETTE[(params[i] - 30) as usize]),
                39 => self.pen.fg = None,
                40..=47 => self.pen.bg = Some(PALETTE[(params[i] - 40) as usize]),
                49 => self.pen.bg = None,
                90..=97 => self.pen.fg = Some(PALETTE[8 + (params[i] - 90) as usize]),
                100..=107 => self.pen.bg = Some(PALETTE[8 + (params[i] - 100) as usize]),
                38 | 48 => {
                    let color = self.parse_extended_color(&params[i..]);
                    if let Some(c) = color {
                        if params[i] == 38 {
                            self.pen.fg = Some(c);
                        } else {
                            self.pen.bg = Some(c);
                        }
                        // Skip consumed params: 38;5;N or 38;2;R;G;B
                        i += if params.get(i + 1) == Some(&5) { 2 } else { 4 };
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn parse_extended_color(&self, params: &[u16]) -> Option<Color> {
        match params.get(1)? {
            5 => {
                let idx = *params.get(2)? as u8;
                Some(palette_256(idx))
            }
            2 => Some(Color::rgb(
                params.get(2).copied().unwrap_or(0) as u8,
                params.get(3).copied().unwrap_or(0) as u8,
                params.get(4).copied().unwrap_or(0) as u8,
            )),
            _ => None,
        }
    }
}

/// 256-color palette: 16 base + 6x6x6 cube + 24 grayscale.
pub fn palette_256(idx: u8) -> Color {
    if idx < 16 {
        return PALETTE[idx as usize];
    }
    if idx < 232 {
        let i = idx - 16;
        let r = (i / 36) * 51;
        let g = ((i % 36) / 6) * 51;
        let b = (i % 6) * 51;
        return Color::rgb(r, g, b);
    }
    let gray = 8u8 + (idx - 232) * 10;
    Color::rgb(gray, gray, gray)
}

impl vte::Perform for VtEngine {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.term.cursor_col = 0,
            b'\n' | 0x0b | 0x0c => self.linefeed(),
            b'\t' => {
                self.term.cursor_col = (self.term.cursor_col / 8 + 1) * 8;
                if self.term.cursor_col >= self.term.grid.cols {
                    self.term.cursor_col = self.term.grid.cols.saturating_sub(1);
                }
            }
            0x08 => self.term.cursor_col = self.term.cursor_col.saturating_sub(1),
            0x07 => {} // BEL
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let p: Vec<u16> = params
            .iter()
            .map(|g| g.first().copied().unwrap_or(0))
            .collect();
        let p0u =
            |i: usize, d: u16| -> u32 { p.get(i).copied().filter(|v| *v != 0).unwrap_or(d) as u32 };
        match action {
            'H' | 'f' => {
                self.term.cursor_row = p0u(0, 1)
                    .saturating_sub(1)
                    .min(self.term.grid.rows.saturating_sub(1));
                self.term.cursor_col = p0u(1, 1)
                    .saturating_sub(1)
                    .min(self.term.grid.cols.saturating_sub(1));
            }
            'A' => {
                self.term.cursor_row = self.term.cursor_row.saturating_sub(p0u(0, 1));
            }
            'B' | 'e' => {
                self.term.cursor_row =
                    (self.term.cursor_row + p0u(0, 1)).min(self.term.grid.rows.saturating_sub(1));
            }
            'C' | 'a' => {
                self.term.cursor_col =
                    (self.term.cursor_col + p0u(0, 1)).min(self.term.grid.cols.saturating_sub(1));
            }
            'D' => {
                self.term.cursor_col = self.term.cursor_col.saturating_sub(p0u(0, 1));
            }
            'E' => {
                self.term.cursor_row =
                    (self.term.cursor_row + p0u(0, 1)).min(self.term.grid.rows.saturating_sub(1));
                self.term.cursor_col = 0;
            }
            'F' => {
                self.term.cursor_row = self.term.cursor_row.saturating_sub(p0u(0, 1));
                self.term.cursor_col = 0;
            }
            'G' | '`' => {
                self.term.cursor_col = p0u(0, 1)
                    .saturating_sub(1)
                    .min(self.term.grid.cols.saturating_sub(1));
            }
            'd' => {
                self.term.cursor_row = p0u(0, 1)
                    .saturating_sub(1)
                    .min(self.term.grid.rows.saturating_sub(1));
            }
            'J' => match p.first().copied().unwrap_or(0) {
                0 => {
                    self.clear_row_from(self.term.cursor_row, self.term.cursor_col);
                    for r in (self.term.cursor_row + 1)..self.term.grid.rows {
                        self.clear_row_from(r, 0);
                    }
                }
                1 => {
                    for r in 0..self.term.cursor_row {
                        self.clear_row_from(r, 0);
                    }
                    self.clear_row_from(self.term.cursor_row, 0);
                }
                _ => {
                    for r in 0..self.term.grid.rows {
                        self.clear_row_from(r, 0);
                    }
                }
            },
            'K' => match p.first().copied().unwrap_or(0) {
                0 => self.clear_row_from(self.term.cursor_row, self.term.cursor_col),
                1 => self.clear_row_from(self.term.cursor_row, 0),
                _ => self.clear_row_from(self.term.cursor_row, 0),
            },
            'm' => {
                if p.is_empty() {
                    self.pen = Pen::default();
                } else {
                    self.apply_sgr(&p);
                }
            }
            'h' | 'l' => {
                // ?25 cursor visibility, ?1049 alt screen, ?2004 bracketed paste
                if p.contains(&1049) {
                    if action == 'h' && self.alt_cells.is_none() {
                        self.alt_cells = Some(self.term.grid.cells.clone());
                        self.clear_screen();
                    } else if action == 'l' {
                        if let Some(alt) = self.alt_cells.take() {
                            self.term.grid.cells = alt;
                            self.term.cursor_row = self.term.grid.rows.saturating_sub(1);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 0;title / OSC 2;title set the window title.
        if let Some(first) = params.first() {
            if (first == b"0" || first == b"2") && params.len() > 1 {
                self.title = String::from_utf8_lossy(params[1]).into_owned();
            }
        }
    }
}

impl VtEngine {
    fn clear_screen(&mut self) {
        for r in 0..self.term.grid.rows {
            self.clear_row_from(r, 0);
        }
        self.term.cursor_row = 0;
        self.term.cursor_col = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{TerminalCell, TerminalConfig};

    fn engine(cols: u32, rows: u32) -> VtEngine {
        let term = Terminal {
            id: crate::SurfaceId(1),
            grid: crate::terminal::TerminalGrid::new(rows, cols),
            config: TerminalConfig::default(),
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            dirty: false,
        };
        VtEngine::new(term)
    }

    fn cell(e: &VtEngine, row: u32, col: u32) -> TerminalCell {
        e.term.grid.get(row, col).unwrap().clone()
    }

    #[test]
    fn test_plain_text() {
        let mut e = engine(20, 5);
        e.feed(b"hello");
        assert_eq!(cell(&e, 0, 0).character, 'h');
        assert_eq!(cell(&e, 0, 4).character, 'o');
        assert_eq!(e.term.cursor_col, 5);
    }

    #[test]
    fn test_newline_and_scrollback() {
        let mut e = engine(10, 3);
        e.feed(b"one\r\ntwo\r\nthree\r\nfour");
        assert_eq!(cell(&e, 0, 0).character, 't'); // scrolled
        assert_eq!(e.term.grid.scrollback.len(), 1);
        assert_eq!(e.term.grid.scrollback[0][0].character, 'o');
    }

    #[test]
    fn test_sgr_colors() {
        let mut e = engine(20, 5);
        e.feed(b"\x1b[1;31mR\x1b[0mn");
        assert_eq!(cell(&e, 0, 0).character, 'R');
        assert_eq!(cell(&e, 0, 0).fg, PALETTE[1]);
        assert!(cell(&e, 0, 0).bold);
        assert_eq!(cell(&e, 0, 1).character, 'n');
        assert_eq!(cell(&e, 0, 1).fg, DEFAULT_FG);
    }

    #[test]
    fn test_truecolor() {
        let mut e = engine(20, 5);
        e.feed(b"\x1b[38;2;12;34;56mX");
        assert_eq!(cell(&e, 0, 0).fg, Color::rgb(12, 34, 56));
    }

    #[test]
    fn test_256_color() {
        let mut e = engine(20, 5);
        e.feed(b"\x1b[48;5;196mX");
        assert_eq!(cell(&e, 0, 0).bg, palette_256(196));
    }

    #[test]
    fn test_cursor_moves_and_clear() {
        let mut e = engine(20, 5);
        e.feed(b"abc\x1b[2D");
        assert_eq!(e.term.cursor_col, 1);
        e.feed(b"\x1b[K");
        assert_eq!(cell(&e, 0, 1).character, ' ');
        assert_eq!(cell(&e, 0, 0).character, 'a');
    }

    #[test]
    fn test_cup() {
        let mut e = engine(20, 5);
        e.feed(b"\x1b[3;5HX");
        assert_eq!(e.term.cursor_row, 2);
        assert_eq!(e.term.cursor_col, 5);
        assert_eq!(cell(&e, 2, 4).character, 'X');
    }

    #[test]
    fn test_reverse_video() {
        let mut e = engine(20, 5);
        e.feed(b"\x1b[7mR\x1b[27mN");
        let r = cell(&e, 0, 0);
        assert!(r.reverse);
        let n = cell(&e, 0, 1);
        assert!(!n.reverse);
    }

    #[test]
    fn test_alt_screen() {
        let mut e = engine(20, 5);
        e.feed(b"main");
        e.feed(b"\x1b[?1049h");
        e.feed(b"alt");
        assert_eq!(cell(&e, 0, 0).character, 'a');
        e.feed(b"\x1b[?1049l");
        assert_eq!(cell(&e, 0, 0).character, 'm');
    }

    #[test]
    fn test_tab_stops() {
        let mut e = engine(40, 5);
        e.feed(b"a\tb");
        assert_eq!(e.term.cursor_col, 9);
        assert_eq!(cell(&e, 0, 8).character, 'b');
    }

    #[test]
    fn test_osc_title() {
        let mut e = engine(20, 5);
        e.feed(b"\x1b]2;my-title\x07");
        assert_eq!(e.title, "my-title");
        e.feed(b"\x1b]0;root-title\x1b\\");
        assert_eq!(e.title, "root-title");
    }

    #[test]
    fn test_wide_char() {
        let mut e = engine(20, 5);
        e.feed("\u{4e16}".as_bytes()); // CJK wide char
        assert_eq!(cell(&e, 0, 0).width, 2);
        assert_eq!(e.term.cursor_col, 2);
        e.feed("x".as_bytes());
        assert_eq!(cell(&e, 0, 1).width, 0); // spacer filled lazily
        assert_eq!(cell(&e, 0, 2).character, 'x');
    }
}
