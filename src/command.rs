//! Command system - typed, reusable actions invokable from palette, keybindings, plugins, HUD, tests, CLI.

use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// JSON-serializable command input value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandValue {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<CommandValue>),
    Object(HashMap<String, CommandValue>),
    Null,
}

impl CommandValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            CommandValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            CommandValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            CommandValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, CommandValue>> {
        match self {
            CommandValue::Object(o) => Some(o),
            _ => None,
        }
    }
}

impl From<String> for CommandValue {
    fn from(s: String) -> Self {
        CommandValue::String(s)
    }
}

impl From<&str> for CommandValue {
    fn from(s: &str) -> Self {
        CommandValue::String(s.to_string())
    }
}

impl From<f64> for CommandValue {
    fn from(n: f64) -> Self {
        CommandValue::Number(n)
    }
}

impl From<bool> for CommandValue {
    fn from(b: bool) -> Self {
        CommandValue::Bool(b)
    }
}

impl From<Vec<CommandValue>> for CommandValue {
    fn from(v: Vec<CommandValue>) -> Self {
        CommandValue::Array(v)
    }
}

impl From<HashMap<String, CommandValue>> for CommandValue {
    fn from(o: HashMap<String, CommandValue>) -> Self {
        CommandValue::Object(o)
    }
}

/// Input parameter schema for a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputParam {
    pub param_type: String,
    pub optional: bool,
    pub default: Option<CommandValue>,
    pub description: String,
    pub enum_values: Option<Vec<String>>,
}

/// Command input specification with validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInputSchema {
    pub params: HashMap<String, InputParam>,
}

impl CommandInputSchema {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn add_param(mut self, name: &str, param_type: &str, optional: bool) -> Self {
        self.params.insert(
            name.to_string(),
            InputParam {
                param_type: param_type.to_string(),
                optional,
                default: None,
                description: String::new(),
                enum_values: None,
            },
        );
        self
    }

    pub fn with_description(mut self, name: &str, description: &str) -> Self {
        if let Some(param) = self.params.get_mut(name) {
            param.description = description.to_string();
        }
        self
    }

    pub fn with_default(mut self, name: &str, default: CommandValue) -> Self {
        if let Some(param) = self.params.get_mut(name) {
            param.default = Some(default);
        }
        self
    }

    pub fn with_enum(mut self, name: &str, values: Vec<String>) -> Self {
        if let Some(param) = self.params.get_mut(name) {
            param.enum_values = Some(values);
        }
        self
    }

