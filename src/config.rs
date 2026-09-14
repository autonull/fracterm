//! Configuration system - TypeScript-first configuration.

use std::collections::HashMap;

/// Font configuration
#[derive(Debug, Clone)]
pub struct FontConfig {
    pub family: String,
    pub size: u32,
    pub ligatures: bool,
    pub weight: u32,
}

impl FontConfig {
    pub fn default() -> Self {
        Self {
            family: "JetBrains Mono".to_string(),
            size: 14,
            ligatures: true,
            weight: 400,
        }
    }
}

/// Theme configuration
#[derive(Debug, Clone)]
pub struct ThemeConfig {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
}

impl ThemeConfig {
    pub fn default() -> Self {
        Self {
            background: "#0b0d12".to_string(),
            foreground: "#dfe3ee".to_string(),
            cursor: "#82aaff".to_string(),
            selection: "#2d3a55".to_string(),
        }
    }
}

/// Camera configuration
#[derive(Debug, Clone)]
pub struct CameraConfig {
    pub wheel_zoom_speed: f64,
    pub auto_zoom_animation_ms: u64,
    pub easing: String,
}

impl CameraConfig {
    pub fn default() -> Self {
        Self {
            wheel_zoom_speed: 1.0,
            auto_zoom_animation_ms: 250,
            easing: "cubic-out".to_string(),
        }
    }
}

/// Effects configuration
#[derive(Debug, Clone)]
pub struct EffectsConfig {
    pub motion_blur: bool,
    pub background_blur: bool,
}

impl EffectsConfig {
    pub fn default() -> Self {
        Self {
            motion_blur: false,
            background_blur: false,
        }
    }
}

/// Input configuration
#[derive(Debug, Clone)]
pub struct InputConfig {
    pub wheel: String,
    pub right_click: String,
    pub right_drag: String,
    pub ctrl_right_click: String,
    pub alt_drag: String,
}

impl InputConfig {
    pub fn default() -> Self {
        Self {
            wheel: "zoom".to_string(),
            right_click: "autozoom".to_string(),
            right_drag: "rectangle-zoom".to_string(),
            ctrl_right_click: "context-menu".to_string(),
            alt_drag: "move-object".to_string(),
        }
    }
}

/// Terminal configuration
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    pub scrollback_lines: usize,
    pub copy_on_select: bool,
    pub ambiguous_width: u8,
}

impl TerminalConfig {
    pub fn default() -> Self {
        Self {
            scrollback_lines: 10000,
            copy_on_select: false,
            ambiguous_width: 1,
        }
    }
}

/// HUD configuration
#[derive(Debug, Clone)]
pub struct HudConfig {
    pub auto_hide: bool,
    pub edge: String,
    pub hotkey: String,
}

impl HudConfig {
    pub fn default() -> Self {
        Self {
            auto_hide: true,
            edge: "top-left".to_string(),
            hotkey: "Ctrl+Shift+H".to_string(),
        }
    }
}

/// Accessibility configuration
#[derive(Debug, Clone)]
pub struct AccessibilityConfig {
    pub font_size: u32,
    pub line_height: f64,
    pub high_contrast: bool,
    pub hide_chrome: bool,
}

impl AccessibilityConfig {
    pub fn default() -> Self {
        Self {
            font_size: 28,
            line_height: 1.6,
            high_contrast: true,
            hide_chrome: true,
        }
    }
}

/// Main configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub font: FontConfig,
    pub theme: ThemeConfig,
    pub camera: CameraConfig,
    pub effects: EffectsConfig,
    pub input: InputConfig,
    pub terminal: TerminalConfig,
    pub hud: HudConfig,
    pub accessibility: AccessibilityConfig,
    pub profiles: HashMap<String, TerminalConfig>,
}

impl Config {
    pub fn default() -> Self {
        Self {
            font: FontConfig::default(),
            theme: ThemeConfig::default(),
            camera: CameraConfig::default(),
            effects: EffectsConfig::default(),
            input: InputConfig::default(),
            terminal: TerminalConfig::default(),
            hud: HudConfig::default(),
            accessibility: AccessibilityConfig::default(),
            profiles: HashMap::new(),
        }
    }
}