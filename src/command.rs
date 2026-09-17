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
                check_param_type(&param.param_type, value)
                    .map_err(|e| format!("Invalid type for {name}: {e}"))?;
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

/// Check a value against the schema's declared type name (`string`,
/// `number`, `boolean`/`bool`, `array`, `object`, `any`). Unknown type
/// names are accepted: the schema cannot judge what it does not know.
fn check_param_type(expected: &str, value: &CommandValue) -> Result<(), String> {
    let ok = match expected {
        "string" => matches!(value, CommandValue::String(_)),
        "number" => matches!(value, CommandValue::Number(_)),
        "boolean" | "bool" => matches!(value, CommandValue::Bool(_)),
        "array" => matches!(value, CommandValue::Array(_)),
        "object" => matches!(value, CommandValue::Object(_)),
        "any" => true,
        _ => return Ok(()),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("expected {expected}, got {}", kind_of(value)))
    }
}

/// Short kind name for a value, used in type-mismatch errors.
fn kind_of(value: &CommandValue) -> &'static str {
    match value {
        CommandValue::String(_) => "string",
        CommandValue::Number(_) => "number",
        CommandValue::Bool(_) => "boolean",
        CommandValue::Array(_) => "array",
        CommandValue::Object(_) => "object",
        CommandValue::Null => "null",
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

/// Handler invoked when a command executes.
pub type CommandHandler = Arc<
    dyn Fn(CommandContext, HashMap<String, CommandValue>) -> Result<CommandResult, String>
        + Send
        + Sync,
>;

/// A command that can be invoked from palette, keybindings, plugins, HUD, tests, CLI.
pub struct Command {
    pub id: CommandId,
    pub command_id: String,
    pub title: String,
    pub category: String,
    pub input_schema: CommandInputSchema,
    pub enabled: bool,
    pub source: Option<String>,
    handler: CommandHandler,
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
        handler: impl Fn(CommandContext, HashMap<String, CommandValue>) -> Result<CommandResult, String>
            + Send
            + Sync
            + 'static,
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

    pub fn execute(
        &self,
        ctx: CommandContext,
        input: HashMap<String, CommandValue>,
    ) -> Result<CommandResult, String> {
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

    pub fn execute(
        &self,
        command_id: &str,
        ctx: CommandContext,
        input: HashMap<String, CommandValue>,
    ) -> Result<CommandResult, String> {
        let command = self
            .commands
            .get(command_id)
            .ok_or_else(|| format!("Command not found: {}", command_id))?;
        if !command.is_enabled() {
            return Err(format!("Command disabled: {}", command_id));
        }
        command.execute(ctx, input)
    }

    pub fn list(&self) -> Vec<&Command> {
        self.commands.values().collect()
    }

    pub fn list_by_category(&self, category: &str) -> Vec<&Command> {
        self.commands
            .values()
            .filter(|c| c.category == category)
            .collect()
    }

    pub fn add_keybinding(&mut self, binding: KeyBinding) {
        self.keybindings.push(binding);
    }

    pub fn remove_keybinding(&mut self, key: &str, when: Option<&str>) -> bool {
        let len = self.keybindings.len();
        self.keybindings
            .retain(|b| b.key != key || b.when.as_deref() != when);
        self.keybindings.len() != len
    }

    pub fn find_keybinding(&self, key: &str, when: Option<&str>) -> Option<&KeyBinding> {
        self.keybindings
            .iter()
            .find(|b| b.key == key && b.when.as_deref() == when)
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

/// Single source of truth for every built-in action id.
///
/// The window palette, context menus, key dispatch, and help text all
/// resolve through these ids (design invariant: every action is a
/// command, not an ad-hoc key handler). Plugins, tests, and a future CLI
/// see the same ids via [`register_builtin_commands`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinCommand {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub key: &'static str,
}

/// All built-in command ids with their palette title, category, and
/// primary workspace-mode key hint ("" when only reachable by digit,
/// palette, or menu).
pub fn builtin_commands() -> Vec<BuiltinCommand> {
    vec![
        BuiltinCommand {
            id: "terminal.new",
            title: "New Terminal",
            category: "Terminal",
            key: "n",
        },
        BuiltinCommand {
            id: "terminal.varied",
            title: "Spawn Varied Terminal",
            category: "Terminal",
            key: "N",
        },
        BuiltinCommand {
            id: "terminal.close",
            title: "Close Selected",
            category: "Terminal",
            key: "x",
        },
        BuiltinCommand {
            id: "terminal.copy",
            title: "Copy Selection",
            category: "Terminal",
            key: "Ctrl+Shift+C",
        },
        BuiltinCommand {
            id: "layout.tileH",
            title: "Tile Horizontally",
            category: "Arrange",
            key: "h",
        },
        BuiltinCommand {
            id: "layout.tileV",
            title: "Tile Vertically",
            category: "Arrange",
            key: "v",
        },
        BuiltinCommand {
            id: "layout.tileGrid",
            title: "Tile Grid",
            category: "Arrange",
            key: "t",
        },
        BuiltinCommand {
            id: "layout.cascade",
            title: "Cascade Diagonal",
            category: "Arrange",
            key: "C",
        },
        BuiltinCommand {
            id: "layout.orbit",
            title: "Orbit Circle",
            category: "Arrange",
            key: "O",
        },
        BuiltinCommand {
            id: "layout.focus",
            title: "Focus Selected Center",
            category: "Arrange",
            key: "F",
        },
        BuiltinCommand {
            id: "layout.dashboard",
            title: "Dashboard 2-up + Fit",
            category: "Arrange",
            key: "d",
        },
        BuiltinCommand {
            id: "layout.save",
            title: "Save Layout",
            category: "Arrange",
            key: "S",
        },
        BuiltinCommand {
            id: "layout.restore",
            title: "Restore Layout",
            category: "Arrange",
            key: "",
        },
        BuiltinCommand {
            id: "view.reduceMotion",
            title: "Toggle Reduce Motion",
            category: "View",
            key: "",
        },
        BuiltinCommand {
            id: "layout.alignLeft",
            title: "Align Left",
            category: "Arrange",
            key: "a",
        },
        BuiltinCommand {
            id: "camera.fit",
            title: "Fit Dashboard",
            category: "Camera",
            key: "f",
        },
        BuiltinCommand {
            id: "camera.workspaceFit",
            title: "Zoom to Workspace Fit",
            category: "Camera",
            key: "0",
        },
        BuiltinCommand {
            id: "camera.bookmarkSave",
            title: "Save Bookmark",
            category: "Camera",
            key: "b",
        },
        BuiltinCommand {
            id: "camera.bookmarkRestore",
            title: "Restore Bookmark 1-9",
            category: "Camera",
            key: "1-9",
        },
        BuiltinCommand {
            id: "view.pin",
            title: "Pin Snapshot of Focused Terminal",
            category: "View",
            key: "p",
        },
        BuiltinCommand {
            id: "view.pinLive",
            title: "Pin Live View of Focused Terminal",
            category: "View",
            key: "P",
        },
        BuiltinCommand {
            id: "style.fontBigger",
            title: "Selected: Font Bigger",
            category: "Style",
            key: ".",
        },
        BuiltinCommand {
            id: "style.fontSmaller",
            title: "Selected: Font Smaller",
            category: "Style",
            key: ",",
        },
        BuiltinCommand {
            id: "style.opacity",
            title: "Selected: Cycle Opacity",
            category: "Style",
            key: "o",
        },
        BuiltinCommand {
            id: "style.tint",
            title: "Selected: Cycle Tint",
            category: "Style",
            key: "c",
        },
        BuiltinCommand {
            id: "help.open",
            title: "Help / Keys",
            category: "Help",
            key: "?",
        },
    ]
}

/// Key (workspace mode) to built-in command id, for single-character keys.
/// Multi-key bindings (palette, copy) and digit restores are handled at
/// the call site; this covers the `c == "..."` dispatch arms.
pub fn command_for_key(key: &str) -> Option<&'static str> {
    match key {
        "n" => Some("terminal.new"),
        "N" => Some("terminal.varied"),
        "x" => Some("terminal.close"),
        "h" => Some("layout.tileH"),
        "v" => Some("layout.tileV"),
        "t" => Some("layout.tileGrid"),
        "C" => Some("layout.cascade"),
        "O" => Some("layout.orbit"),
        "F" => Some("layout.focus"),
        "d" => Some("layout.dashboard"),
        "S" => Some("layout.save"),
        "a" => Some("layout.alignLeft"),
        "f" => Some("camera.fit"),
        "0" => Some("camera.workspaceFit"),
        "b" => Some("camera.bookmarkSave"),
        "p" => Some("view.pin"),
        "P" => Some("view.pinLive"),
        "." => Some("style.fontBigger"),
        "," => Some("style.fontSmaller"),
        "o" => Some("style.opacity"),
        "c" => Some("style.tint"),
        "?" => Some("help.open"),
        _ => None,
    }
}

/// Register every built-in id into a registry so palette rows, plugins,
/// tests, and a future CLI resolve the same ids. Handlers validate empty
/// input and report success; real execution lives in the window layer,
/// which dispatches by these ids.
pub fn register_builtin_commands(registry: &mut CommandRegistry) {
    for spec in builtin_commands() {
        let title = spec.title;
        let category = spec.category;
        registry.register(Command::new(
            CommandId(0),
            spec.id,
            title,
            category,
            move |_ctx, _input| Ok(CommandResult::ok(format!("{title} dispatched"))),
        ));
    }
}

/// Plugin host trait for command execution context
pub trait PluginHost: Send + Sync {
    fn execute_command(
        &self,
        plugin_id: &str,
        command: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

impl PluginHost for () {
    fn execute_command(
        &self,
        _plugin_id: &str,
        _command: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err("No plugin host available".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_registration_and_execution() {
        let mut registry = CommandRegistry::new();
        let cmd = Command::new(CommandId(1), "test.echo", "Echo", "Test", |_ctx, input| {
            let msg = input
                .get("message")
                .and_then(|v| v.as_string())
                .unwrap_or("");
            Ok(CommandResult::ok(msg.to_string()))
        })
        .with_input(
            CommandInputSchema::new()
                .add_param("message", "string", false)
                .with_description("message", "Message to echo"),
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
            |_ctx, input| Ok(CommandResult::ok(input.get("required").unwrap().clone())),
        )
        .with_input(
            CommandInputSchema::new()
                .add_param("required", "string", false)
                .add_param("optional", "string", true),
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
    fn test_command_type_validation() {
        let schema = CommandInputSchema::new()
            .add_param("count", "number", false)
            .add_param("label", "string", true)
            .add_param("flag", "boolean", true);

        let mut good = HashMap::new();
        good.insert("count".to_string(), CommandValue::Number(3.0));
        good.insert("label".to_string(), "x".into());
        good.insert("flag".to_string(), CommandValue::Bool(true));
        assert!(schema.validate(&good).is_ok());

        let mut wrong = HashMap::new();
        wrong.insert("count".to_string(), "three".into());
        let err = schema.validate(&wrong).unwrap_err();
        assert!(err.contains("count") && err.contains("number"));

        // Optional params may be omitted entirely.
        let mut minimal = HashMap::new();
        minimal.insert("count".to_string(), CommandValue::Number(1.0));
        assert!(schema.validate(&minimal).is_ok());
    }

    #[test]
    fn test_command_unknown_type_is_lenient() {
        let schema = CommandInputSchema::new().add_param("custom", "some-future-type", false);
        let mut input = HashMap::new();
        input.insert("custom".to_string(), "anything".into());
        assert!(schema.validate(&input).is_ok());
    }

    #[test]
    fn test_builtin_catalog_ids_unique_and_registered() {
        use std::collections::HashSet;
        let specs = builtin_commands();
        let ids: HashSet<_> = specs.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), specs.len(), "duplicate builtin command id");
        let mut registry = CommandRegistry::new();
        register_builtin_commands(&mut registry);
        for spec in &specs {
            assert!(registry.get(spec.id).is_some(), "missing {}", spec.id);
        }
        assert_eq!(registry.list().len(), specs.len());
    }

    #[test]
    fn test_command_for_key_covers_single_char_dispatch() {
        for key in [
            "n", "N", "x", "h", "v", "t", "C", "O", "F", "d", "a", "f", "0", "b", "p", "P", ".",
            ",", "o", "c", "?", "S",
        ] {
            let id = command_for_key(key).unwrap_or_else(|| panic!("no command for key {key}"));
            assert!(builtin_commands().iter().any(|s| s.id == id));
        }
        assert_eq!(command_for_key("q"), None);
    }

    #[test]
    fn test_builtin_command_executes() {
        let mut registry = CommandRegistry::new();
        register_builtin_commands(&mut registry);
        let ctx = CommandContext::new(
            Arc::new(Workspace::new()),
            Arc::new(EventBus::new()),
            Arc::new(()),
            Config::default(),
        );
        let result = registry
            .execute("layout.tileH", ctx, HashMap::new())
            .unwrap();
        assert!(result.success);
        assert!(registry
            .execute(
                "nope.missing",
                CommandContext::new(
                    Arc::new(Workspace::new()),
                    Arc::new(EventBus::new()),
                    Arc::new(()),
                    Config::default(),
                ),
                HashMap::new()
            )
            .is_err());
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

        assert_eq!(
            registry
                .find_keybinding("Ctrl+N", Some("workspace"))
                .map(|b| b.command.as_str()),
            Some("terminal.new")
        );
        assert_eq!(
            registry
                .find_keybinding("Ctrl+N", Some("terminal"))
                .map(|b| b.command.as_str()),
            Some("terminal.split")
        );
        assert_eq!(registry.find_keybinding("Ctrl+N", None), None);
    }
}
