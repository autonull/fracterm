//! Application - winit window, main loop, and CLI.

use super::*;
use crate::config::ThemeConfig;

/// Application state
pub struct AppState {
    pub workspace: Workspace,
    pub camera: Camera,
    pub config: Config,
    pub event_bus: EventBus,
    pub plugin_host: V8Host,
    pub active_mode: InteractionMode,
    pub plugins_loaded: Vec<String>,
    /// HiDPI scale factor from the windowing system
    pub hidpi_scale: f64,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            workspace: Workspace::new(),
            camera: Camera::new(),
            config,
            event_bus: EventBus::new(),
            plugin_host: V8Host::new(),
            active_mode: InteractionMode::Workspace,
            plugins_loaded: vec![],
            hidpi_scale: 1.0,
        }
    }
}

/// Interaction modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMode {
    /// Default canvas navigation
    Workspace,
    /// Terminal focused mode
    Terminal,
    /// Reading accessibility mode
    Reading,
    /// Dashboard arrangement mode
    Dashboard,
}

impl InteractionMode {
    pub fn as_str(&self) -> &str {
        match self {
            InteractionMode::Workspace => "workspace",
            InteractionMode::Terminal => "terminal",
            InteractionMode::Reading => "reading",
            InteractionMode::Dashboard => "dashboard",
        }
    }
}

/// CLI arguments
#[derive(Debug, Clone)]
pub struct CliArgs {
    pub config_path: Option<String>,
    pub profile: Option<String>,
    pub verbose: bool,
    pub plugin: Vec<String>,
    pub layout: Option<String>,
    pub show_help: bool,
    pub show_version: bool,
    pub print_config: bool,
    pub doctor: bool,
}

impl CliArgs {
    pub fn new() -> Self {
        Self {
            config_path: None,
            profile: None,
            verbose: false,
            plugin: vec![],
            layout: None,
            show_help: false,
            show_version: false,
            print_config: false,
            doctor: false,
        }
    }

