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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<SettingValue>),
    Object(HashMap<String, SettingValue>),
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
        ).with_input(CommandInputSchema {
            params: manifest.input,
        });
        self.commands.lock().map_err(|e| e.to_string())?.register(cmd);
        Ok(())
    }

    pub fn register_widget(&self, manifest: WidgetManifest, renderer: Box<dyn WidgetRenderer>) -> Result<(), String> {
        self.workspace.register_widget(manifest, renderer)
    }

    pub fn on_event(&self, event_type: &str, handler: impl Fn(&Event) + Send + Sync + 'static) -> Result<(), String> {
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
        self.events.lock().map_err(|e| e.to_string())?.subscribe(event_type, handler);
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
    cursor: (f64, f64),
}

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Clear { color: Color },
    Rect { x: f64, y: f64, w: f64, h: f64, color: Color },
    Text { x: f64, y: f64, text: String, scale: f64, color: Color, align: TextAlign },
    Line { x1: f64, y1: f64, x2: f64, y2: f64, color: Color, width: f64 },
    Image { x: f64, y: f64, w: f64, h: f64, texture_id: u32 },
    Scissor { x: f64, y: f64, w: f64, h: f64 },
    ScissorEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl UiBuilder {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            cursor: (0.0, 0.0),
        }
    }

    pub fn clear(&mut self, color: impl Into<Color>) {
        self.commands.push(DrawCommand::Clear { color: color.into() });
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: impl Into<Color>) {
        self.commands.push(DrawCommand::Rect { x, y, w, h, color: color.into() });
    }

    pub fn text(&mut self, text: impl Into<String>, options: TextOptions) {
        self.commands.push(DrawCommand::Text {
            x: options.x,
            y: options.y,
            text: text.into(),
            scale: options.scale,
            color: options.color.into(),
            align: options.align.unwrap_or(TextAlign::Left),
        });
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: impl Into<Color>, width: f64) {
        self.commands.push(DrawCommand::Line { x1, y1, x2, y2, color: color.into(), width });
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
    fn call(&mut self, id: &PluginId, method: &str, args: serde_json::Value) -> Result<serde_json::Value, String>;
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
        self.host.lock().map_err(|e| e.to_string())?.load_plugin(manifest)
    }

    pub fn unload(&mut self, id: &PluginId) -> Result<(), String> {
        self.host.lock().map_err(|e| e.to_string())?.unload_plugin(id)
    }
}

/// Workspace extension for widget registration
impl Workspace {
    pub fn register_widget(&self, manifest: WidgetManifest, _renderer: Box<dyn WidgetRenderer>) -> Result<(), String> {
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

    fn unload_plugin(&mut self, id: &PluginId) -> Result<(), String> {
        Ok(())
    }

    fn call(&mut self, _id: &PluginId, _method: &str, _args: serde_json::Value) -> Result<serde_json::Value, String> {
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

// Re-export for plugin authors
pub mod prelude {
    pub use super::{
        CommandManifest, Disposable, DrawCommand, Modifiers, MouseButton,
        PluginContext, PluginHost, PluginId, PluginManager, PluginManifest, PluginSDK, PermissionRequest,
        PluginStorage, PluginLogger, SettingSchema, SettingValue, TextAlign, TextOptions, UiBuilder,
        WidgetEvent, WidgetManifest, WidgetRenderContext, WidgetRenderer,
    };
    pub use crate::{Command, CommandContext, CommandId, CommandInputSchema, CommandRegistry, CommandResult, CommandValue, InputParam, KeyBinding, Theme, Workspace};
    pub use crate::surface::Color;
}