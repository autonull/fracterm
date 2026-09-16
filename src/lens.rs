//! Lens - a way of viewing a surface or projection.

use super::*;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

/// CameraLens is the camera's view of the workspace.
/// It represents the current viewing lens and can be pinned or animated.
#[derive(Debug, Clone)]
pub struct CameraLens {
    /// The camera position and zoom
    pub camera: Camera,
    /// Current zoom target
    pub target: ZoomTarget,
    /// Whether this lens is pinned
    pub pinned: bool,
    /// Whether this is a reading mode lens
    pub reading_mode: bool,
}

impl CameraLens {
    pub fn new(camera: Camera) -> Self {
        Self {
            camera,
            target: ZoomTarget::WorkspaceFit,
            pinned: false,
            reading_mode: false,
        }
    }

    pub fn zoom_to(&mut self, target: ZoomTarget, workspace: &Workspace) {
        self.zoom_to_viewport(target, workspace, 1280.0, 720.0);
    }

    /// Zoom to a target fitted into a `view_w` x `view_h` viewport.
    /// All branches are animated moves: they set the camera target and
    /// `animating = true` (never write current position directly).
    pub fn zoom_to_viewport(
        &mut self,
        target: ZoomTarget,
        workspace: &Workspace,
        view_w: f64,
        view_h: f64,
    ) {
        self.target = target.clone();
        self.apply_target(workspace, &target, view_w, view_h);
    }

    fn apply_target(
        &mut self,
        workspace: &Workspace,
        target: &ZoomTarget,
        view_w: f64,
        view_h: f64,
    ) {
        match target {
            ZoomTarget::WorkspaceFit => {
                if let Some(bounds) = workspace_content_bounds(workspace) {
                    let margin = 24.0;
                    self.camera.fit_rect(
                        Rect {
                            x: bounds.x - margin,
                            y: bounds.y - margin,
                            width: bounds.width + 2.0 * margin,
                            height: bounds.height + 2.0 * margin,
                        },
                        view_w,
                        view_h,
                    );
                    self.camera.target_zoom = self.camera.target_zoom.min(1.0);
                } else {
                    self.camera.zoom_to_workspace_fit();
                }
            }
            ZoomTarget::Object { node_id } => {
                if let Some(node) = workspace.get_node(*node_id) {
                    self.camera.fit_rect(
                        Rect {
                            x: node.transform.x,
                            y: node.transform.y,
                            width: node.size.0.max(1.0),
                            height: node.size.1.max(1.0),
                        },
                        view_w,
                        view_h,
                    );
                }
            }
            ZoomTarget::Rectangle { rect } => {
                self.camera.fit_rect(rect.clone(), view_w, view_h);
            }
            ZoomTarget::TerminalRange {
                terminal_id,
                range: _,
            } => {
                // Grid dims live in the window session, not the workspace,
                // so the lens fits the whole terminal node here. Callers
                // with live dims resolve the sub-rect via
                // `terminal_range_rect` and zoom to `Rectangle` instead.
                if let Some(node) = workspace.get_node(*terminal_id) {
                    self.camera.fit_rect(
                        Rect {
                            x: node.transform.x,
                            y: node.transform.y,
                            width: node.size.0.max(1.0),
                            height: node.size.1.max(1.0),
                        },
                        view_w,
                        view_h,
                    );
                }
            }
            ZoomTarget::Projection { projection_id } => {
                if let Some(node) = resolve_projection_node(workspace, projection_id) {
                    self.camera.fit_rect(
                        Rect {
                            x: node.transform.x,
                            y: node.transform.y,
                            width: node.size.0.max(1.0),
                            height: node.size.1.max(1.0),
                        },
                        view_w,
                        view_h,
                    );
                }
            }
            ZoomTarget::Reading {
                source_id,
                range: _,
            } => {
                self.reading_mode = true;
                if let Some(node) = workspace
                    .all_nodes()
                    .into_iter()
                    .find(|n| n.surface_id == Some(*source_id))
                {
                    // As with TerminalRange, precise sub-rect zoom needs
                    // live grid dims; fit the whole source node here.
                    self.camera.fit_rect(
                        Rect {
                            x: node.transform.x,
                            y: node.transform.y,
                            width: node.size.0.max(1.0),
                            height: node.size.1.max(1.0),
                        },
                        view_w,
                        view_h,
                    );
                }
            }
        }
    }

    pub fn pin(&mut self) {
        self.pinned = true;
    }

    pub fn unpin(&mut self) {
        self.pinned = false;
    }

    pub fn enter_reading_mode(&mut self) {
        self.reading_mode = true;
    }

    pub fn exit_reading_mode(&mut self) {
        self.reading_mode = false;
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn is_reading_mode(&self) -> bool {
        self.reading_mode
    }
}

impl Deref for CameraLens {
    type Target = Camera;

    fn deref(&self) -> &Self::Target {
        &self.camera
    }
}

impl DerefMut for CameraLens {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.camera
    }
}

