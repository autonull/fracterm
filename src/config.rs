//! Configuration system - TypeScript-first configuration with profile support.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Font configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub family: String,
    pub size: u32,
    pub ligatures: bool,
    pub weight: u32,
}

impl FontConfig {
    pub fn new() -> Self {
        Self {
            family: "JetBrains Mono".to_string(),
            size: 14,
            ligatures: true,
            weight: 400,
        }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
}

impl ThemeConfig {
    pub fn new() -> Self {
        Self {
            background: "#0b0d12".to_string(),
            foreground: "#dfe3ee".to_string(),
            cursor: "#82aaff".to_string(),
            selection: "#2d3a55".to_string(),
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Camera configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    pub wheel_zoom_speed: f64,
    pub auto_zoom_animation_ms: u64,
    pub easing: String,
}

impl CameraConfig {
    pub fn new() -> Self {
        Self {
            wheel_zoom_speed: 1.0,
            auto_zoom_animation_ms: 250,
            easing: "cubic-out".to_string(),
        }
    }
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Effects configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectsConfig {
    pub motion_blur: bool,
    pub background_blur: bool,
}

impl EffectsConfig {
    pub fn new() -> Self {
        Self {
            motion_blur: false,
            background_blur: false,
        }
    }
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Input configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    pub wheel: String,
    pub right_click: String,
    pub right_drag: String,
    pub ctrl_right_click: String,
    pub alt_drag: String,
}

impl InputConfig {
    pub fn new() -> Self {
        Self {
            wheel: "zoom".to_string(),
            right_click: "autozoom".to_string(),
            right_drag: "rectangle-zoom".to_string(),
            ctrl_right_click: "context-menu".to_string(),
            alt_drag: "move-object".to_string(),
        }
    }
}

impl Default for InputConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Terminal configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    pub scrollback_lines: usize,
    pub copy_on_select: bool,
    pub ambiguous_width: u8,
}

impl TerminalConfig {
    pub fn new() -> Self {
        Self {
            scrollback_lines: 10000,
            copy_on_select: false,
            ambiguous_width: 1,
        }
    }

    pub fn profile(_name: &str, _font_size: u32) -> Self {
        Self {
            scrollback_lines: 10000,
            copy_on_select: false,
            ambiguous_width: 1,
        }
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// HUD configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HudConfig {
    pub auto_hide: bool,
    pub edge: String,
    pub hotkey: String,
}

impl HudConfig {
    pub fn new() -> Self {
        Self {
            auto_hide: true,
            edge: "top-left".to_string(),
            hotkey: "Ctrl+Shift+H".to_string(),
        }
    }
}

impl Default for HudConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Accessibility configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityConfig {
    pub reading_mode: ReadingModeConfig,
    pub cursor: CursorConfig,
    pub reduce_motion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingModeConfig {
    pub font_size: u32,
    pub line_height: f64,
    pub high_contrast: bool,
    pub hide_chrome: bool,
}

impl ReadingModeConfig {
    pub fn new() -> Self {
        Self {
            font_size: 28,
            line_height: 1.6,
            high_contrast: true,
            hide_chrome: true,
        }
    }
}

impl Default for ReadingModeConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorConfig {
    pub size: String,
    pub color: String,
}

impl CursorConfig {
    pub fn new() -> Self {
        Self {
            size: "large".to_string(),
            color: "#ffffff".to_string(),
        }
    }
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessibilityConfig {
    pub fn new() -> Self {
        Self {
            reading_mode: ReadingModeConfig::new(),
            cursor: CursorConfig::new(),
            reduce_motion: false,
        }
    }
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Profile configuration (overrides base config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub font: Option<FontConfig>,
    pub theme: Option<ThemeConfig>,
    pub terminal: Option<TerminalConfig>,
    pub accessibility: Option<AccessibilityConfig>,
    pub effects: Option<EffectsConfig>,
}

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub font: FontConfig,
    pub theme: ThemeConfig,
    pub camera: CameraConfig,
    pub effects: EffectsConfig,
    pub input: InputConfig,
    pub terminal: TerminalConfig,
    pub hud: HudConfig,
    pub accessibility: AccessibilityConfig,
    pub profiles: HashMap<String, ProfileConfig>,
}

impl Config {
    pub fn new() -> Self {
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

    /// Apply a profile on top of base config
    pub fn apply_profile(&mut self, profile_name: &str) {
        if let Some(profile) = self.profiles.get(profile_name) {
            if let Some(font) = &profile.font {
                self.font = font.clone();
            }
            if let Some(theme) = &profile.theme {
                self.theme = theme.clone();
            }
            if let Some(terminal) = &profile.terminal {
                self.terminal = terminal.clone();
            }
            if let Some(accessibility) = &profile.accessibility {
                self.accessibility = accessibility.clone();
            }
            if let Some(effects) = &profile.effects {
                self.effects = effects.clone();
            }
        }
    }

    /// Create a new profile from current config and add it to profiles
    pub fn create_profile(&mut self, name: &str) {
        let profile = ProfileConfig {
            font: Some(self.font.clone()),
            theme: Some(self.theme.clone()),
            terminal: Some(self.terminal.clone()),
            accessibility: Some(self.accessibility.clone()),
            effects: Some(self.effects.clone()),
        };
        self.profiles.insert(name.to_string(), profile);
    }

    /// Get the config directory path
    pub fn config_dir() -> PathBuf {
        if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(config_home).join("fracterm")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config").join("fracterm")
        } else {
            PathBuf::from("/tmp/fracterm")
        }
    }

    /// Get the default config file path
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("fracterm.config.ts")
    }

    /// Get the data directory path (layouts, saved state)
    pub fn data_dir() -> PathBuf {
        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(data_home).join("fracterm")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("fracterm")
        } else {
            PathBuf::from("/tmp/fracterm")
        }
    }

    /// Get the default saved-layout path
    pub fn layout_path() -> PathBuf {
        Self::data_dir().join("layout.json")
    }

    /// Load configuration from TypeScript file
    pub fn load_from_ts(path: &PathBuf) -> Result<Self, String> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;

        // Transpile TypeScript to JavaScript using SWC
        let js = transpile_ts_config(&source, path.to_string_lossy().as_ref())?;

        // Evaluate the JavaScript to get the config object
        let config = eval_ts_config(&js)?;
        Ok(config)
    }

    /// Load configuration with fallback to defaults
    pub fn load_or_default(config_path: Option<&PathBuf>, profile: Option<&str>) -> Self {
        let mut config = if let Some(path) = config_path {
            Self::load_from_ts(path).unwrap_or_else(|e| {
                eprintln!("Failed to load config from {}: {}", path.display(), e);
                Self::new()
            })
        } else if let Ok(path_str) = std::env::var("FRACTERM_CONFIG") {
            let path = PathBuf::from(path_str);
            Self::load_from_ts(&path).unwrap_or_else(|e| {
                eprintln!("Failed to load config from {}: {}", path.display(), e);
                Self::new()
            })
        } else {
            let default_path = Self::config_path();
            if default_path.exists() {
                Self::load_from_ts(&default_path).unwrap_or_else(|e| {
                    eprintln!(
                        "Failed to load config from {}: {}",
                        default_path.display(),
                        e
                    );
                    Self::new()
                })
            } else {
                Self::new()
            }
        };

        if let Some(profile_name) = profile {
            config.apply_profile(profile_name);
        }

        config
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Transpile TypeScript config to JavaScript using SWC
fn transpile_ts_config(source: &str, filename: &str) -> Result<String, String> {
    use swc_common::{sync::Lrc, FileName, Globals, Mark, SourceMap, GLOBALS};
    use swc_ecma_codegen::{text_writer::JsWriter, Emitter};
    use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};
    use swc_ecma_transforms_typescript::strip;

    let cm: Lrc<SourceMap> = Lrc::new(SourceMap::new(Default::default()));
    let globals = Globals::new();

    GLOBALS.set(&globals, || {
        let fm = cm.new_source_file(
            FileName::Custom(filename.to_string()).into(),
            source.to_string(),
        );

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                decorators: true,
                ..Default::default()
            }),
            swc_ecma_ast::EsVersion::Es2022,
            StringInput::from(&*fm),
            None,
        );
        let mut parser = Parser::new_from(lexer);
        let module = parser
            .parse_module()
            .map_err(|e| format!("Parse error in {}: {:?}", filename, e))?;

        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();
        let mut ts = strip(unresolved_mark, top_level_mark);
        let mut program = swc_ecma_ast::Program::Module(module);
        swc_ecma_ast::Pass::process(&mut ts, &mut program);
        let swc_ecma_ast::Program::Module(module) = program else {
            return Err("Unexpected program shape after TS strip".into());
        };

        let mut buf = Vec::new();
        {
            let writer = JsWriter::new(cm.clone(), "\n", &mut buf, None);
            let mut emitter = Emitter {
                cfg: Default::default(),
                cm: cm.clone(),
                comments: None,
                wr: Box::new(writer),
            };
            emitter.emit_module(&module).map_err(|e| e.to_string())?;
        }
        String::from_utf8(buf).map_err(|e| e.to_string())
    })
}

/// Evaluate JavaScript config to produce Config struct
fn eval_ts_config(js: &str) -> Result<Config, String> {
    use rquickjs::{Context, Runtime};

    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;

    context.with(|ctx| -> Result<Config, String> {
        // Define the defineConfig function that returns the config object
        let define_config = r#"
            function defineConfig(config) {
                return config;
            }
            globalThis.defineConfig = defineConfig;
        "#;
        ctx.eval::<(), _>(define_config.as_bytes())
            .map_err(|e| e.to_string())?;

        // Evaluate the user's config
        let result: rquickjs::Value = ctx.eval(js.as_bytes()).map_err(|e| e.to_string())?;

        // Serialize to JSON and deserialize to Config using a helper function
        let stringify_fn: rquickjs::Function = ctx
            .eval(
                r#"
                (function(val) {
                    return JSON.stringify(val);
                })
                "#
                .as_bytes(),
            )
            .map_err(|e| e.to_string())?;
        let json_str: String = stringify_fn.call((result,)).map_err(|e| e.to_string())?;

        serde_json::from_str(&json_str).map_err(|e| format!("Failed to parse config: {}", e))
    })
}

/// Built-in profiles
pub fn builtin_profiles() -> HashMap<String, ProfileConfig> {
    let mut profiles = HashMap::new();

    profiles.insert(
        "big-text".to_string(),
        ProfileConfig {
            font: Some(FontConfig {
                size: 24,
                weight: 500,
                ..FontConfig::default()
            }),
            theme: Some(ThemeConfig {
                background: "#000000".to_string(),
                foreground: "#ffffff".to_string(),
                ..ThemeConfig::default()
            }),
            terminal: None,
            accessibility: None,
            effects: None,
        },
    );

    profiles.insert(
        "ssh".to_string(),
        ProfileConfig {
            font: None,
            theme: None,
            terminal: Some(TerminalConfig {
                scrollback_lines: 50000,
                ..TerminalConfig::default()
            }),
            accessibility: None,
            effects: None,
        },
    );

    profiles.insert(
        "logs".to_string(),
        ProfileConfig {
            font: Some(FontConfig {
                size: 12,
                ..FontConfig::default()
            }),
            theme: None,
            terminal: Some(TerminalConfig {
                copy_on_select: true,
                ..TerminalConfig::default()
            }),
            accessibility: None,
            effects: None,
        },
    );

    profiles.insert(
        "presentation".to_string(),
        ProfileConfig {
            font: Some(FontConfig {
                size: 20,
                ..FontConfig::default()
            }),
            theme: None,
            terminal: None,
            accessibility: None,
            effects: Some(EffectsConfig {
                motion_blur: true,
                ..EffectsConfig::default()
            }),
        },
    );

    profiles.insert(
        "high-contrast".to_string(),
        ProfileConfig {
            font: None,
            theme: Some(ThemeConfig::default()),
            terminal: None,
            accessibility: Some(AccessibilityConfig {
                reading_mode: ReadingModeConfig {
                    high_contrast: true,
                    font_size: 28,
                    ..ReadingModeConfig::default()
                },
                ..AccessibilityConfig::default()
            }),
            effects: None,
        },
    );

    profiles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.font.size, 14);
        assert_eq!(config.theme.background, "#0b0d12");
    }

    #[test]
    fn test_profile_application() {
        let mut config = Config {
            profiles: builtin_profiles(),
            ..Default::default()
        };
        config.apply_profile("big-text");
        assert_eq!(config.font.size, 24);
        assert_eq!(config.font.weight, 500);
    }

    #[test]
    fn test_ts_config_transpile() {
        let ts = r###"
            export default defineConfig({
                font: { family: "Test Mono", size: 16, ligatures: false, weight: 500 },
                theme: { background: "#111111", foreground: "#eeeeee", cursor: "#ff0000", selection: "#333333" },
            });
        "###;
        let js = transpile_ts_config(ts, "test.ts").unwrap();
        assert!(js.contains("Test Mono"));
        assert!(js.contains("16"));
    }
}
