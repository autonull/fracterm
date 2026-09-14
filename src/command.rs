//! Command - typed, reusable action that can be invoked from palette, keybindings, plugins, HUD.

use super::*;
use std::collections::HashMap;

/// Input parameter definition for a command
#[derive(Debug, Clone)]
pub struct InputParam {
    /// Parameter type
    pub param_type: String,
    /// Whether the parameter is optional
    pub optional: bool,
    /// Default value
    pub default: Option<String>,
    /// Description
    pub description: String,
}

/// Command input specification
#[derive(Debug, Clone)]
pub struct CommandInput {
    /// Parameters for the command
    pub params: HashMap<String, InputParam>,
}

impl CommandInput {
    /// Create a new empty command input
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    /// Add a parameter to the command input
    pub fn add_param(&mut self, name: &str, param_type: &str, optional: bool) -> &mut Self {
        self.params.insert(
            name.to_string(),
            InputParam {
                param_type: param_type.to_string(),
                optional,
                default: None,
                description: String::new(),
            },
        );
        self
    }
}

/// A command that can be invoked from palette, keybindings, plugins, HUD.
pub struct Command {
    /// Unique identifier
    pub id: CommandId,
    /// Command identifier string (e.g., "terminal.new")
    pub command_id: String,
    /// Display title
    pub title: String,
    /// Category for grouping
    pub category: String,
    /// Input specification
    pub input: CommandInput,
    /// Whether the command is enabled
    pub enabled: bool,
    /// Source plugin ID if applicable
    pub source: Option<String>,
}

impl Command {
    /// Create a new command
    pub fn new(id: CommandId, command_id: &str, title: &str, category: &str) -> Self {
        Self {
            id,
            command_id: command_id.to_string(),
            title: title.to_string(),
            category: category.to_string(),
            input: CommandInput::new(),
            enabled: true,
            source: None,
        }
    }

    /// Add an input parameter
    pub fn add_input_param(&mut self, name: &str, param_type: &str, optional: bool) {
        self.input.add_param(name, param_type, optional);
    }

    /// Get the command identifier
    pub fn id(&self) -> &str {
        &self.command_id
    }

    /// Get the display title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Check if command is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set enabled state
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}
impl Default for CommandInput {
    fn default() -> Self {
        Self::new()
    }
}