/// Types of zoom targets
#[derive(Debug, Clone)]
pub enum ZoomTarget {
    /// Fit the entire workspace
    WorkspaceFit,
    /// Zoom to a specific node
    Object { node_id: NodeId },
    /// Zoom to a rectangle
    Rectangle { rect: Rect },
    /// Zoom to a terminal grid range
    TerminalRange {
        terminal_id: NodeId,
        range: GridRange,
    },
    /// Zoom to a projection
    Projection { projection_id: String },
    /// Reading mode with optional range
    Reading {
        source_id: SurfaceId,
        range: Option<GridRange>,
    },
}

/// A rectangular region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A grid range for terminal selection
#[derive(Debug, Clone)]
pub struct GridRange {
    pub start_row: u32,
    pub end_row: u32,
    pub start_col: u32,
    pub end_col: u32,
}

/// Lens represents a way of viewing a surface or projection.
/// The camera is a lens onto the workspace.
/// Zooming into a terminal is a lens onto a terminal surface.
/// Pinned subrange view is a node whose surface is a projection.
/// Reading mode is a lens with accessibility presentation rules.
#[allow(dead_code)]
pub struct Lens {
    /// Current zoom target
    target: ZoomTarget,
    /// Zoom level (1.0 = 100%)
    zoom: f64,
    /// Pan offset
    pan_x: i32,
    pan_y: i32,
    /// Whether this lens is pinned (materialized as a view)
    pinned: bool,
    /// Whether this is a reading mode lens
    reading_mode: bool,
}

impl Lens {
    /// Create a default lens at workspace fit
    pub fn new() -> Self {
        Self {
            target: ZoomTarget::WorkspaceFit,
            zoom: 1.0,
            pan_x: 0,
            pan_y: 0,
            pinned: false,
            reading_mode: false,
        }
    }

    /// Zoom to a target with optional animation
    pub fn zoom_to(&mut self, target: ZoomTarget, duration_ms: u64) {
        self.target = target;
        self.apply_zoom_animation(duration_ms);
    }

    /// Pin this lens as a materialized view
    pub fn pin(&mut self) {
        self.pinned = true;
    }

    /// Unpin this lens
    pub fn unpin(&mut self) {
        self.pinned = false;
    }

    /// Enter reading mode lens
    pub fn enter_reading_mode(&mut self) {
        self.reading_mode = true;
    }

    /// Exit reading mode lens
    pub fn exit_reading_mode(&mut self) {
        self.reading_mode = false;
    }

    /// Check if lens is pinned
    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    /// Check if this is a reading mode lens
    pub fn is_reading_mode(&self) -> bool {
        self.reading_mode
    }

    /// Apply zoom animation
    fn apply_zoom_animation(&mut self, _duration_ms: u64) {
        // Animation would be implemented here
    }
}

impl Default for Lens {
    fn default() -> Self {
        Self::new()
    }
}

/// Content bounds of every node in the workspace, or `None` when empty.
/// Shared by `WorkspaceFit` zoom and dashboard fitting so both agree on
/// what "everything" means.
pub fn workspace_content_bounds(workspace: &Workspace) -> Option<Rect> {
    let nodes = workspace.all_nodes();
    if nodes.is_empty() {
        return None;
    }
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for n in nodes {
        x0 = x0.min(n.transform.x);
        y0 = y0.min(n.transform.y);
        x1 = x1.max(n.transform.x + n.size.0.max(1.0));
        y1 = y1.max(n.transform.y + n.size.1.max(1.0));
    }
    Some(Rect {
        x: x0,
        y: y0,
        width: (x1 - x0).max(1.0),
        height: (y1 - y0).max(1.0),
    })
}

/// Sub-rectangle of a terminal node covering a grid range, given the live
/// grid dims (`cols`, `rows`). The range is clamped into the grid; uniform
/// cells are assumed. Window code with live `Terminal` dims calls this and
/// zooms to `ZoomTarget::Rectangle`, since the workspace alone cannot know
/// grid geometry.
pub fn terminal_range_rect(
    node_x: f64,
    node_y: f64,
    node_w: f64,
    node_h: f64,
    range: &GridRange,
    cols: u32,
    rows: u32,
) -> Rect {
    let cols = cols.max(1) as f64;
    let rows = rows.max(1) as f64;
    let cell_w = node_w.max(1.0) / cols;
    let cell_h = node_h.max(1.0) / rows;
    let sc = range.start_col.min(range.end_col).min(cols as u32) as f64;
    let ec = range.start_col.max(range.end_col).min(cols as u32) as f64;
    let sr = range.start_row.min(range.end_row).min(rows as u32) as f64;
    let er = range.start_row.max(range.end_row).min(rows as u32) as f64;
    Rect {
        x: node_x + sc * cell_w,
        y: node_y + sr * cell_h,
        width: ((ec - sc).max(1.0)) * cell_w,
        height: ((er - sr).max(1.0)) * cell_h,
    }
}

