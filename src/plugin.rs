//! Plugin host - abstract interface for plugin systems.

use super::*;
use std::collections::HashMap;

/// A widget for plugin UI
#[derive(Debug, Clone)]
pub struct Widget {
    pub id: String,
    pub title: String,
}

impl Widget {
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
        }
    }
}

/// Base class for all plugins
pub trait Plugin {
    /// Name of the plugin
    fn name(&self) -> &str;

    /// Version
    fn version(&self) -> &str;

    /// Initialize the plugin
    fn initialize(&self) -> Result<(), Box<dyn std::error::Error>>;

    /// Cleanup the plugin
    fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>>;

    /// Register a command
    fn register_command(
        &self,
        command_id: CommandId,
        command: Command,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Register a widget
    fn register_widget(
        &self,
        widget_id: String,
        widget: Widget,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Load a plugin by ID
    fn load(&self, plugin_id: String) -> Result<Box<dyn Plugin>, Box<dyn std::error::Error>>;

    /// Unload a plugin by ID
    fn unload(&self, plugin_id: String) -> Result<(), Box<dyn std::error::Error>>;
}

/// Host for managing plugins (V8-based)
pub struct V8Host {
    plugins: HashMap<String, Box<dyn Plugin>>,
    commands: HashMap<String, Command>,
}

impl Default for V8Host {
    fn default() -> Self {
        Self::new()
    }
}

impl V8Host {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            commands: HashMap::new(),
        }
    }

    /// Register a plugin
    pub fn register(&mut self, plugin: Box<dyn Plugin>) -> Result<(), Box<dyn std::error::Error>> {
        self.plugins.insert(plugin.name().to_string(), plugin);
        Ok(())
    }

    /// Unregister a plugin
    pub fn unregister(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.plugins.remove(name);
        Ok(())
    }

    /// Get a plugin by name
    pub fn get(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins.get(name).map(|p| &**p)
    }

    /// Execute a command
    pub fn execute_command(&self, command_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.commands.get(command_id).ok_or("Command not found")?;
        Ok(())
    }

    /// List all registered plugins
    pub fn list_plugins(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }
}

/// Plugin SDK types for defining plugins
pub struct PluginSDK {
    /// Available commands
    pub commands: HashMap<String, Command>,

    /// Available widgets
    pub widgets: HashMap<String, Widget>,

    /// Available themes
    pub themes: HashMap<String, Theme>,

    /// Available permissions
    pub permissions: HashMap<String, Permission>,
}

impl PluginSDK {
    /// Register a command
    pub fn register_command(&mut self, id: &str, command: Command) {
        self.commands.insert(id.to_string(), command);
    }

    /// Register a widget
    pub fn register_widget(&mut self, id: &str, widget: Widget) {
        self.widgets.insert(id.to_string(), widget);
    }

    /// Register a theme
    pub fn register_theme(&mut self, name: &str, theme: Theme) {
        self.themes.insert(name.to_string(), theme);
    }

    /// Register a permission
    pub fn register_permission(&mut self, name: &str, permission: Permission) {
        self.permissions.insert(name.to_string(), permission);
    }

    /// Get a command
    pub fn get_command(&self, id: &str) -> Option<&Command> {
        self.commands.get(id)
    }

    /// Get a widget
    pub fn get_widget(&self, id: &str) -> Option<&Widget> {
        self.widgets.get(id)
    }

    /// Get a theme
    pub fn get_theme(&self, name: &str) -> Option<&Theme> {
        self.themes.get(name)
    }

    /// Get a permission
    pub fn get_permission(&self, name: &str) -> Option<&Permission> {
        self.permissions.get(name)
    }
}
