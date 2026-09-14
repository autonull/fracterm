//! Camera - the lens onto the workspace, controlling pan and zoom.

use super::*;

/// Camera state for the workspace
#[derive(Debug, Clone)]
pub struct Camera {
    /// X position of the camera
    pub x: f64,
    /// Y position of the camera
    pub y: f64,
    /// Zoom level (1.0 = 100%)
    pub zoom: f64,
    /// Whether the camera is animating
    pub animating: bool,
    /// Current target for animation
    pub target_zoom: f64,
    pub target_x: f64,
    pub target_y: f64,
}

impl Camera {
    /// Create a new camera
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            animating: false,
            target_zoom: 1.0,
            target_x: 0.0,
            target_y: 0.0,
        }
    }

    /// Set the camera position
    pub fn set_position(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
    }

    /// Set the zoom level
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom;
        self.target_zoom = zoom;
    }

    /// Check if camera is animating
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Fit the camera to a rectangle
    pub fn fit_to(&mut self, rect: Rect) {
        self.target_zoom = 1.0;
        self.target_x = rect.x as f64;
        self.target_y = rect.y as f64;
        self.animating = true;
    }

    /// Zoom to a rectangle
    pub fn zoom_to_rect(&mut self, rect: Rect, duration_ms: u64) {
        self.target_zoom = 2.0;
        self.target_x = rect.x as f64;
        self.target_y = rect.y as f64;
        self.animating = true;
        let _ = duration_ms;
    }

    /// Zoom to an object
    pub fn zoom_to_object(&mut self, node_id: NodeId) {
        self.target_zoom = 2.0;
        self.animating = true;
        let _ = node_id;
    }

    /// Zoom to a terminal range
    pub fn zoom_to_terminal_range(&mut self, _terminal_id: NodeId, _range: GridRange) {
        self.target_zoom = 4.0;
        self.animating = true;
    }

    /// Zoom to workspace fit
    pub fn zoom_to_workspace_fit(&mut self) {
        self.target_zoom = 1.0;
        self.target_x = 0.0;
        self.target_y = 0.0;
        self.animating = true;
    }

    /// Pan the camera by delta
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        self.target_x = self.x;
        self.target_y = self.y;
    }

    /// Update the camera toward its target
    pub fn update(&mut self, dt: f64) {
        if self.animating {
            self.zoom += (self.target_zoom - self.zoom) * dt * 0.1;
            self.x += (self.target_x - self.x) * dt * 0.1;
            self.y += (self.target_y - self.y) * dt * 0.1;
            if (self.target_zoom - self.zoom).abs() < 0.01
                && (self.target_x - self.x).abs() < 0.1
                && (self.target_y - self.y).abs() < 0.1
            {
                self.zoom = self.target_zoom;
                self.x = self.target_x;
                self.y = self.target_y;
                self.animating = false;
            }
        }
    }
}