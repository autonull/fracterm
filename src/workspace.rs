//! Workspace - the infinite zoomable canvas containing nodes and a camera.

use super::*;

/// The workspace is the infinite zoomable canvas that contains nodes and a camera.
pub struct Workspace {
    /// The camera lens controlling the view
    camera: Camera,
    /// Nodes in the workspace
    nodes: Vec<NodeId>,
    /// Current zoom level
    zoom: f64,
    /// Current pan offset
    pan: (i32, i32),
    /// Whether the workspace is active
    active: bool,
}

impl Workspace {
    /// Create a new workspace
    pub fn new() -> Self {
        Self {
            camera: Camera::default(),
            nodes: vec![],
            zoom: 1.0,
            pan: (0, 0),
            active: true,
        }
    }

    /// Add a node to the workspace
    pub fn add_node(&mut self, node_id: NodeId) {
        self.nodes.push(node_id);
    }

    /// Get the camera
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Get all node IDs
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Set the zoom level
    pub fn set_zoom(&mut self, level: f64) {
        self.zoom = level;
    }

    /// Set the pan offset
    pub fn set_pan(&mut self, dx: i32, dy: i32) {
        self.pan = (dx, dy);
    }

    /// Check if the workspace is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}
