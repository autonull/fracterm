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
    pub description: &'static str,
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
            description: "Spawn a new terminal beside the focused one (wraps below on narrow viewports, max 8)",
        },
        BuiltinCommand {
            id: "terminal.varied",
            title: "Spawn Varied Terminal",
            category: "Terminal",
            key: "N",
            description: "Spawn a terminal with preset size (cycles 80x24, 100x32, 60x16, 120x28 with matched font scales)",
        },
        BuiltinCommand {
            id: "terminal.close",
            title: "Close Selected",
            category: "Terminal",
            key: "x",
            description: "Close the selected terminal node (press 'u' to reopen, undo stack depth 20)",
        },
        BuiltinCommand {
            id: "terminal.copy",
            title: "Copy Selection",
            category: "Terminal",
            key: "Ctrl+Shift+C",
            description: "Copy the current terminal text selection to clipboard",
        },
        BuiltinCommand {
            id: "layout.tileH",
            title: "Tile Horizontally",
            category: "Arrange",
            key: "h",
            description: "Arrange all nodes side-by-side horizontally with equal spacing",
        },
        BuiltinCommand {
            id: "layout.tileV",
            title: "Tile Vertically",
            category: "Arrange",
            key: "v",
            description: "Arrange all nodes stacked vertically with equal spacing",
        },
        BuiltinCommand {
            id: "layout.tileGrid",
            title: "Tile Grid",
            category: "Arrange",
            key: "t",
            description: "Arrange all nodes in a 3-column grid",
        },
        BuiltinCommand {
            id: "layout.cascade",
            title: "Cascade Diagonal",
            category: "Arrange",
            key: "C",
            description: "Cascade nodes diagonally from the first node (48px steps)",
        },
        BuiltinCommand {
            id: "layout.orbit",
            title: "Orbit Circle",
            category: "Arrange",
            key: "O",
            description: "Orbit nodes in a circle around the centroid",
        },
        BuiltinCommand {
            id: "layout.focus",
            title: "Focus Selected Center",
            category: "Arrange",
            key: "F",
            description: "Focus ring: center the selected node, orbit others around it",
        },
        BuiltinCommand {
            id: "layout.dashboard",
            title: "Dashboard 2-up + Fit",
            category: "Arrange",
            key: "d",
            description: "Tile 2-up from the margin and fit to viewport",
        },
        BuiltinCommand {
            id: "layout.save",
            title: "Save Layout",
            category: "Arrange",
            key: "S",
            description: "Save the current scene graph (nodes, transforms, projections, groups, bookmarks) to XDG layout path",
        },
        BuiltinCommand {
            id: "layout.restore",
            title: "Restore Layout",
            category: "Arrange",
            key: "",
            description: "Restore layout from XDG path: terminals re-spawn fresh PTYs, projections re-link",
        },
        BuiltinCommand {
            id: "view.reduceMotion",
            title: "Toggle Reduce Motion",
            category: "View",
            key: "",
            description: "Toggle reduce-motion mode (animated camera moves snap instantly)",
        },
        BuiltinCommand {
            id: "layout.alignLeft",
            title: "Align Left",
            category: "Arrange",
            key: "a",
            description: "Align all nodes to the left edge",
        },
        BuiltinCommand {
            id: "camera.fit",
            title: "Fit Dashboard",
            category: "Camera",
            key: "f",
            description: "Smooth fly-to: fit all nodes in viewport with margin",
        },
        BuiltinCommand {
            id: "camera.workspaceFit",
            title: "Zoom to Workspace Fit",
            category: "Camera",
            key: "0",
            description: "Instant snap: zoom to show all content (or restore bookmark bm0 if saved)",
        },
        BuiltinCommand {
            id: "camera.bookmarkSave",
            title: "Save Bookmark",
            category: "Camera",
            key: "b",
            description: "Save camera position to next rotating slot bm0–bm9 (digits 1-9 restore)",
        },
        BuiltinCommand {
            id: "camera.bookmarkRestore",
            title: "Restore Bookmark 1-9",
            category: "Camera",
            key: "1-9",
            description: "Restore camera bookmark bm1–bm9 (bm0 via '0' key if saved)",
        },
        BuiltinCommand {
            id: "view.pin",
            title: "Pin Snapshot of Focused Terminal",
            category: "View",
            key: "p",
            description: "Pin a frozen snapshot of the focused terminal as a card beside it",
        },
        BuiltinCommand {
            id: "view.pinLive",
            title: "Pin Live View of Focused Terminal",
            category: "View",
            key: "P",
            description: "Pin a live view of the focused terminal: streams source output every frame (follow tail)",
        },
        BuiltinCommand {
            id: "style.fontBigger",
            title: "Selected: Font Bigger",
            category: "Style",
            key: ".",
            description: "Grow the selected node's font scale (0.7x–2.5x range)",
        },
        BuiltinCommand {
            id: "style.fontSmaller",
            title: "Selected: Font Smaller",
            category: "Style",
            key: ",",
            description: "Shrink the selected node's font scale (0.7x–2.5x range)",
        },
        BuiltinCommand {
            id: "style.opacity",
            title: "Selected: Cycle Opacity",
            category: "Style",
            key: "o",
            description: "Cycle node opacity: 1.0 → 0.92 → 0.8 → 0.67 → 1.0",
        },
        BuiltinCommand {
            id: "style.tint",
            title: "Selected: Cycle Tint",
            category: "Style",
            key: "c",
            description: "Cycle node background tint: ink → moss → indigo → maroon",
        },
        BuiltinCommand {
            id: "hud.toggle",
            title: "Toggle HUD",
            category: "View",
            key: "",
            description: "Toggle the auto-hiding HUD visibility",
        },
        BuiltinCommand {
            id: "terminal.reopen",
            title: "Reopen Closed Terminal",
            category: "Terminal",
            key: "u",
            description: "Reopen the last closed node (fresh PTY for terminals, depth 20)",
        },
        BuiltinCommand {
            id: "layout.selectNext",
            title: "Select Next Node",
            category: "Arrange",
            key: "Tab",
            description: "Cycle node selection in z-order (follows with keyboard focus on terminals)",
        },
        BuiltinCommand {
            id: "help.open",
            title: "Help / Keys",
            category: "Help",
            key: "?",
            description: "Toggle the on-screen key cheatsheet (Esc closes)",
        },
        BuiltinCommand {
            id: "profile.default",
            title: "Profile: Default",
            category: "Profile",
            key: "",
            description: "Apply the default terminal profile",
        },
        BuiltinCommand {
            id: "profile.big-text",
            title: "Profile: Big Text",
            category: "Profile",
            key: "",
            description: "Apply the big-text profile (larger font, high contrast)",
        },
        BuiltinCommand {
            id: "profile.ssh",
            title: "Profile: SSH",
            category: "Profile",
            key: "",
            description: "Apply the SSH profile (large scrollback)",
        },
        BuiltinCommand {
            id: "profile.logs",
            title: "Profile: Logs",
            category: "Profile",
            key: "",
            description: "Apply the logs profile (smaller font, copy on select)",
        },
        BuiltinCommand {
            id: "profile.presentation",
            title: "Profile: Presentation",
            category: "Profile",
            key: "",
            description: "Apply the presentation profile (larger font, motion blur)",
        },
        BuiltinCommand {
            id: "profile.high-contrast",
            title: "Profile: High Contrast",
            category: "Profile",
            key: "",
            description: "Apply the high-contrast profile (accessibility)",
        },
        BuiltinCommand {
            id: "reading.enter",
            title: "Enter Reading Mode",
            category: "Accessibility",
            key: "r",
            description: "Enter reading mode for the focused terminal (reflow, high contrast, large text)",
        },
        BuiltinCommand {
            id: "reading.exit",
            title: "Exit Reading Mode",
            category: "Accessibility",
            key: "Esc",
            description: "Exit reading mode and return to normal view",
        },
        BuiltinCommand {
            id: "reading.readSelection",
            title: "Read Selection",
            category: "Accessibility",
            key: "",
            description: "Open the selected text in reading mode",
        },
        BuiltinCommand {
            id: "reading.readCurrentLine",
            title: "Read Current Line",
            category: "Accessibility",
            key: "",
            description: "Open the current line in reading mode",
        },
        BuiltinCommand {
            id: "reading.readLastLines",
            title: "Read Last 50 Lines",
            category: "Accessibility",
            key: "",
            description: "Open the last 50 lines in reading mode",
        },
        BuiltinCommand {
            id: "reading.readFiltered",
            title: "Read Filtered",
            category: "Accessibility",
            key: "",
            description: "Open filtered content (errors, warnings) in reading mode",
        },
        BuiltinCommand {
            id: "search.find",
            title: "Find in Terminal",
            category: "Search",
            key: "Ctrl+F",
            description: "Open incremental search in the focused terminal",
        },
        BuiltinCommand {
            id: "search.findNext",
            title: "Find Next",
            category: "Search",
            key: "F3",
            description: "Jump to the next search match",
        },
        BuiltinCommand {
            id: "search.findPrev",
            title: "Find Previous",
            category: "Search",
            key: "Shift+F3",
            description: "Jump to the previous search match",
        },
        BuiltinCommand {
            id: "search.toggleCase",
            title: "Toggle Case Sensitivity",
            category: "Search",
            key: "",
            description: "Toggle case-sensitive search",
        },
        BuiltinCommand {
            id: "search.toggleRegex",
            title: "Toggle Regex Mode",
            category: "Search",
            key: "",
            description: "Toggle regex search mode",
        },
        BuiltinCommand {
            id: "search.projectFromResults",
            title: "Project from Search Results",
            category: "Search",
            key: "",
            description: "Create a projection from current search matches",
        },
        BuiltinCommand {
            id: "edit.undo",
            title: "Undo",
            category: "Edit",
            key: "Ctrl+Z",
            description: "Undo the last workspace action (move, resize, create, delete, style change)",
        },
        BuiltinCommand {
            id: "edit.redo",
            title: "Redo",
            category: "Edit",
            key: "Ctrl+Shift+Z",
            description: "Redo the last undone workspace action",
        },
        BuiltinCommand {
            id: "config.keybindings",
            title: "Open Keybinding Editor",
            category: "Config",
            key: "",
            description: "Open the keybinding editor to view and customize keyboard shortcuts",
        },
        BuiltinCommand {
            id: "plugin.hotReload",
            title: "Hot Reload Plugins",
            category: "Plugin",
            key: "",
            description: "Hot reload all plugins (edit → SWC → deactivate → reload → restore state)",
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
        "u" => Some("terminal.reopen"),
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