/// Resolve a projection zoom id to a node: first try it as a node id, then
/// as a surface id, then fall back to the first node carrying a projection.
fn resolve_projection_node<'a>(workspace: &'a Workspace, projection_id: &str) -> Option<&'a Node> {
    if let Ok(n) = projection_id.parse::<u64>() {
        if let Some(node) = workspace.get_node(NodeId(n)) {
            return Some(node);
        }
        let surface = SurfaceId(n);
        if let Some(node) = workspace
            .all_nodes()
            .into_iter()
            .find(|n| n.surface_id == Some(surface))
        {
            return Some(node);
        }
    }
    workspace
        .all_nodes()
        .into_iter()
        .find(|n| n.projection.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with_node(id: u64, x: f64, y: f64, w: f64, h: f64) -> (Workspace, NodeId) {
        let mut ws = Workspace::new();
        let nid = NodeId(id);
        let mut node = Node::new(nid, x, y);
        node.size = (w, h);
        ws.add_node(node);
        (ws, nid)
    }

    #[test]
    fn test_workspace_fit_animates_to_content() {
        let (mut ws, _) = workspace_with_node(1, 0.0, 0.0, 400.0, 300.0);
        let snap = ws_snapshot(&ws);
        ws.camera
            .zoom_to_viewport(ZoomTarget::WorkspaceFit, &snap, 1280.0, 720.0);
        assert!(ws.camera.animating);
        assert!(ws.camera.target_zoom <= 1.0);
    }

    // Snapshot the node list without the camera borrow conflict: rebuild a
    // throwaway workspace sharing the same nodes for target resolution.
    fn ws_snapshot(ws: &Workspace) -> Workspace {
        let mut out = Workspace::new();
        for n in ws.all_nodes() {
            out.add_node(n.clone());
        }
        out
    }

    #[test]
    fn test_object_zoom_fits_node() {
        let (mut ws, nid) = workspace_with_node(1, 100.0, 50.0, 400.0, 300.0);
        let snap = ws_snapshot(&ws);
        ws.camera
            .zoom_to_viewport(ZoomTarget::Object { node_id: nid }, &snap, 1280.0, 720.0);
        assert!(ws.camera.animating);
        let mut expect = Camera::new();
        expect.fit_rect(
            Rect {
                x: 100.0,
                y: 50.0,
                width: 400.0,
                height: 300.0,
            },
            1280.0,
            720.0,
        );
        assert!((ws.camera.target_zoom - expect.target_zoom).abs() < 1e-9);
        assert!((ws.camera.target_x - expect.target_x).abs() < 1e-9);
    }

    #[test]
    fn test_rectangle_zoom_matches_fit_rect() {
        let (mut ws, _) = workspace_with_node(1, 0.0, 0.0, 400.0, 300.0);
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let snap = ws_snapshot(&ws);
        ws.camera.zoom_to_viewport(
            ZoomTarget::Rectangle { rect: rect.clone() },
            &snap,
            800.0,
            600.0,
        );
        let mut expect = Camera::new();
        expect.fit_rect(rect, 800.0, 600.0);
        assert!((ws.camera.target_zoom - expect.target_zoom).abs() < 1e-9);
    }

    #[test]
    fn test_terminal_range_rect_subdivides_node() {
        let range = GridRange {
            start_row: 0,
            end_row: 12,
            start_col: 0,
            end_col: 40,
        };
        let rect = terminal_range_rect(0.0, 0.0, 800.0, 600.0, &range, 80, 24);
        assert!((rect.x - 0.0).abs() < 1e-9);
        assert!((rect.width - 400.0).abs() < 1e-9);
        assert!((rect.height - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_terminal_range_clamps_to_grid() {
        let range = GridRange {
            start_row: 0,
            end_row: 100,
            start_col: 0,
            end_col: 200,
        };
        let rect = terminal_range_rect(0.0, 0.0, 800.0, 600.0, &range, 80, 24);
        assert!(rect.width <= 800.0 + 1e-9);
        assert!(rect.height <= 600.0 + 1e-9);
    }

    #[test]
    fn test_reading_zoom_sets_reading_mode() {
        let mut ws = Workspace::new();
        let mut node = Node::new(NodeId(7), 0.0, 0.0);
        node.size = (400.0, 300.0);
        node.surface_id = Some(SurfaceId(42));
        ws.add_node(node);
        let snap = ws_snapshot(&ws);
        ws.camera.zoom_to_viewport(
            ZoomTarget::Reading {
                source_id: SurfaceId(42),
                range: None,
            },
            &snap,
            1280.0,
            720.0,
        );
        assert!(ws.camera.reading_mode);
        assert!(ws.camera.animating);
    }

    #[test]
    fn test_unknown_object_zoom_is_noop() {
        let (mut ws, _) = workspace_with_node(1, 0.0, 0.0, 400.0, 300.0);
        let snap = ws_snapshot(&ws);
        ws.camera.zoom_to_viewport(
            ZoomTarget::Object {
                node_id: NodeId(999),
            },
            &snap,
            1280.0,
            720.0,
        );
        assert!(!ws.camera.animating);
    }
}
