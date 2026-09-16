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
    /// Next digit slot for `save_next_bookmark` (cycles 0..10).
    pub bookmark_slot: usize,
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
            bookmark_slot: 0,
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
            bookmark_slot: layout.bookmark_slot,
        }
    }

    /// Convert camera to layout data
    pub fn to_layout(&self) -> CameraLayout {
        CameraLayout {
            x: self.x,
            y: self.y,
            zoom: self.zoom,
            bookmarks: self.bookmarks.clone(),
            bookmark_slot: self.bookmark_slot,
        }
    }

    /// Save a camera bookmark, replacing any existing bookmark with the
    /// same name instead of stacking duplicates.
    pub fn save_bookmark(&mut self, name: &str) {
        let bookmark = CameraBookmark {
            name: name.to_string(),
            x: self.x,
            y: self.y,
            zoom: self.zoom,
        };
        if let Some(existing) = self.bookmarks.iter_mut().find(|b| b.name == name) {
            *existing = bookmark;
        } else {
            self.bookmarks.push(bookmark);
        }
    }

    /// Save to the next rotating digit slot (`bm0`..`bm9`) and return its
    /// name, so a plain `b` press is always restorable via `0`–`9`.
    pub fn save_next_bookmark(&mut self) -> String {
        let name = format!("bm{}", self.bookmark_slot % 10);
        self.save_bookmark(&name);
        self.bookmark_slot = (self.bookmark_slot + 1) % 10;
        name
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
        let rw = rect.width.max(1.0);
        let rh = rect.height.max(1.0);
        let zoom = (view_w / rw).min(view_h / rh).clamp(0.05, 64.0);
        self.target_zoom = zoom;
        self.target_x = rect.x - (view_w / zoom - rw) / 2.0;
        self.target_y = rect.y - (view_h / zoom - rh) / 2.0;
        self.animating = true;
    }

    /// Zoom directly to a node's bounds, centered in the viewport.
    pub fn zoom_to_node(
        &mut self,
        node_id: NodeId,
        transform: crate::Transform,
        size: (f64, f64),
        view_w: f64,
        view_h: f64,
    ) {
        let rect = Rect {
            x: transform.x,
            y: transform.y,
            width: size.0.max(1.0),
            height: size.1.max(1.0),
        };
        let _ = node_id;
        self.fit_rect(rect, view_w, view_h);
    }

    /// Fit the camera to a rectangle
    pub fn fit_to(&mut self, rect: Rect) {
        self.target_zoom = 1.0;
        self.target_x = rect.x;
        self.target_y = rect.y;
        self.animating = true;
    }

    /// Zoom to a rectangle
    pub fn zoom_to_rect(&mut self, rect: Rect, duration_ms: u64) {
        self.target_zoom = 2.0;
        self.target_x = rect.x;
        self.target_y = rect.y;
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

    /// Pan the camera by delta. Direct manipulation: current and target
    /// move together and any in-flight animation is cancelled, so the
    /// camera never steers itself back after the user pans.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        self.target_x = self.x;
        self.target_y = self.y;
        self.target_zoom = self.zoom;
        self.animating = false;
    }

    /// Smoothing rate (1/s) for animated moves: converges ~99% in 0.33s.
    /// Fast enough to feel instant, slow enough to read as motion.
    pub const SMOOTH_RATE: f64 = 14.0;

    /// Update the camera toward its target (frame-rate independent).
    /// Only acts while `animating`; direct user drags cancel animation
    /// via `pan`, so this never fights the user.
    pub fn update(&mut self, dt: f64) {
        if !self.animating {
            return;
        }
        let k = 1.0 - (-Self::SMOOTH_RATE * dt.max(0.0)).exp();
        self.zoom += (self.target_zoom - self.zoom) * k;
        self.x += (self.target_x - self.x) * k;
        self.y += (self.target_y - self.y) * k;
        if (self.target_zoom - self.zoom).abs() < 0.001
            && (self.target_x - self.x).abs() < 0.05
            && (self.target_y - self.y).abs() < 0.05
        {
            self.zoom = self.target_zoom;
            self.x = self.target_x;
            self.y = self.target_y;
            self.animating = false;
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
    #[serde(default)]
    pub bookmark_slot: usize,
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
                x: 200.0,
                y: 300.0,
                width: 100.0,
                height: 100.0,
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
        let t = crate::Transform::at(0.0, 0.0);
        cam.zoom_to_node(NodeId(1), t, (640.0, 400.0), 800.0, 600.0);
        assert!((cam.target_zoom - 1.25).abs() < 1e-9); // min(800/640, 600/400)
        assert!(cam.animating);
    }

    #[test]
    fn test_animated_convergence_snaps_exact() {
        let mut cam = Camera::new();
        cam.target_x = 100.0;
        cam.target_y = 50.0;
        cam.target_zoom = 2.0;
        cam.animating = true;
        for _ in 0..60 {
            cam.update(1.0 / 60.0);
            if !cam.animating {
                break;
            }
        }
        assert!(!cam.animating);
        assert_eq!((cam.x, cam.y, cam.zoom), (100.0, 50.0, 2.0));
    }

    #[test]
    fn test_pan_cancels_animation() {
        let mut cam = Camera::new();
        cam.target_x = 100.0;
        cam.target_y = 50.0;
        cam.target_zoom = 2.0;
        cam.animating = true;
        cam.pan(5.0, -3.0);
        assert!(!cam.animating);
        assert_eq!((cam.target_x, cam.target_y), (cam.x, cam.y));
        assert_eq!(cam.target_zoom, cam.zoom);
    }

    #[test]
    fn test_save_bookmark_upserts_same_name() {
        let mut cam = Camera::new();
        cam.x = 1.0;
        cam.save_bookmark("bm1");
        cam.x = 42.0;
        cam.save_bookmark("bm1");
        assert_eq!(cam.bookmarks.len(), 1);
        assert_eq!(cam.bookmarks[0].x, 42.0);
    }

    #[test]
    fn test_save_next_bookmark_rotates_slots() {
        let mut cam = Camera::new();
        assert_eq!(cam.save_next_bookmark(), "bm0");
        assert_eq!(cam.save_next_bookmark(), "bm1");
        for _ in 2..10 {
            cam.save_next_bookmark();
        }
        // Full cycle wraps back to bm0 without stacking duplicates.
        assert_eq!(cam.save_next_bookmark(), "bm0");
        assert_eq!(cam.bookmarks.len(), 10);
        assert!(cam.restore_bookmark("bm0"));
    }

    #[test]
    fn test_bookmark_slot_round_trips_layout() {
        let mut cam = Camera::new();
        cam.save_next_bookmark();
        cam.save_next_bookmark();
        let layout = cam.to_layout();
        let restored = Camera::from_layout(&layout);
        assert_eq!(restored.bookmark_slot, 2);
        assert_eq!(restored.bookmarks.len(), 2);
    }
}
