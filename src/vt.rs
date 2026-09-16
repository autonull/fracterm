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

/// Visible cursor shape (DECSCUSR, `CSI Ps SP q`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

/// Cursor style: shape + whether it blinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorStyle {
    pub shape: CursorShape,
    pub blink: bool,
}

impl CursorStyle {
    /// Decode a DECSCUSR parameter: 0/1 blinking block, 2 steady block,
    /// 3 blinking underline, 4 steady underline, 5 blinking bar, 6 steady
    /// bar. Unknown values keep the current style.
    pub fn from_decscusr(param: u16, current: Self) -> Self {
        match param {
            0 | 1 => Self {
                shape: CursorShape::Block,
                blink: true,
            },
            2 => Self {
                shape: CursorShape::Block,
                blink: false,
            },
            3 => Self {
                shape: CursorShape::Underline,
                blink: true,
            },
            4 => Self {
                shape: CursorShape::Underline,
                blink: false,
            },
            5 => Self {
                shape: CursorShape::Bar,
                blink: true,
            },
            6 => Self {
                shape: CursorShape::Bar,
                blink: false,
            },
            _ => current,
        }
    }
}

/// Mouse reporting modes requested by the child (xterm 1000/1002/1003 +
/// SGR 1006). 1000 = clicks, 1002 = +button drags, 1003 = +bare motion.
/// The host forwards events via [`MouseMode::encode`]; Shift-click bypasses
/// forwarding so window management stays reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseMode {
    /// Any of 1000/1002/1003 active: clicks (and maybe drags/motion) go to
    /// the child instead of the canvas.
    pub report: bool,
    /// SGR (1006) extended coordinates; otherwise legacy X10 encoding.
    pub sgr: bool,
    /// Button-drag motion wanted (1002, or implied by 1003).
    pub drag: bool,
    /// Bare (no-button) motion wanted (1003).
    pub any_motion: bool,
    p1000: bool,
    p1002: bool,
    p1003: bool,
}

impl MouseMode {
    fn recompute(&mut self) {
        self.report = self.p1000 || self.p1002 || self.p1003;
        self.drag = self.p1002 || self.p1003;
        self.any_motion = self.p1003;
    }

    /// Record a DECSET (`CSI ? Pm h`) / DECRST (`CSI ? Pm l`) mouse mode.
    /// Unknown modes are ignored; other DEC modes are handled elsewhere.
    pub fn set_mode(&mut self, mode: u16, set: bool) {
        match mode {
            1000 => self.p1000 = set,
            1002 => self.p1002 = set,
            1003 => self.p1003 = set,
            _ => return,
        }
        self.recompute();
    }

    /// Whether this event reaches the child. Presses, releases, and wheel
    /// go whenever reporting is on; motion needs 1002 (dragging) or 1003.
    pub fn wants(&self, report: &MouseReport) -> bool {
        if !self.report {
            return false;
        }
        match report.button {
            MouseButton::WheelUp | MouseButton::WheelDown => true,
            _ if report.release => true,
            _ if report.motion => {
                if report.dragging {
                    self.drag
                } else {
                    self.any_motion
                }
            }
            _ => true,
        }
    }

    /// Encode a mouse event for the child (`None` when [`wants`] is false):
    /// SGR (`CSI < Cb ; Cx ; Cy M/m`, 1-based cells) with ?1006, otherwise
    /// legacy X10 (`ESC [ M Cb Cx Cy`, +32 offsets clamped to 255).
    pub fn encode(&self, report: &MouseReport) -> Option<Vec<u8>> {
        if !self.wants(report) {
            return None;
        }
        let mut cb: u32 = match report.button {
            MouseButton::Left => {
                if report.release {
                    3
                } else {
                    0
                }
            }
            MouseButton::Middle => {
                if report.release {
                    3
                } else {
                    1
                }
            }
            MouseButton::Right => {
                if report.release {
                    3
                } else {
                    2
                }
            }
            MouseButton::WheelUp => 64,
            MouseButton::WheelDown => 65,
        };
        if report.shift {
            cb += 4;
        }
        if report.alt {
            cb += 8;
        }
        if report.ctrl {
            cb += 16;
        }
        if report.motion {
            cb += 32;
        }
        if self.sgr {
            let kind = if report.release { 'm' } else { 'M' };
            Some(format!("\x1b[<{cb};{};{}{kind}", report.col + 1, report.row + 1).into_bytes())
        } else {
            let byte = |v: u32| v.min(223) as u8 + 32;
            Some(vec![
                0x1b,
                b'[',
                b'M',
                byte(cb),
                byte(report.col + 1),
                byte(report.row + 1),
            ])
        }
    }
}

