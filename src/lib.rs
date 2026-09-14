//! Core primitives for Fracterm: Workspace, Node, Surface, Projection, Lens, Command, Event, Theme, Permission

use std::collections::HashMap;
use std::sync::Arc;

pub mod camera;
pub mod command;
pub mod event;
pub mod lens;
pub mod node;
pub mod permission;
pub mod surface;
pub mod theme;
pub mod transform;
pub mod workspace;

pub use camera::Camera;
pub use command::Command;
pub use event::{Event, EventBus};
pub use lens::Lens;
pub use node::Node;
pub use permission::{Permission, PermissionScope};
pub use surface::{Surface, SurfaceId};
pub use theme::Theme;
pub use transform::Transform;
pub use workspace::Workspace;

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