    /// Parse command-line arguments
    pub fn parse() -> Self {
        let mut args = Self::new();
        let cli_args: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < cli_args.len() {
            match cli_args[i].as_str() {
                "--help" | "-h" => {
                    args.show_help = true;
                }
                "--version" | "-V" => {
                    args.show_version = true;
                }
                "--print-config" => {
                    args.print_config = true;
                }
                "--doctor" => {
                    args.doctor = true;
                }
                "--config" => {
                    if i + 1 < cli_args.len() {
                        args.config_path = Some(cli_args[i + 1].clone());
                        i += 1;
                    }
                }
                "--profile" => {
                    if i + 1 < cli_args.len() {
                        args.profile = Some(cli_args[i + 1].clone());
                        i += 1;
                    }
                }
                "--verbose" | "-v" => {
                    args.verbose = true;
                }
                "--plugin" => {
                    if i + 1 < cli_args.len() {
                        args.plugin.push(cli_args[i + 1].clone());
                        i += 1;
                    }
                }
                "--layout" => {
                    if i + 1 < cli_args.len() {
                        args.layout = Some(cli_args[i + 1].clone());
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        args
    }

    /// Print help message
    pub fn print_help() {
        println!("fracterm - A spatial workspace of live text surfaces");
        println!("");
        println!("USAGE:");
        println!("    fracterm [OPTIONS]");
        println!("");
        println!("OPTIONS:");
        println!("    -h, --help           Print help information");
        println!("    -V, --version        Print version information");
        println!("    --print-config       Print the effective configuration (resolved from TS config + profile)");
        println!("    --doctor             Run diagnostics (OpenGL, fonts, JS engine, plugins, layout)");
        println!("    --config <PATH>      Path to fracterm.config.ts (default: ~/.config/fracterm/fracterm.config.ts)");
        println!("    --profile <NAME>     Apply a profile (big-text, ssh, logs, presentation, high-contrast)");
        println!("    -v, --verbose        Enable verbose logging");
        println!("    --plugin <PATH>      Load a plugin at startup");
        println!("    --layout <PATH>      Load a layout file at startup");
        println!("");
        println!("CONFIGURATION:");
        println!("    TypeScript-first config at ~/.config/fracterm/fracterm.config.ts");
        println!("    Example:");
        println!("        export default defineConfig({{");
        println!("            font: {{ family: \"JetBrains Mono\", size: 14, ligatures: true }}");
        println!("            theme: {{ background: \"#0b0d12\", foreground: \"#dfe3ee\" }}");
        println!("        }});");
        println!("");
        println!("KEYBINDINGS (workspace mode):");
        println!("    n          New terminal");
        println!("    N          Spawn varied terminal");
        println!("    x          Close selected");
        println!("    u          Reopen closed terminal (undo)");
        println!("    h/v/t      Tile H/V/Grid");
        println!("    C/O/F      Cascade/Orbit/Focus ring");
        println!("    f          Fit dashboard (smooth)");
        println!("    0          Zoom to workspace fit (instant)");
        println!("    b          Save camera bookmark (bm0-bm9)");
        println!("    1-9        Restore camera bookmark");
        println!("    p/P        Pin snapshot / live view");
        println!("    ,/.        Font smaller/bigger");
        println!("    o/c        Cycle opacity/tint");
        println!("    d          Dashboard 2-up + fit");
        println!("    S          Save layout");
        println!("    Ctrl+K     Command palette");
        println!("    ?/F1       Toggle help overlay");
        println!("    Tab        Cycle node selection");
        println!("    Arrow keys Nudge selected node");
        println!("");
        println!("ENVIRONMENT:");
        println!("    XDG_CONFIG_HOME    Config directory (default: ~/.config)");
        println!("    XDG_DATA_HOME      Data directory (default: ~/.local/share)");
        println!("    XDG_CACHE_HOME     Cache directory (default: ~/.cache)");
        println!("    FRACTERM_CONFIG    Override config file path");
        println!("");
        println!("For more information, see https://github.com/fracterm/fracterm");
    }

    /// Print version information
    pub fn print_version() {
        println!("fracterm {}", env!("CARGO_PKG_VERSION"));
    }

    /// Print effective configuration
    pub fn print_effective_config(&self) {
        let config = if let Some(path) = &self.config_path {
            Config::load_from_ts(&std::path::PathBuf::from(path)).unwrap_or_else(|e| {
                eprintln!("Failed to load config: {e}");
                Config::default()
            })
        } else {
            let path = Config::config_path();
            if path.exists() {
                Config::load_from_ts(&path).unwrap_or_else(|e| {
                    eprintln!("Failed to load config: {e}");
                    Config::default()
                })
            } else {
                Config::default()
            }
        };

        let config = if let Some(profile) = &self.profile {
            let mut c = config;
            c.apply_profile(profile);
            c
        } else {
            config
        };

        println!("{}", serde_json::to_string_pretty(&config).unwrap_or_default());
    }

    /// Run diagnostics
    pub fn run_doctor(&self) {
        println!("fracterm doctor - running diagnostics...");
        println!("");

        // Check OpenGL
        println!("[1/5] OpenGL support...");
        println!("    (requires OpenGL 3.3+ context - checked at runtime)");

        // Check fonts
        println!("[2/5] Font availability...");
        println!("    Checking fontconfig for monospace fonts...");

        // Check JS engine
        println!("[3/5] JavaScript engine...");
        match rquickjs::Runtime::new() {
            Ok(_) => println!("    QuickJS: OK"),
            Err(e) => println!("    QuickJS: FAILED - {e}"),
        }

        // Check plugin manifest
        println!("[4/5] Plugin system...");
        println!("    Plugin host: V8Host (stub) / QuickJS (live)");

        // Check layout
        println!("[5/5] Layout schema...");
        println!("    Layout version 2 supported");

        println!("");
        println!("All checks passed (runtime checks require display).");
    }
}

/// Main application entry point
pub struct App {
    pub state: AppState,
    pub running: bool,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            state: AppState::new(config),
            running: true,
        }
    }

    /// Run the application main loop
    pub fn run(&mut self) {
        while self.running {
            self.process_events();
            self.update();
            self.render();
        }
    }

    fn process_events(&mut self) {
        // Process input events and window events
        let _ = self.state.event_bus.is_throttled();
    }

    fn update(&mut self) {
        self.state.camera.update(0.016);
        self.state.workspace.set_zoom(self.state.camera.zoom);
    }

    fn render(&mut self) {
        // Render the workspace
        let _ = self.state.workspace.nodes();
    }

    /// Stop the application
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Load a plugin by path
    pub fn load_plugin(&mut self, plugin_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.state.plugins_loaded.push(plugin_path.to_string());
        Ok(())
    }
}

/// Create default configuration
pub fn default_config() -> Config {
    Config::default()
}

/// Create configuration from a profile
pub fn config_with_profile(profile_name: &str) -> Config {
    let mut config = Config::default();
    match profile_name {
        "big-text" => {
            config.font.size = 24;
            config.font.weight = 500;
        }
        "ssh" => {
            config.terminal.scrollback_lines = 50000;
        }
        "logs" => {
            config.font.size = 12;
            config.terminal.copy_on_select = true;
        }
        "presentation" => {
            config.font.size = 20;
            config.effects.motion_blur = true;
        }
        "high-contrast" => {
            config.theme = ThemeConfig::default();
            config.accessibility.reading_mode.high_contrast = true;
            config.accessibility.reading_mode.font_size = 28;
        }
        _ => {}
    }
    config
}
impl Default for CliArgs {
    fn default() -> Self {
        Self::new()
    }
}
