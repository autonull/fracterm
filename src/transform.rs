//! Transform - position, scale, and rotation for nodes.

use std::fmt::Display;

/// A transform representing position, scale, and rotation of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transform {
    /// Position (x, y) in pixels
    pub x: i32,
    /// Position (y) in pixels
    pub y: i32,
    /// Scale factor (multiplier)
    pub scale: f64,
    /// Rotation angle in radians
    pub rotation: f64,
}

impl Transform {
    /// Create a new transform at origin with unit scale and zero rotation
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            scale: 1.0,
            rotation: 0.0,
        }
    }

    /// Create a transform at given coordinates
    pub fn at(&self, x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            scale: 1.0,
            rotation: 0.0,
        }
    }

    /// Apply a translation to the transform
    pub fn translate(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    /// Apply a uniform scale multiplier
    pub fn set_scale(&mut self, factor: f64) {
        self.scale *= factor;
    }

    /// Rotate by angle in radians
    pub fn rotate(&mut self, degrees: f64) {
        self.rotation += degrees * std::f64::PI as f64 / 180.0;
    }

    /// Convert rotation to degrees
    pub fn to_degrees(&self) -> f64 {
        self.rotation * 180.0 / std::f64::PI as f64
    }

    /// Get the bounds of the transformed area
    pub fn bounds(&self) -> (i32, i32) {
        (self.x, self.y)
    }
}

impl Display for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Transform({}, {}, {})", self.x, self.y, self.scale)
    }
}