//! Lens - a way of viewing a surface or projection.

use super::*;
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
        self.target = target;
        self.apply_target(workspace);
    }

    fn apply_target(&mut self, workspace: &Workspace) {
        match &self.target {
            ZoomTarget::Object { node_id } => {
                if let Some(transform) = workspace.node_transform(*node_id) {
                    self.camera.target_x = transform.x as f64;
                    self.camera.target_y = transform.y as f64;
                    self.camera.target_zoom = 1.0;
                    self.camera.animating = true;
                }
            }
            _ => {
                // TODO: implement other zoom targets
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
#[derive(Debug, Clone)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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
