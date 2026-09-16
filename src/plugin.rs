//! Plugin host and TypeScript SDK - typed commands, events, widgets, settings.

use super::*;
use crate::event::EventType;
use crate::surface::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Plugin manifest - matches the TypeScript definePlugin schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: String,
    pub permissions: Vec<PermissionRequest>,
    pub settings: HashMap<String, SettingSchema>,
    pub commands: Vec<CommandManifest>,
    pub widgets: Vec<WidgetManifest>,
    pub events: Vec<String>,
}

/// Permission request with optional scope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRequest {
    Unscoped(String),
    Scoped { permission: String, scope: String },
}

/// Setting schema for auto-generated UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingSchema {
    pub setting_type: String,
    pub default: serde_json::Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub description: Option<String>,
    pub enum_values: Option<Vec<String>>,
}

/// Command manifest for declarative registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandManifest {
    pub id: String,
    pub title: String,
    pub category: String,
    pub input: HashMap<String, InputParam>,
}

/// Widget manifest for declarative registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetManifest {
    pub id: String,
    pub title: String,
    pub default_width: u32,
    pub default_height: u32,
}

/// Setting value from user config
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SettingValue {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<SettingValue>),
    Object(HashMap<String, SettingValue>),
}

impl SettingValue {
    /// Convert a JSON value (e.g. a schema default) into a setting value.
    /// Returns `None` for JSON null, which has no setting counterpart.
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::String(s) => Some(SettingValue::String(s.clone())),
            serde_json::Value::Number(n) => n.as_f64().map(SettingValue::Number),
            serde_json::Value::Bool(b) => Some(SettingValue::Bool(*b)),
            serde_json::Value::Array(items) => items
                .iter()
                .map(SettingValue::from_json)
                .collect::<Option<Vec<_>>>()
                .map(SettingValue::Array),
            serde_json::Value::Object(map) => map
                .iter()
                .map(|(k, v)| SettingValue::from_json(v).map(|sv| (k.clone(), sv)))
                .collect::<Option<HashMap<_, _>>>()
                .map(SettingValue::Object),
            serde_json::Value::Null => None,
        }
    }

    /// Short kind name, used in type-mismatch errors.
    pub fn kind(&self) -> &'static str {
        match self {
            SettingValue::String(_) => "string",
            SettingValue::Number(_) => "number",
            SettingValue::Bool(_) => "boolean",
            SettingValue::Array(_) => "array",
            SettingValue::Object(_) => "object",
        }
    }
}

impl SettingSchema {
    /// Validate a value against this schema: declared type, numeric range,
    /// and string enum membership. Unknown type names pass leniently.
    pub fn validate_value(&self, value: &SettingValue) -> Result<(), String> {
        let type_ok = match self.setting_type.as_str() {
            "string" => matches!(value, SettingValue::String(_)),
            "number" => matches!(value, SettingValue::Number(_)),
            "boolean" => matches!(value, SettingValue::Bool(_)),
            "array" => matches!(value, SettingValue::Array(_)),
            "object" => matches!(value, SettingValue::Object(_)),
            _ => true,
        };
        if !type_ok {
            return Err(format!(
                "expected {}, got {}",
                self.setting_type,
                value.kind()
            ));
        }
        if let SettingValue::Number(n) = value {
            if let Some(min) = self.min {
                if *n < min {
                    return Err(format!("{n} is below minimum {min}"));
                }
            }
            if let Some(max) = self.max {
                if *n > max {
                    return Err(format!("{n} is above maximum {max}"));
                }
            }
        }
        if let (SettingValue::String(s), Some(allowed)) = (value, &self.enum_values) {
            if !allowed.contains(s) {
                return Err(format!("{s:?} is not one of {allowed:?}"));
            }
        }
        Ok(())
    }
}