/// Which button a forwarded mouse event concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

/// One host-side mouse event, with 0-based cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseReport {
    pub button: MouseButton,
    pub col: u32,
    pub row: u32,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    /// Button release (wheel ignores this: it has no release).
    pub release: bool,
    /// Motion event; `dragging` says a button is held.
    pub motion: bool,
    pub dragging: bool,
}

impl MouseReport {
    /// Button press without modifiers.
    pub fn press(button: MouseButton, col: u32, row: u32) -> Self {
        Self {
            button,
            col,
            row,
            shift: false,
            alt: false,
            ctrl: false,
            release: false,
            motion: false,
            dragging: false,
        }
    }
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
    /// Cursor saved across the alternate screen (?1049h/?1049l).
    saved_cursor: Option<(u32, u32)>,
    /// Reply bytes owed to the child (e.g. Device Attributes answers).
    /// Drained by the window loop and written back to the PTY.
    pending_reply: Vec<u8>,
    /// DCS sequence being collected (`dcs_hook` → `dcs_put`* → `unhook`).
    dcs_action: Option<char>,
    dcs_plus: bool,
    dcs_payload: Vec<u8>,
    /// DECTCEM (?25): false while the child hides the cursor (vim, etc.).
    pub cursor_visible: bool,
    /// DECSCUSR cursor style (block/bar/underline + blink).
    pub cursor_style: CursorStyle,
    /// DECAWM (?7): automatic wrap at the right margin.
    pub auto_wrap: bool,
    /// Bracketed paste (?2004): pastes must be wrapped in
    /// `ESC[200~` … `ESC[201~` (see [`VtEngine::bracket_paste`]).
    pub bracketed_paste: bool,
    /// Mouse reporting requested by the child.
    pub mouse_mode: MouseMode,
}

impl VtEngine {
    pub fn new(term: Terminal) -> Self {
        Self {
            term,
            pen: Pen::default(),
            alt_cells: None,
            wide_spacer: None,
            title: String::new(),
            saved_cursor: None,
            pending_reply: Vec::new(),
            dcs_action: None,
            dcs_plus: false,
            dcs_payload: Vec::new(),
            cursor_visible: true,
            cursor_style: CursorStyle::default(),
            auto_wrap: true,
            bracketed_paste: false,
            mouse_mode: MouseMode::default(),
        }
    }

    /// Wrap pasted text for the child: when the child enabled bracketed
    /// paste (?2004), the paste is framed with `ESC[200~` … `ESC[201~` so
    /// shells treat it as one literal unit instead of keystrokes.
    pub fn bracket_paste(&self, text: &str) -> Vec<u8> {
        if self.bracketed_paste {
            let mut out = Vec::with_capacity(text.len() + 12);
            out.extend_from_slice(b"\x1b[200~");
            out.extend_from_slice(text.as_bytes());
            out.extend_from_slice(b"\x1b[201~");
            out
        } else {
            text.as_bytes().to_vec()
        }
    }

