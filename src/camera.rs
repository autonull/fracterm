//! Camera - the lens onto the workspace, controlling pan and zoom.
//!
//! The camera is a lens onto the workspace. Zooming into a terminal rectangle
//! is a lens onto a terminal surface. Camera bookmarks are lens targets.

use super::*;
use crate::lens::{GridRange, Rect};

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
    /// Saved camera positions
    pub bookmarks: Vec<CameraBookmark>,
}

/// A saved camera position/bookmark
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraBookmark {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
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
            bookmarks: Vec::new(),
        }
    }

    /// Create a camera from layout data
    pub fn from_layout(layout: &CameraLayout) -> Self {
        Self {
            x: layout.x,
            y: layout.y,
            zoom: layout.zoom,
            animating: false,
            target_zoom: layout.zoom,
            target_x: layout.x,
            target_y: layout.y,
            bookmarks: layout.bookmarks.clone(),
        }
    }

    /// Convert camera to layout data
    pub fn to_layout(&self) -> CameraLayout {
        CameraLayout {
            x: self.x,
            y: self.y,
            zoom: self.zoom,
            bookmarks: self.bookmarks.clone(),
        }
    }

    /// Save a camera bookmark
    pub fn save_bookmark(&mut self, name: &str) {
        self.bookmarks.push(CameraBookmark {
            name: name.to_string(),
            x: self.x,
            y: self.y,
            zoom: self.zoom,
        });
    }

    /// Get a bookmark by name
    pub fn get_bookmark(&self, name: &str) -> Option<&CameraBookmark> {
        self.bookmarks.iter().find(|b| b.name == name)
    }

    /// Restore camera to a bookmark position
    pub fn restore_bookmark(&mut self, name: &str) -> bool {
        if let Some(bookmark) = self.get_bookmark(name) {
            let (x, y, zoom) = (bookmark.x, bookmark.y, bookmark.zoom);
            self.x = x;
            self.y = y;
            self.zoom = zoom;
            self.target_x = x;
            self.target_y = y;
            self.target_zoom = zoom;
            self.animating = true;
            return true;
        }
        false
    }

    /// List all bookmark names
    pub fn bookmark_names(&self) -> Vec<String> {
        self.bookmarks.iter().map(|b| b.name.clone()).collect()
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

    /// Fit a world rectangle into the viewport, centered, with easing.
    pub fn fit_rect(&mut self, rect: Rect, view_w: f64, view_h: f64) {
        let rw = rect.width.max(1) as f64;
        let rh = rect.height.max(1) as f64;
        let zoom = (view_w / rw).min(view_h / rh).clamp(0.05, 64.0);
        self.target_zoom = zoom;
        self.target_x = rect.x as f64 - (view_w / zoom - rw) / 2.0;
        self.target_y = rect.y as f64 - (view_h / zoom - rh) / 2.0;
        self.animating = true;
    }

    /// Zoom directly to a node's bounds, centered in the viewport.
    pub fn zoom_to_node(
        &mut self,
        node_id: NodeId,
        transform: crate::Transform,
        size: (i32, i32),
        view_w: f64,
        view_h: f64,
    ) {
        let rect = Rect {
            x: transform.x,
            y: transform.y,
            width: size.0.max(1) as u32,
            height: size.1.max(1) as u32,
        };
        let _ = node_id;
        self.fit_rect(rect, view_w, view_h);
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

/// Camera layout state for persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraLayout {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub bookmarks: Vec<CameraBookmark>,
}

/// Camera bookmark for persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraBookmarkLayout {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::Rect;

    #[test]
    fn test_fit_rect_centers_and_scales() {
        let mut cam = Camera::new();
        // Viewport 1000x500, rect 100x100 at (200, 300).
        cam.fit_rect(
            Rect {
                x: 200,
                y: 300,
                width: 100,
                height: 100,
            },
            1000.0,
            500.0,
        );
        // Zoom limited by the smaller axis: 500/100 = 5.
        assert!((cam.target_zoom - 5.0).abs() < 1e-9);
        // Visible world width = 200; rect centered => x = 200 - (200-100)/2 = 150.
        assert!((cam.target_x - 150.0).abs() < 1e-9);
        assert!(cam.animating);
    }

    #[test]
    fn test_zoom_to_node_uses_size() {
        let mut cam = Camera::new();
        let t = crate::Transform::at(0, 0);
        cam.zoom_to_node(NodeId(1), t, (640, 400), 800.0, 600.0);
        assert!((cam.target_zoom - 1.25).abs() < 1e-9); // min(800/640, 600/400)
        assert!(cam.animating);
    }
}
