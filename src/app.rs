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
}

impl CliArgs {
    pub fn new() -> Self {
        Self {
            config_path: None,
            profile: None,
            verbose: false,
            plugin: vec![],
            layout: None,
        }
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
            config.accessibility.high_contrast = true;
            config.accessibility.font_size = 28;
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
