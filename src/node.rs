//! Node - a positioned object in the workspace.

use super::*;
use std::collections::HashMap;

/// Components that a node can have
pub struct Node {
    /// The unique ID of this node
    pub id: NodeId,
    /// The transform component (position, scale, rotation)
    pub transform: Transform,
    /// The style component (colors, fonts)
    pub style: Theme,
    /// Whether the node has focus
    pub focus: bool,
    /// The input behavior mode
    pub input_mode: String,
    /// The surface ID this node renders
    pub surface_id: Option<SurfaceId>,
    /// The behavior type (terminal, widget, reading, etc.)
    pub behavior: String,
    /// Permissions for this node
    pub permissions: Vec<Permission>,
}

impl Node {
    /// Create a new node with default values
    pub fn new(id: NodeId, x: i32, y: i32) -> Self {
        Self {
            id,
            transform: Transform::new(x, y),
            style: Theme::default(),
            focus: false,
            input_mode: "default".to_string(),
            surface_id: None,
            behavior: "generic".to_string(),
            permissions: vec![],
        }
    }

    /// Set the surface for this node
    pub fn set_surface(&mut self, surface_id: SurfaceId) {
        self.surface_id = Some(surface_id);
    }

    /// Get the surface ID
    pub fn surface_id(&self) -> Option<SurfaceId> {
        self.surface_id
    }

    /// Set the behavior type
    pub fn set_behavior(&mut self, behavior: &str) {
        self.behavior = behavior.to_string();
    }

    /// Get the behavior type
    pub fn behavior(&self) -> &str {
        &self.behavior
    }

    /// Add a permission to this node
    pub fn add_permission(&mut self, permission: Permission) {
        self.permissions.push(permission);
    }
}