    pub fn validate(&self, input: &HashMap<String, CommandValue>) -> Result<(), String> {
        for (name, param) in &self.params {
            if !param.optional && !input.contains_key(name) {
                return Err(format!("Missing required parameter: {}", name));
            }
            if let Some(value) = input.get(name) {
                if let Some(enum_values) = &param.enum_values {
                    if let CommandValue::String(s) = value {
                        if !enum_values.contains(s) {
                            return Err(format!(
                                "Invalid value for {}: expected one of {:?}, got {}",
                                name, enum_values, s
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn apply_defaults(&self, input: &mut HashMap<String, CommandValue>) {
        for (name, param) in &self.params {
            if !input.contains_key(name) {
                if let Some(default) = &param.default {
                    input.insert(name.clone(), default.clone());
                }
            }
        }
    }
}

impl Default for CommandInputSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// Command execution context
#[derive(Clone)]
pub struct CommandContext {
    pub workspace: Arc<Workspace>,
    pub event_bus: Arc<EventBus>,
    pub plugin_host: Arc<dyn PluginHost>,
    pub config: Config,
}

impl CommandContext {
    pub fn new(
        workspace: Arc<Workspace>,
        event_bus: Arc<EventBus>,
        plugin_host: Arc<dyn PluginHost>,
        config: Config,
    ) -> Self {
        Self {
            workspace,
            event_bus,
            plugin_host,
            config,
        }
    }
}

/// Command execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub output: Option<CommandValue>,
    pub error: Option<String>,
}

impl CommandResult {
    pub fn ok(output: impl Into<CommandValue>) -> Self {
        Self {
            success: true,
            output: Some(output.into()),
            error: None,
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: None,
            error: Some(error.into()),
        }
    }
}

/// A command that can be invoked from palette, keybindings, plugins, HUD, tests, CLI.
pub struct Command {
    pub id: CommandId,
    pub command_id: String,
    pub title: String,
    pub category: String,
    pub input_schema: CommandInputSchema,
    pub enabled: bool,
    pub source: Option<String>,
    handler: Arc<dyn Fn(CommandContext, HashMap<String, CommandValue>) -> Result<CommandResult, String> + Send + Sync>,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("id", &self.id)
            .field("command_id", &self.command_id)
            .field("title", &self.title)
            .field("category", &self.category)
            .field("input_schema", &self.input_schema)
            .field("enabled", &self.enabled)
            .field("source", &self.source)
            .finish()
    }
}

impl Command {
    pub fn new(
        id: CommandId,
        command_id: &str,
        title: &str,
        category: &str,
        handler: impl Fn(CommandContext, HashMap<String, CommandValue>) -> Result<CommandResult, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            id,
            command_id: command_id.to_string(),
            title: title.to_string(),
            category: category.to_string(),
            input_schema: CommandInputSchema::new(),
            enabled: true,
            source: None,
            handler: Arc::new(handler),
        }
    }

    pub fn with_input(mut self, schema: CommandInputSchema) -> Self {
        self.input_schema = schema;
        self
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    pub fn execute(&self, ctx: CommandContext, input: HashMap<String, CommandValue>) -> Result<CommandResult, String> {
        let mut input = input;
        self.input_schema.apply_defaults(&mut input);
        self.input_schema.validate(&input)?;
        (self.handler)(ctx, input)
    }

    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn category(&self) -> &str {
        &self.category
    }

    pub fn input_schema(&self) -> &CommandInputSchema {
        &self.input_schema
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

/// Keybinding entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub command: String,
    pub when: Option<String>,
    pub args: HashMap<String, CommandValue>,
}

/// Command registry - central store for all commands
pub struct CommandRegistry {
    commands: HashMap<String, Command>,
    keybindings: Vec<KeyBinding>,
    next_id: u64,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            keybindings: Vec::new(),
            next_id: 1,
        }
    }

    pub fn register(&mut self, command: Command) -> CommandId {
        let id = CommandId(self.next_id);
        self.next_id += 1;
        let cmd_id = command.command_id.clone();
        self.commands.insert(cmd_id, command);
        id
    }

    pub fn unregister(&mut self, command_id: &str) -> Option<Command> {
        self.commands.remove(command_id)
    }

    pub fn get(&self, command_id: &str) -> Option<&Command> {
        self.commands.get(command_id)
    }

    pub fn get_mut(&mut self, command_id: &str) -> Option<&mut Command> {
        self.commands.get_mut(command_id)
    }

    pub fn execute(&self, command_id: &str, ctx: CommandContext, input: HashMap<String, CommandValue>) -> Result<CommandResult, String> {
        let command = self.commands.get(command_id).ok_or_else(|| format!("Command not found: {}", command_id))?;
        if !command.is_enabled() {
            return Err(format!("Command disabled: {}", command_id));
        }
        command.execute(ctx, input)
    }

    pub fn list(&self) -> Vec<&Command> {
        self.commands.values().collect()
    }

    pub fn list_by_category(&self, category: &str) -> Vec<&Command> {
        self.commands.values().filter(|c| c.category == category).collect()
    }

    pub fn add_keybinding(&mut self, binding: KeyBinding) {
        self.keybindings.push(binding);
    }

    pub fn remove_keybinding(&mut self, key: &str, when: Option<&str>) -> bool {
        let len = self.keybindings.len();
        self.keybindings.retain(|b| b.key != key || b.when.as_deref() != when);
        self.keybindings.len() != len
    }

    pub fn find_keybinding(&self, key: &str, when: Option<&str>) -> Option<&KeyBinding> {
        self.keybindings.iter().find(|b| b.key == key && b.when.as_deref() == when)
    }

    pub fn keybindings(&self) -> &[KeyBinding] {
        &self.keybindings
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin host trait for command execution context
pub trait PluginHost: Send + Sync {
    fn execute_command(&self, plugin_id: &str, command: &str, args: serde_json::Value) -> Result<serde_json::Value, String>;
}

impl PluginHost for () {
    fn execute_command(&self, _plugin_id: &str, _command: &str, _args: serde_json::Value) -> Result<serde_json::Value, String> {
        Err("No plugin host available".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_registration_and_execution() {
        let mut registry = CommandRegistry::new();
        let cmd = Command::new(
            CommandId(1),
            "test.echo",
            "Echo",
            "Test",
            |_ctx, input| {
                let msg = input.get("message").and_then(|v| v.as_string()).unwrap_or("");
                Ok(CommandResult::ok(msg.to_string()))
            },
        ).with_input(
            CommandInputSchema::new()
                .add_param("message", "string", false)
                .with_description("message", "Message to echo")
        );
        registry.register(cmd);

        let ctx = CommandContext::new(
            Arc::new(Workspace::new()),
            Arc::new(EventBus::new()),
            Arc::new(()),
            Config::default(),
        );
        let mut input = HashMap::new();
        input.insert("message".to_string(), "hello".into());
        let result = registry.execute("test.echo", ctx, input).unwrap();
        assert!(result.success);
        assert_eq!(result.output.unwrap().as_string(), Some("hello"));
    }

    #[test]
    fn test_command_validation() {
        let mut registry = CommandRegistry::new();
        let cmd = Command::new(
            CommandId(1),
            "test.required",
            "Required Param",
            "Test",
            |_ctx, input| {
                Ok(CommandResult::ok(input.get("required").unwrap().clone()))
            },
        ).with_input(
            CommandInputSchema::new()
                .add_param("required", "string", false)
                .add_param("optional", "string", true)
        );
        registry.register(cmd);

        let ctx = CommandContext::new(
            Arc::new(Workspace::new()),
            Arc::new(EventBus::new()),
            Arc::new(()),
            Config::default(),
        );

        // Missing required param should fail
        let result = registry.execute("test.required", ctx.clone(), HashMap::new());
        assert!(result.is_err());

        // With required param should succeed
        let mut input = HashMap::new();
        input.insert("required".to_string(), "value".into());
        let result = registry.execute("test.required", ctx, input).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_keybindings() {
        let mut registry = CommandRegistry::new();
        registry.add_keybinding(KeyBinding {
            key: "Ctrl+N".to_string(),
            command: "terminal.new".to_string(),
            when: Some("workspace".to_string()),
            args: HashMap::new(),
        });
        registry.add_keybinding(KeyBinding {
            key: "Ctrl+N".to_string(),
            command: "terminal.split".to_string(),
            when: Some("terminal".to_string()),
            args: HashMap::new(),
        });

        assert_eq!(registry.find_keybinding("Ctrl+N", Some("workspace")).map(|b| b.command.as_str()), Some("terminal.new"));
        assert_eq!(registry.find_keybinding("Ctrl+N", Some("terminal")).map(|b| b.command.as_str()), Some("terminal.split"));
        assert_eq!(registry.find_keybinding("Ctrl+N", None), None);
    }
}