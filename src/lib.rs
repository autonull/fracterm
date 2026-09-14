//! Core primitives for Fracterm: Workspace, Node, Surface, Projection, Lens, Command, Event, Theme, Permission

pub mod app;
pub mod camera;
pub mod command;
pub mod config;
pub mod event;
pub mod lens;
pub mod node;
pub mod permission;
pub mod plugin;
pub mod projection;
pub mod rendering;
pub mod surface;
pub mod terminal;
pub mod theme;
pub mod transform;
pub mod workspace;

pub use app::{App, CliArgs, default_config};
pub use camera::{Camera, CameraBookmark};
pub use command::Command;
pub use config::Config;
pub use event::{Event, EventBus};
pub use lens::{CameraLens, Lens};
pub use node::{InputBehavior, Node, PluginBehavior};
pub use permission::{Permission, PermissionScope};
pub use plugin::{Plugin, PluginSDK, V8Host, Widget};
pub use projection::{ProjectionSurface, ProjectionSelector, ProjectionPresentation, ProjectionMode};
pub use rendering::{RenderGraph, RenderPass, RenderPassConfig, RenderGraphNode, GlyphAtlas};
pub use surface::Surface;
pub use terminal::{Terminal, TerminalConfig, TerminalGrid, TerminalCell, TextSource, TerminalTextSource, CommandOutputSource, FileTailSource};
pub use theme::Theme;
pub use transform::Transform;
pub use workspace::{Workspace, SceneGraph};

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