    /// Take bytes the terminal owes the child process.
    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending_reply)
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
            if self.auto_wrap {
                self.term.cursor_col = 0;
                self.linefeed();
            } else {
                // No autowrap (?7l): the cursor sticks at the last column
                // and new output overwrites it.
                self.term.cursor_col = self.term.grid.cols.saturating_sub(1);
            }
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
        intermediates: &[u8],
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
                let set = action == 'h';
                // DEC private modes (`CSI ? …`) vs. ANSI modes (`CSI …`):
                // only the `?` forms touch terminal state.
                if intermediates.contains(&b'?') {
                    for mode in p {
                        match mode {
                            7 => self.auto_wrap = set,
                            25 => self.cursor_visible = set,
                            1049 => self.set_alt_screen(set),
                            2004 => self.bracketed_paste = set,
                            1000 | 1002 | 1003 => self.mouse_mode.set_mode(mode, set),
                            1006 => self.mouse_mode.sgr = set,
                            _ => {}
                        }
                    }
                }
            }
            'q' => {
                // DECSCUSR (`CSI Ps SP q`): cursor style. The space
                // intermediate distinguishes it from XTVERSION (`CSI > 0 q`),
                // which asks for the terminal version string.
                if intermediates.contains(&b' ') {
                    let param = p.first().copied().unwrap_or(0);
                    self.cursor_style = CursorStyle::from_decscusr(param, self.cursor_style);
                } else if intermediates.contains(&b'>') {
                    self.pending_reply
                        .extend_from_slice(b"\x1bP>|fracterm 2.0.0\x1b\\");
                }
            }
            'u' => {
                // Kitty keyboard capability query (`CSI ? u`): answer "not
                // supported" instead of stalling the child on a timeout.
                if intermediates.contains(&b'?') {
                    self.pending_reply.extend_from_slice(b"\x1b[?0u");
                }
            }
            'n' => {
                // Device Status Report: `CSI 5 n` asks "are you OK?",
                // `CSI 6 n` asks for the cursor position. Prompts and
                // shells (fish among them) can stall their first render
                // until the position report arrives, so answer both.
                match p.first().copied().unwrap_or(0) {
                    5 => self.pending_reply.extend_from_slice(b"\x1b[0n"),
                    6 => {
                        let report = format!(
                            "\x1b[{};{}R",
                            self.term.cursor_row + 1,
                            self.term.cursor_col + 1
                        );
                        self.pending_reply.extend_from_slice(report.as_bytes());
                    }
                    _ => {}
                }
            }
            'c' => {
                // Device Attributes: `\e[0c` asks "what terminal are you?".
                // A real terminal always answers; fish holds its first
                // prompt until this reply arrives, so silence here means
                // a blank window for many seconds. Private (`\e[>0c`)
                // secondary-DA is ignored for now.
                if !intermediates.contains(&b'>') {
                    self.pending_reply.extend_from_slice(b"\x1b[?1;2c");
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
            } else if first == b"11" {
                // OSC 11 background query (`OSC 11;?`): answer with the
                // default background so the child never waits on a timeout.
                self.pending_reply
                    .extend_from_slice(b"\x1b]11;rgb:0b0b/0d0d/1212\x1b\\");
            }
        }
    }

    fn hook(&mut self, _params: &vte::Params, intermediates: &[u8], _ignore: bool, action: char) {
        self.dcs_action = Some(action);
        self.dcs_plus = intermediates.contains(&b'+');
        self.dcs_payload.clear();
    }

    fn put(&mut self, byte: u8) {
        self.dcs_payload.push(byte);
    }

    fn unhook(&mut self) {
        // XTGETTCAP (`DCS + q <hex> ST`): we expose no terminfo caps, so
        // answer "unknown" (`DCS 0 + r ST`) instead of stalling the child.
        // The `+` guard keeps Sixel payloads (also `q`-final) untouched.
        if self.dcs_action == Some('q') && self.dcs_plus && !self.dcs_payload.is_empty() {
            self.pending_reply.extend_from_slice(b"\x1bP0+r\x1b\\");
        }
        self.dcs_action = None;
        self.dcs_plus = false;
        self.dcs_payload.clear();
    }
}

impl VtEngine {
    fn set_alt_screen(&mut self, on: bool) {
        if on && self.alt_cells.is_none() {
            self.alt_cells = Some(self.term.grid.cells.clone());
            self.saved_cursor = Some((self.term.cursor_row, self.term.cursor_col));
            self.clear_screen();
            self.term.cursor_row = 0;
            self.term.cursor_col = 0;
        } else if !on {
            if let Some(alt) = self.alt_cells.take() {
                self.term.grid.cells = alt;
                // Restore the pre-alt cursor: parking at the
                // bottom row pushed the first prompt to the last
                // line and scrolled away row 0.
                let (r, c) = self.saved_cursor.take().unwrap_or((0, 0));
                self.term.cursor_row = r.min(self.term.grid.rows.saturating_sub(1));
                self.term.cursor_col = c.min(self.term.grid.cols.saturating_sub(1));
            }
        }
    }

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
    fn test_cursor_visibility_and_style() {
        let mut e = engine(20, 5);
        assert!(e.cursor_visible);
        e.feed(b"\x1b[?25l");
        assert!(!e.cursor_visible);
        e.feed(b"\x1b[?25h");
        assert!(e.cursor_visible);
        // DECSCUSR shapes.
        e.feed(b"\x1b[4 q");
        assert_eq!(e.cursor_style.shape, CursorShape::Underline);
        assert!(!e.cursor_style.blink);
        e.feed(b"\x1b[5 q");
        assert_eq!(e.cursor_style.shape, CursorShape::Bar);
        assert!(e.cursor_style.blink);
        e.feed(b"\x1b[0 q");
        assert_eq!(e.cursor_style.shape, CursorShape::Block);
        // A bare `q` without the space intermediate is not DECSCUSR.
        e.feed(b"\x1b[5q");
        assert_eq!(e.cursor_style.shape, CursorShape::Block);
    }

