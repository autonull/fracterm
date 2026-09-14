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
#[derive(Debug, Clone, Default)]
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

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
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
    pub fn title(&self) -> &str {
        &self.data.borrow().title
    }

    /// Mark the surface as needing an update
    pub fn mark_dirty(&self) {
        self.data.borrow_mut().last_updated = chrono::Utc::now().timestamp();
    }
}