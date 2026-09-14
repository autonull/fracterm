//! Theme - theming for the Fracterm workspace.

use super::*;

/// Color theme for the workspace
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background color
    pub background: Color,
    /// Foreground/text color
    pub foreground: Color,
    /// Cursor color
    pub cursor: Color,
    /// Selection color
    pub selection: Color,
    /// Bright accent color
    pub accent: Color,
    /// Border color
    pub border: Color,
    /// HUD background
    pub hud_background: Color,
    /// HUD foreground
    pub hud_foreground: Color,
    /// High contrast mode
    pub high_contrast: bool,
}

impl Theme {
    /// Create a default dark theme
    pub fn default() -> Self {
        Self {
            background: Color::from_hex("#0b0d12"),
            foreground: Color::from_hex("#dfe3ee"),
            cursor: Color::from_hex("#82aaff"),
            selection: Color::from_hex("#2d3a55"),
            accent: Color::from_hex("#82aaff"),
            border: Color::from_hex("#1a1f2b"),
            hud_background: Color::from_hex("#101018"),
            hud_foreground: Color::from_hex("#e8e8f0"),
            high_contrast: false,
        }
    }

    /// Create a light theme
    pub fn light() -> Self {
        Self {
            background: Color::from_hex("#ffffff"),
            foreground: Color::from_hex("#000000"),
            cursor: Color::from_hex("#0055ff"),
            selection: Color::from_hex("#b3d4ff"),
            accent: Color::from_hex("#0055ff"),
            border: Color::from_hex("#cccccc"),
            hud_background: Color::from_hex("#f0f0f0"),
            hud_foreground: Color::from_hex("#000000"),
            high_contrast: false,
        }
    }

    /// Create a high contrast theme
    pub fn high_contrast() -> Self {
        Self {
            background: Color::from_hex("#000000"),
            foreground: Color::from_hex("#ffffff"),
            cursor: Color::from_hex("#ffffff"),
            selection: Color::from_hex("#444444"),
            accent: Color::from_hex("#ffff00"),
            border: Color::from_hex("#666666"),
            hud_background: Color::from_hex("#111111"),
            hud_foreground: Color::from_hex("#ffffff"),
            high_contrast: true,
        }
    }

    /// Toggle high contrast mode
    pub fn toggle_high_contrast(&mut self) {
        self.high_contrast = !self.high_contrast;
        if self.high_contrast {
            self.background = Color::from_hex("#000000");
            self.foreground = Color::from_hex("#ffffff");
        }
    }
}