    #[test]
    fn test_bracketed_paste_mode_and_wrap() {
        let mut e = engine(20, 5);
        assert!(!e.bracketed_paste);
        assert_eq!(e.bracket_paste("hi"), b"hi");
        e.feed(b"\x1b[?2004h");
        assert!(e.bracketed_paste);
        assert_eq!(e.bracket_paste("hi"), b"\x1b[200~hi\x1b[201~");
        e.feed(b"\x1b[?2004l");
        assert!(!e.bracketed_paste);
    }

    #[test]
    fn test_autowrap_off_overwrites_last_column() {
        let mut e = engine(4, 2);
        e.feed(b"\x1b[?7l");
        assert!(!e.auto_wrap);
        e.feed(b"abcdef");
        assert_eq!(cell(&e, 0, 3).character, 'f');
        assert_eq!(e.term.cursor_row, 0);
        e.feed(b"\x1b[?7h");
        assert!(e.auto_wrap);
    }

    #[test]
    fn test_mouse_mode_tracking() {
        let mut e = engine(20, 5);
        assert!(!e.mouse_mode.report);
        e.feed(b"\x1b[?1000h\x1b[?1006h");
        assert!(e.mouse_mode.report);
        assert!(e.mouse_mode.sgr);
        assert!(!e.mouse_mode.drag);
        e.feed(b"\x1b[?1002h");
        assert!(e.mouse_mode.drag);
        assert!(!e.mouse_mode.any_motion);
        e.feed(b"\x1b[?1002l\x1b[?1000l");
        assert!(!e.mouse_mode.report);
        // 1003 implies drag; resetting 1003 clears both.
        e.feed(b"\x1b[?1003h");
        assert!(e.mouse_mode.report && e.mouse_mode.drag && e.mouse_mode.any_motion);
        e.feed(b"\x1b[?1003l");
        assert!(!e.mouse_mode.report && !e.mouse_mode.drag);
    }

    fn sgr_mouse() -> MouseMode {
        let mut m = MouseMode::default();
        m.set_mode(1000, true);
        m.set_mode(1006, false);
        m.sgr = true;
        m
    }

    #[test]
    fn test_mouse_sgr_press_release_wheel() {
        let m = sgr_mouse();
        assert_eq!(
            m.encode(&MouseReport::press(MouseButton::Left, 4, 9)),
            Some(b"\x1b[<0;5;10M".to_vec())
        );
        let mut rel = MouseReport::press(MouseButton::Left, 4, 9);
        rel.release = true;
        assert_eq!(m.encode(&rel), Some(b"\x1b[<3;5;10m".to_vec()));
        assert_eq!(
            m.encode(&MouseReport::press(MouseButton::WheelUp, 0, 0)),
            Some(b"\x1b[<64;1;1M".to_vec())
        );
        assert_eq!(
            m.encode(&MouseReport::press(MouseButton::Right, 79, 23)),
            Some(b"\x1b[<2;80;24M".to_vec())
        );
    }

    #[test]
    fn test_mouse_modifiers_and_motion_bits() {
        let mut m = sgr_mouse();
        m.set_mode(1002, true);
        let mut r = MouseReport::press(MouseButton::Left, 0, 0);
        r.shift = true;
        r.ctrl = true;
        assert_eq!(m.encode(&r), Some(b"\x1b[<20;1;1M".to_vec()));
        // Drag motion adds 32 and needs 1002.
        let drag = MouseReport {
            motion: true,
            dragging: true,
            ..MouseReport::press(MouseButton::Left, 1, 1)
        };
        assert_eq!(m.encode(&drag), Some(b"\x1b[<32;2;2M".to_vec()));
        // Bare hover needs 1003.
        let hover = MouseReport {
            motion: true,
            dragging: false,
            ..MouseReport::press(MouseButton::Left, 1, 1)
        };
        assert_eq!(m.encode(&hover), None);
        m.set_mode(1003, true);
        assert_eq!(m.encode(&hover), Some(b"\x1b[<32;2;2M".to_vec()));
    }