/// Resolve effective plugin settings: schema defaults filled in, user
/// values overlaid after validation. Unknown keys and invalid values
/// (including invalid schema defaults) are errors, so typos and author
/// bugs surface at load instead of misbehaving at runtime.
pub fn resolve_settings(
    schemas: &HashMap<String, SettingSchema>,
    provided: &HashMap<String, SettingValue>,
) -> Result<HashMap<String, SettingValue>, String> {
    for key in provided.keys() {
        if !schemas.contains_key(key) {
            return Err(format!("unknown setting: {key}"));
        }
    }
    let mut resolved = HashMap::with_capacity(schemas.len());
    for (key, schema) in schemas {
        let default = SettingValue::from_json(&schema.default)
            .ok_or_else(|| format!("setting {key} has no usable default"))?;
        schema
            .validate_value(&default)
            .map_err(|e| format!("setting {key} has an invalid default: {e}"))?;
        resolved.insert(key.clone(), default);
    }
    for (key, value) in provided {
        let schema = schemas.get(key).expect("checked above");
        schema
            .validate_value(value)
            .map_err(|e| format!("invalid value for setting {key}: {e}"))?;
        resolved.insert(key.clone(), value.clone());
    }
    Ok(resolved)
}

/// Plugin context provided to activate/deactivate
pub struct PluginContext {
    pub plugin_id: String,
    pub commands: CommandRegistryRef,
    pub events: EventBusRef,
    pub workspace: WorkspaceRef,
    pub settings: HashMap<String, SettingValue>,
    pub storage: PluginStorage,
    pub log: PluginLogger,
}

pub type CommandRegistryRef = Arc<Mutex<CommandRegistry>>;
pub type EventBusRef = Arc<Mutex<EventBus>>;
pub type WorkspaceRef = Arc<Workspace>;

impl PluginContext {
    pub fn register_command(&self, manifest: CommandManifest) -> Result<(), String> {
        let cmd = Command::new(
            CommandId(0),
            &manifest.id,
            &manifest.title,
            &manifest.category,
            move |_ctx, input| {
                // Plugin command handlers are invoked via JS bridge
                Ok(CommandResult::ok(input))
            },
        )
        .with_input(CommandInputSchema {
            params: manifest.input,
        });
        self.commands
            .lock()
            .map_err(|e| e.to_string())?
            .register(cmd);
        Ok(())
    }

    pub fn register_widget(
        &self,
        manifest: WidgetManifest,
        renderer: Box<dyn WidgetRenderer>,
    ) -> Result<(), String> {
        self.workspace.register_widget(manifest, renderer)
    }

    pub fn on_event(
        &self,
        event_type: &str,
        handler: impl Fn(&Event) + Send + Sync + 'static,
    ) -> Result<(), String> {
        let event_type = match event_type {
            "workspace:objectCreated" => EventType::ObjectCreated,
            "terminal:output" => EventType::TerminalOutput,
            "camera:zoomChanged" => EventType::ZoomChanged,
            "plugin:loaded" => EventType::PluginLoaded,
            "plugin:unloaded" => EventType::PluginUnloaded,
            "command:executed" => EventType::CommandExecuted,
            "settings:changed" => EventType::SettingsChanged,
            _ => EventType::Custom(event_type.to_string()),
        };
        self.events
            .lock()
            .map_err(|e| e.to_string())?
            .subscribe(event_type, handler);
        Ok(())
    }

    pub fn emit_event(&self, event: Event) {
        if let Ok(events) = self.events.lock() {
            events.emit(&event);
        }
    }

    pub fn get_setting(&self, key: &str) -> Option<&SettingValue> {
        self.settings.get(key)
    }
}

/// Widget renderer - produces display lists (no direct OpenGL)
pub trait WidgetRenderer: Send + Sync {
    fn render(&self, ctx: &WidgetRenderContext, ui: &mut UiBuilder);
    fn handle_event(&self, event: &WidgetEvent) -> bool;
    fn size(&self) -> (u32, u32);
}

/// Widget render context
pub struct WidgetRenderContext<'a> {
    pub widget_id: String,
    pub bounds: (f64, f64, f64, f64),
    pub theme: &'a Theme,
    pub scale: f64,
    pub camera_x: f64,
    pub camera_y: f64,
    pub camera_zoom: f64,
}

