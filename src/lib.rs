//! Core primitives for Fracterm: Workspace, Node, Surface, Projection, Lens, Command, Event, Theme, Permission

pub mod app;
pub mod arrange;
pub mod camera;
pub mod canvas;
pub mod command;
pub mod config;
pub mod dts;
pub mod event;
pub mod input;
pub mod lens;
pub mod node;
pub mod permission;
pub mod plugin;
pub mod projection;
pub mod pty;
pub mod regions;
pub mod rendering;
pub mod script;
pub mod surface;
pub mod terminal;
pub mod text;
pub mod theme;
pub mod transform;
pub mod vt;
pub mod widget;
pub mod window;
pub mod workspace;

pub use app::{default_config, App, CliArgs};
pub use camera::{Camera, CameraBookmark};
pub use command::{
    builtin_commands, command_for_key, register_builtin_commands, BuiltinCommand, Command,
    CommandContext, CommandInputSchema, CommandRegistry, CommandResult, CommandValue, InputParam,
    KeyBinding,
};
pub use config::Config;
pub use event::{Event, EventBus, EventType};
pub use input::{
    ContextMenuBuilder, ContextMenuItem, DragState, InputContext, InputEvent, InputHandler,
    InputManager, InputResult, InteractionMode,
};
pub use lens::{CameraLens, Lens};
pub use node::{InputBehavior, Node, PluginBehavior};
pub use permission::{Permission, PermissionScope};
pub use plugin::{
    settings_ui_rows, CommandManifest, Disposable, DrawCommand, Modifiers, MouseButton,
    PermissionRequest, PluginContext, PluginHost, PluginId, PluginLogger, PluginManager,
    PluginManifest, PluginSDK, PluginStorage, SettingSchema, SettingUiRow, SettingValue, TextAlign,
    TextOptions, UiBuilder, V8Host, WidgetEvent, WidgetManifest, WidgetRenderContext,
    WidgetRenderer,
};
pub use projection::{
    ProjectionMode, ProjectionPresentation, ProjectionSelector, ProjectionSurface,
};
pub use rendering::{GlyphAtlas, RenderGraph, RenderGraphNode, RenderPass, RenderPassConfig};
pub use surface::Surface;
pub use terminal::{
    CommandOutputSource, FileTailSource, Terminal, TerminalCell, TerminalConfig, TerminalGrid,
    TerminalTextSource, TextSource,
};
pub use theme::Theme;
pub use transform::Transform;
pub use widget::{
    collect_widget_display_lists, render_widget_display_list, LiveWidget, WidgetDefinition,
    WidgetDisplayList, WidgetInit, WidgetRegistry, WidgetTimer, WidgetTimerFn, WidgetViewFn,
};
pub use workspace::{SceneGraph, Workspace};

/// Unique identifier for a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeId(pub u64);

/// Unique identifier for a surface
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SurfaceId(pub u64);

/// Unique identifier for a command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CommandId(pub u64);

/// Unique identifier for an event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EventId(pub u64);
