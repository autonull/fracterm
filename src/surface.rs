//! Surface - source of visual/textual content.

use super::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Types of surfaces in Fracterm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceType {
    /// Terminal surface from a PTY process
    Terminal,
    /// Plain text view surface
    TextView,
    /// Plugin widget surface
    Widget,
    /// Group surface containing multiple nodes
    Group,
    /// Reading pane surface
    Reading,
}

/// A surface is a source of visual/textual content in the workspace.
pub struct Surface {
    /// Unique identifier
    pub id: SurfaceId,
    /// Type of surface
    pub surface_type: SurfaceType,
    /// Content width in columns
    pub columns: u32,
    /// Content height in rows
    pub rows: u32,
    /// Whether the surface is visible
    pub visible: bool,
    /// Opacity from 0.0 to 1.0
    pub opacity: f64,
    /// Content data stored as Rc for sharing
    pub data: Rc<RefCell<SurfaceData>>,
}

/// The actual data stored in a surface
#[derive(Debug, Default)]
pub struct SurfaceData {
    /// Grid of cells indexed by [row][col]
    pub grid: Vec<Vec<Cell>>,
    /// Title of the surface
    pub title: String,
    /// Whether output is synchronized
    pub synchronized: bool,
    /// Timestamp of last update
    pub last_updated: i64,
}

/// A single cell in the terminal grid
#[derive(Debug, Clone)]
pub struct Cell {
    /// The character in this cell
    pub ch: char,
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Whether this cell is bold
    pub bold: bool,
    /// Whether this cell is italic
    pub italic: bool,
    /// Whether this cell is underlined
    pub underline: bool,
    /// Whether this cell has reverse video
    pub reverse: bool,
    /// Character width (1 for narrow, 2 for wide)
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::from_hex("#dfe3ee"),
            bg: Color::from_hex("#0b0d12"),
            bold: false,
            italic: false,
            underline: false,
            reverse: false,
            width: 1,
        }
    }
}

/// RGBA color representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn hex_digit(c: u8) -> u8 {
        if c.is_ascii_digit() {
            c - b'0'
        } else {
            (c | 0x20) - b'a' + 10
        }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn from_hex_const(hex: &'static str) -> Self {
        let b = hex.as_bytes();
        Self {
            r: Self::hex_digit(b[0]) * 16 + Self::hex_digit(b[1]),
            g: Self::hex_digit(b[2]) * 16 + Self::hex_digit(b[3]),
            b: Self::hex_digit(b[4]) * 16 + Self::hex_digit(b[5]),
            a: 255,
        }
    }

    pub fn from_hex(hex: &str) -> Self {
        let hex = hex.trim_start_matches('#');
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        let a = if hex.len() > 6 {
            u8::from_str_radix(&hex[6..8], 16).unwrap_or(255)
        } else {
            255
        };
        Self { r, g, b, a }
    }
}

impl Surface {
    /// Create a new surface
    pub fn new(id: SurfaceId, surface_type: SurfaceType, columns: u32, rows: u32) -> Self {
        let grid = vec![vec![Cell::default(); columns as usize]; rows as usize];
        Self {
            id,
            surface_type,
            columns,
            rows,
            visible: true,
            opacity: 1.0,
            data: Rc::new(RefCell::new(SurfaceData {
                grid,
                title: String::new(),
                synchronized: false,
                last_updated: 0,
            })),
        }
    }

    /// Get the width in columns
    pub fn columns(&self) -> u32 {
        self.columns
    }

    /// Get the height in rows
    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// Set the title of the surface
    pub fn set_title(&mut self, title: &str) {
        self.data.borrow_mut().title = title.to_string();
    }

    /// Get the title
    pub fn title(&self) -> String {
        self.data.borrow().title.clone()
    }

    /// Mark the surface as needing an update
    pub fn mark_dirty(&self) {
        self.data.borrow_mut().last_updated = chrono::Utc::now().timestamp();
    }
}