    #[test]
    fn test_mouse_gated_when_reporting_off() {
        let m = MouseMode::default();
        assert_eq!(m.encode(&MouseReport::press(MouseButton::Left, 0, 0)), None);
    }

    #[test]
    fn test_mouse_legacy_x10_clamps() {
        let mut m = MouseMode::default();
        m.set_mode(1000, true);
        assert!(!m.sgr);
        assert_eq!(
            m.encode(&MouseReport::press(MouseButton::Left, 0, 0)),
            Some(vec![0x1b, b'[', b'M', 32, 33, 33])
        );
        // Far coordinates saturate at 255 instead of wrapping.
        assert_eq!(
            m.encode(&MouseReport::press(MouseButton::Left, 300, 300)),
            Some(vec![0x1b, b'[', b'M', 32, 255, 255])
        );
    }

    #[test]
    fn test_dsr_status_and_cursor_report() {
        let mut e = engine(80, 24);
        e.feed(b"\x1b[5n");
        assert_eq!(e.take_reply(), b"\x1b[0n");
        e.feed(b"\x1b[3;7H\x1b[6n");
        assert_eq!(e.take_reply(), b"\x1b[3;7R");
        assert!(e.take_reply().is_empty());
    }

    #[test]
    fn test_terminal_queries_answered() {
        // fish's startup burst must never stall on a timeout: kitty,
        // XTVERSION, background-color, and terminfo queries all get
        // immediate replies.
        let mut e = engine(80, 24);
        e.feed(b"\x1b[?u");
        assert_eq!(e.take_reply(), b"\x1b[?0u");
        e.feed(b"\x1b[>0q");
        assert_eq!(e.take_reply(), b"\x1bP>|fracterm 2.0.0\x1b\\");
        e.feed(b"\x1b]11;?\x1b\\");
        assert_eq!(e.take_reply(), b"\x1b]11;rgb:0b0b/0d0d/1212\x1b\\");
        // Exact bytes of fish's first-prompt XTGETTCAP probe.
        e.feed(b"\x1bP+q696e646e\x1b\\");
        assert_eq!(e.take_reply(), b"\x1bP0+r\x1b\\");
        // Sixel payloads (also `q`-final, no `+`) stay untouched.
        e.feed(b"\x1bPq#0;2;0;0;0~-~\x1b\\");
        assert!(e.take_reply().is_empty());
        // A bare DECSCUSR `q` (space intermediate) still styles the cursor.
        e.feed(b"\x1b[2 q");
        assert_eq!(e.cursor_style.shape, CursorShape::Block);
        assert!(e.take_reply().is_empty());
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

    #[test]
    fn test_da_query_answered() {
        // fish holds its first prompt until Primary DA is answered.
        let mut e = engine(80, 24);
        e.feed(b"\x1b[0c");
        assert_eq!(e.take_reply(), b"\x1b[?1;2c");
        assert!(e.take_reply().is_empty());
        // Secondary DA (`>`) stays silent.
        e.feed(b"\x1b[>0c");
        assert!(e.take_reply().is_empty());
    }

    #[test]
    fn test_colored_cells_preserved() {
        // Colored characters must land in the grid with their colors;
        // color must never drop a cell.
        let mut e = engine(80, 24);
        e.feed(b"\x1b[31mR\x1b[32mG\x1b[90mB\x1b[38;5;200mC\x1b[38;2;10;20;30mD\x1b[m*");
        assert_eq!(cell(&e, 0, 0).character, 'R');
        assert_eq!(cell(&e, 0, 0).fg, super::PALETTE[1]);
        assert_eq!(cell(&e, 0, 1).character, 'G');
        assert_eq!(cell(&e, 0, 1).fg, super::PALETTE[2]);
        assert_eq!(cell(&e, 0, 2).character, 'B');
        assert_eq!(cell(&e, 0, 2).fg, super::PALETTE[8]);
        assert_eq!(cell(&e, 0, 3).character, 'C');
        assert_eq!(cell(&e, 0, 3).fg, super::palette_256(200));
        assert_eq!(cell(&e, 0, 4).character, 'D');
        assert_eq!(cell(&e, 0, 4).fg, crate::surface::Color::rgb(10, 20, 30));
        assert_eq!(cell(&e, 0, 5).character, '*');
        assert_eq!(
            cell(&e, 0, 5).fg,
            crate::surface::Color::rgb(0xdf, 0xe3, 0xee)
        );
    }
}