/// Widget event types
#[derive(Debug, Clone)]
pub enum WidgetEvent {
    MouseMove { x: f64, y: f64 },
    MouseDown { x: f64, y: f64, button: MouseButton },
    MouseUp { x: f64, y: f64, button: MouseButton },
    KeyDown { key: String, modifiers: Modifiers },
    KeyUp { key: String, modifiers: Modifiers },
    FocusGained,
    FocusLost,
    Resize { width: u32, height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Immediate-mode UI builder for widgets
pub struct UiBuilder {
    commands: Vec<DrawCommand>,
}

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Clear {
        color: Color,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: Color,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        scale: f64,
        color: Color,
        align: TextAlign,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: Color,
        width: f64,
    },
    Image {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        texture_id: u32,
    },
    Scissor {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    },
    ScissorEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Default for UiBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBuilder {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn clear(&mut self, color: impl Into<Color>) {
        self.commands.push(DrawCommand::Clear {
            color: color.into(),
        });
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: impl Into<Color>) {
        self.commands.push(DrawCommand::Rect {
            x,
            y,
            w,
            h,
            color: color.into(),
        });
    }

    pub fn text(&mut self, text: impl Into<String>, options: TextOptions) {
        self.commands.push(DrawCommand::Text {
            x: options.x,
            y: options.y,
            text: text.into(),
            scale: options.scale,
            color: options.color,
            align: options.align.unwrap_or(TextAlign::Left),
        });
    }

    pub fn line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: impl Into<Color>,
        width: f64,
    ) {
        self.commands.push(DrawCommand::Line {
            x1,
            y1,
            x2,
            y2,
            color: color.into(),
            width,
        });
    }

    pub fn scissor(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.commands.push(DrawCommand::Scissor { x, y, w, h });
    }

    pub fn scissor_end(&mut self) {
        self.commands.push(DrawCommand::ScissorEnd);
    }

    pub fn take_commands(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.commands)
    }
}

#[derive(Debug, Clone)]
pub struct TextOptions {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub color: Color,
    pub align: Option<TextAlign>,
}

impl TextOptions {
    pub fn new(x: f64, y: f64, scale: f64, color: impl Into<Color>) -> Self {
        Self {
            x,
            y,
            scale,
            color: color.into(),
            align: None,
        }
    }

    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = Some(align);
        self
    }
}

/// Plugin storage - isolated per plugin
pub struct PluginStorage {
    plugin_id: String,
    data: HashMap<String, serde_json::Value>,
}

impl PluginStorage {
    pub fn new(plugin_id: &str) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
            data: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn set(&mut self, key: &str, value: serde_json::Value) {
        self.data.insert(key.to_string(), value);
    }

    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.data.remove(key)
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

/// Plugin logger with structured output
pub struct PluginLogger {
    plugin_id: String,
}

impl PluginLogger {
    pub fn new(plugin_id: &str) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
        }
    }

    pub fn info(&self, msg: &str) {
        eprintln!("[plugin {}] {}", self.plugin_id, msg);
    }

    pub fn warn(&self, msg: &str) {
        eprintln!("[plugin {}] WARN: {}", self.plugin_id, msg);
    }

    pub fn error(&self, msg: &str) {
        eprintln!("[plugin {}] ERROR: {}", self.plugin_id, msg);
    }
}

