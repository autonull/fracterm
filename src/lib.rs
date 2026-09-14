//! Core primitives for Fracterm: Workspace, Node, Surface, Projection, Lens, Command, Event, Theme, Permission

pub mod app;
pub mod arrange;
pub mod camera;
pub mod canvas;
pub mod command;
pub mod config;
pub mod event;
pub mod lens;
pub mod node;
pub mod permission;
pub mod plugin;
pub mod projection;
pub mod pty;
pub mod rendering;
pub mod script;
pub mod surface;
pub mod terminal;
pub mod text;
pub mod theme;
pub mod transform;
pub mod vt;
pub mod window;
pub mod workspace;

pub use app::{default_config, App, CliArgs};
pub use camera::{Camera, CameraBookmark};
pub use command::Command;
pub use config::Config;
pub use event::{Event, EventBus};
pub use lens::{CameraLens, Lens};
pub use node::{InputBehavior, Node, PluginBehavior};
pub use permission::{Permission, PermissionScope};
pub use plugin::{Plugin, PluginSDK, V8Host, Widget};
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
pub use workspace::{SceneGraph, Workspace};

/// Unique identifier for a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

/// Unique identifier for a surface
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub u64);

/// Unique identifier for a command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId(pub u64);

/// Unique identifier for an event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(pub u64);