/// Disposable for automatic cleanup
pub struct Disposable {
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl Disposable {
    pub fn new(cleanup: impl FnOnce() + Send + 'static) -> Self {
        Self {
            cleanup: Some(Box::new(cleanup)),
        }
    }

    pub fn dispose(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

impl Drop for Disposable {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

/// Plugin trait - implemented by the host, used by plugins
pub trait PluginHost: Send + Sync {
    fn load_plugin(&mut self, manifest: PluginManifest) -> Result<PluginId, String>;
    fn unload_plugin(&mut self, id: &PluginId) -> Result<(), String>;
    fn call(
        &mut self,
        id: &PluginId,
        method: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    fn dispatch_event(&mut self, event: &Event) -> Result<(), String>;
    fn has_permission(&self, id: &PluginId, permission: &str, scope: Option<&str>) -> bool;
}

/// Plugin ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(pub u64);

/// Host for managing plugins
pub struct PluginManager {
    host: Arc<Mutex<dyn PluginHost>>,
    commands: CommandRegistryRef,
    events: EventBusRef,
    workspace: WorkspaceRef,
}

impl PluginManager {
    pub fn new(
        host: Arc<Mutex<dyn PluginHost>>,
        commands: CommandRegistryRef,
        events: EventBusRef,
        workspace: WorkspaceRef,
    ) -> Self {
        Self {
            host,
            commands,
            events,
            workspace,
        }
    }

    pub fn load(&mut self, manifest: PluginManifest) -> Result<PluginId, String> {
        self.host
            .lock()
            .map_err(|e| e.to_string())?
            .load_plugin(manifest)
    }

    pub fn unload(&mut self, id: &PluginId) -> Result<(), String> {
        self.host
            .lock()
            .map_err(|e| e.to_string())?
            .unload_plugin(id)
    }

    pub fn commands(&self) -> &CommandRegistryRef {
        &self.commands
    }

    pub fn events(&self) -> &EventBusRef {
        &self.events
    }

    pub fn workspace(&self) -> &WorkspaceRef {
        &self.workspace
    }
}

/// Workspace extension for widget registration
impl Workspace {
    pub fn register_widget(
        &self,
        manifest: WidgetManifest,
        _renderer: Box<dyn WidgetRenderer>,
    ) -> Result<(), String> {
        // Widget registration would create a node with WidgetSurface
        // This is a placeholder for the full implementation
        eprintln!("Registering widget: {} ({})", manifest.title, manifest.id);
        Ok(())
    }
}

/// V8 host stub - will be implemented behind PluginHost trait
pub struct V8Host {
    plugins: HashMap<String, Box<dyn std::any::Any + Send + Sync>>,
}

impl V8Host {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }
}

impl PluginHost for V8Host {
    fn load_plugin(&mut self, manifest: PluginManifest) -> Result<PluginId, String> {
        let id = PluginId(1); // placeholder
        self.plugins.insert(manifest.id, Box::new(()));
        Ok(id)
    }

    fn unload_plugin(&mut self, _id: &PluginId) -> Result<(), String> {
        Ok(())
    }

    fn call(
        &mut self,
        _id: &PluginId,
        _method: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err("V8Host not implemented".to_string())
    }

    fn dispatch_event(&mut self, _event: &Event) -> Result<(), String> {
        Ok(())
    }

    fn has_permission(&self, _id: &PluginId, _permission: &str, _scope: Option<&str>) -> bool {
        false
    }
}

impl Default for V8Host {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin SDK for TypeScript development
pub struct PluginSDK {
    pub commands: CommandRegistryRef,
    pub events: EventBusRef,
    pub workspace: WorkspaceRef,
}

impl PluginSDK {
    pub fn new(commands: CommandRegistryRef, events: EventBusRef, workspace: WorkspaceRef) -> Self {
        Self {
            commands,
            events,
            workspace,
        }
    }
}

/// One auto-generated settings UI row (README §9.6): the border options
/// menu renders these directly, so plugins get native-feeling options
/// without hand-written UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingUiRow {
    pub key: String,
    pub kind: String,
    pub default: serde_json::Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub options: Option<Vec<String>>,
    pub description: Option<String>,
}

/// Build the settings UI rows for a plugin's schemas, sorted by key for a
/// stable menu order.
pub fn settings_ui_rows(schemas: &HashMap<String, SettingSchema>) -> Vec<SettingUiRow> {
    let mut keys: Vec<&String> = schemas.keys().collect();
    keys.sort();
    keys.into_iter()
        .map(|key| {
            let schema = &schemas[key];
            SettingUiRow {
                key: key.clone(),
                kind: schema.setting_type.clone(),
                default: schema.default.clone(),
                min: schema.min,
                max: schema.max,
                options: schema.enum_values.clone(),
                description: schema.description.clone(),
            }
        })
        .collect()
}

// Re-export for plugin authors
pub mod prelude {
    pub use super::{
        resolve_settings, settings_ui_rows, CommandManifest, Disposable, DrawCommand, Modifiers,
        MouseButton, PermissionRequest, PluginContext, PluginHost, PluginId, PluginLogger,
        PluginManager, PluginManifest, PluginSDK, PluginStorage, SettingSchema, SettingUiRow,
        SettingValue, TextAlign, TextOptions, UiBuilder, WidgetEvent, WidgetManifest,
        WidgetRenderContext, WidgetRenderer,
    };
    pub use crate::surface::Color;
    pub use crate::{
        Command, CommandContext, CommandId, CommandInputSchema, CommandRegistry, CommandResult,
        CommandValue, InputParam, KeyBinding, Theme, Workspace,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number_schema(default: f64, min: f64, max: f64) -> SettingSchema {
        SettingSchema {
            setting_type: "number".to_string(),
            default: serde_json::json!(default),
            min: Some(min),
            max: Some(max),
            description: None,
            enum_values: None,
        }
    }

    fn clock_schemas() -> HashMap<String, SettingSchema> {
        HashMap::from([
            (
                "refreshMs".to_string(),
                number_schema(1000.0, 100.0, 60000.0),
            ),
            (
                "timezone".to_string(),
                SettingSchema {
                    setting_type: "string".to_string(),
                    default: serde_json::json!("local"),
                    min: None,
                    max: None,
                    description: None,
                    enum_values: None,
                },
            ),
            (
                "showSeconds".to_string(),
                SettingSchema {
                    setting_type: "boolean".to_string(),
                    default: serde_json::json!(true),
                    min: None,
                    max: None,
                    description: None,
                    enum_values: None,
                },
            ),
        ])
    }

    #[test]
    fn test_resolve_settings_fills_defaults() {
        let resolved = resolve_settings(&clock_schemas(), &HashMap::new()).unwrap();
        assert_eq!(
            resolved.get("refreshMs"),
            Some(&SettingValue::Number(1000.0))
        );
        assert_eq!(
            resolved.get("timezone"),
            Some(&SettingValue::String("local".to_string()))
        );
        assert_eq!(resolved.get("showSeconds"), Some(&SettingValue::Bool(true)));
    }

    #[test]
    fn test_resolve_settings_accepts_valid_override() {
        let provided = HashMap::from([("refreshMs".to_string(), SettingValue::Number(500.0))]);
        let resolved = resolve_settings(&clock_schemas(), &provided).unwrap();
        assert_eq!(
            resolved.get("refreshMs"),
            Some(&SettingValue::Number(500.0))
        );
        // Untouched settings still resolve to defaults.
        assert_eq!(
            resolved.get("timezone"),
            Some(&SettingValue::String("local".to_string()))
        );
    }

    #[test]
    fn test_resolve_settings_rejects_bad_values() {
        // Below minimum.
        let provided = HashMap::from([("refreshMs".to_string(), SettingValue::Number(1.0))]);
        assert!(resolve_settings(&clock_schemas(), &provided).is_err());
        // Wrong type.
        let provided = HashMap::from([(
            "refreshMs".to_string(),
            SettingValue::String("fast".to_string()),
        )]);
        assert!(resolve_settings(&clock_schemas(), &provided).is_err());
        // Unknown key (likely a typo).
        let provided = HashMap::from([("refrehMs".to_string(), SettingValue::Number(5.0))]);
        assert!(resolve_settings(&clock_schemas(), &provided).is_err());
    }

    #[test]
    fn test_setting_enum_values() {
        let schema = SettingSchema {
            setting_type: "string".to_string(),
            default: serde_json::json!("info"),
            min: None,
            max: None,
            description: None,
            enum_values: Some(vec!["info".to_string(), "warn".to_string()]),
        };
        assert!(schema
            .validate_value(&SettingValue::String("warn".to_string()))
            .is_ok());
        assert!(schema
            .validate_value(&SettingValue::String("debug".to_string()))
            .is_err());
    }

    #[test]
    fn test_setting_from_json_round_trip() {
        let json = serde_json::json!({"a": 1.0, "b": [true, "x"]});
        let value = SettingValue::from_json(&json).unwrap();
        assert!(matches!(value, SettingValue::Object(_)));
        assert!(SettingValue::from_json(&serde_json::Value::Null).is_none());
    }

    #[test]
    fn test_settings_ui_rows_follow_schemas() {
        let rows = settings_ui_rows(&clock_schemas());
        assert_eq!(rows.len(), 3);
        // Stable key order for the menu.
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, vec!["refreshMs", "showSeconds", "timezone"]);
        let refresh = &rows[0];
        assert_eq!(refresh.kind, "number");
        assert_eq!(refresh.default, serde_json::json!(1000.0));
        assert_eq!(refresh.min, Some(100.0));
        assert_eq!(refresh.max, Some(60000.0));
        assert_eq!(rows[1].kind, "boolean");
        assert_eq!(rows[2].kind, "string");
    }
}
