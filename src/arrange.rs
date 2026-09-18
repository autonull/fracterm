//! Dashboard arrangement commands (README M8 / §11): tiling, alignment,
//! distribution, and snapping. Pure functions over node transforms/sizes.

use crate::node::Node;
use crate::NodeId;

/// Layout gap in world pixels between tiled nodes.
pub const DEFAULT_GAP: f64 = 16.0;

fn sizes(nodes: &[&Node]) -> Vec<(f64, f64)> {
    nodes.iter().map(|n| (n.size.0, n.size.1)).collect()
}

/// Tile nodes left-to-right in one row, preserving vertical order of
/// the first node. Returns (node_id, x, y) placement list.
pub fn tile_horizontally(nodes: &[&Node], gap: f64) -> Vec<(NodeId, f64, f64)> {
    let sizes = sizes(nodes);
    let mut out = Vec::with_capacity(nodes.len());
    let mut x = nodes.first().map(|n| n.transform.x).unwrap_or(0.0);
    let y = nodes.first().map(|n| n.transform.y).unwrap_or(0.0);
    for (node, (w, _)) in nodes.iter().zip(sizes) {
        out.push((node.id, x, y));
        x += w + gap;
    }
    out
}

/// Tile nodes top-to-bottom in one column.
pub fn tile_vertically(nodes: &[&Node], gap: f64) -> Vec<(NodeId, f64, f64)> {
    let sizes = sizes(nodes);
    let mut out = Vec::with_capacity(nodes.len());
    let x = nodes.first().map(|n| n.transform.x).unwrap_or(0.0);
    let mut y = nodes.first().map(|n| n.transform.y).unwrap_or(0.0);
    for (node, (_, h)) in nodes.iter().zip(sizes) {
        out.push((node.id, x, y));
        y += h + gap;
    }
    out
}

/// Tile into a grid with `cols` columns. Cell size is the max of all node
/// sizes; every node fills its cell.
pub fn tile_grid(nodes: &[&Node], cols: usize, gap: f64) -> Vec<(NodeId, f64, f64)> {
    if nodes.is_empty() || cols == 0 {
        return Vec::new();
    }
    let sizes = sizes(nodes);
    let cell_w = sizes.iter().map(|s| s.0).fold(0.0, f64::max);
    let cell_h = sizes.iter().map(|s| s.1).fold(0.0, f64::max);
    let origin = (nodes[0].transform.x, nodes[0].transform.y);
    let mut out = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        out.push((
            node.id,
            origin.0 + col as f64 * (cell_w + gap),
            origin.1 + row as f64 * (cell_h + gap),
        ));
    }
    out
}

/// Cascade: offset each node diagonally by a fixed step.
pub fn cascade(nodes: &[&Node], step: f64) -> Vec<(NodeId, f64, f64)> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                n.id,
                n.transform.x + i as f64 * step,
                n.transform.y + i as f64 * step,
            )
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    CenterX,
    CenterY,
}

/// Align all nodes to the extreme edge of the current bounding box.
pub fn align(nodes: &[&Node], edge: Edge) -> Vec<(NodeId, f64, f64)> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let sizes = sizes(nodes);
    let max_right = nodes
        .iter()
        .zip(&sizes)
        .map(|(n, (w, _))| n.transform.x + w)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_bottom = nodes
        .iter()
        .zip(&sizes)
        .map(|(n, (_, h))| n.transform.y + h)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_left = nodes
        .iter()
        .map(|n| n.transform.x)
        .fold(f64::INFINITY, f64::min);
    let min_top = nodes
        .iter()
        .map(|n| n.transform.y)
        .fold(f64::INFINITY, f64::min);
    nodes
        .iter()
        .zip(&sizes)
        .map(|(n, (w, h))| {
            let (x, y) = match edge {
                Edge::Left => (min_left, n.transform.y),
                Edge::Right => (max_right - w, n.transform.y),
                Edge::Top => (n.transform.x, min_top),
                Edge::Bottom => (n.transform.x, max_bottom - h),
                Edge::CenterX => (min_left + (max_right - min_left - w) / 2.0, n.transform.y),
                Edge::CenterY => (n.transform.x, min_top + (max_bottom - min_top - h) / 2.0),
            };
            (n.id, x, y)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Distribute nodes with equal gaps between their bounding edges along
/// `axis`. The first and last nodes stay fixed.
pub fn distribute(nodes: &[&Node], axis: Axis) -> Vec<(NodeId, f64, f64)> {
    if nodes.len() < 3 {
        return Vec::new();
    }
    let sizes = sizes(nodes);
    let last = nodes.len() - 1;
    let (start, end, total_size): (f64, f64, f64) = match axis {
        Axis::Horizontal => (
            nodes[0].transform.x,
            nodes[last].transform.x + sizes[last].0,
            sizes.iter().map(|(w, _)| w).sum(),
        ),
        Axis::Vertical => (
            nodes[0].transform.y,
            nodes[last].transform.y + sizes[last].1,
            sizes.iter().map(|(_, h)| h).sum(),
        ),
    };
    let span = (end - start - total_size).max(0.0);
    let gap = span / (nodes.len() - 1) as f64;
    let mut out = Vec::with_capacity(nodes.len());
    let mut cursor = start;
    for (node, (w, h)) in nodes.iter().zip(sizes) {
        let size = match axis {
            Axis::Horizontal => w,
            Axis::Vertical => h,
        };
        let (x, y) = match axis {
            Axis::Horizontal => (cursor, node.transform.y),
            Axis::Vertical => (node.transform.x, cursor),
        };
        out.push((node.id, x, y));
        cursor += size + gap;
    }
    out
}

/// Snap a coordinate to the nearest grid multiple.
pub fn snap_to_grid(value: i32, grid: i32) -> i32 {
    if grid <= 0 {
        return value;
    }
    let half = grid / 2;
    if value.rem_euclid(grid) > half {
        value + grid - value.rem_euclid(grid)
    } else {
        value - value.rem_euclid(grid)
    }
}

/// Fractal-dashboard geometry: pure world/screen/NDC math shared by the
/// zooming canvas and its simulation tests.
///
/// World space is an infinite pixel plane. Screen space is physical window
/// pixels: `screen = (world - cam) * zoom`. NDC maps the viewport onto
/// `[-1, 1]`: `ndc = (2 * s / view - 1, 1 - 2 * s / view)`, matching the
/// canvas and glyph vertex shaders.
pub fn world_to_screen(wx: f64, wy: f64, cam_x: f64, cam_y: f64, zoom: f64) -> (f64, f64) {
    ((wx - cam_x) * zoom, (wy - cam_y) * zoom)
}

pub fn screen_to_ndc(sx: f64, sy: f64, view_w: f64, view_h: f64) -> (f64, f64) {
    (2.0 * sx / view_w - 1.0, 1.0 - 2.0 * sy / view_h)
}

pub fn world_to_ndc(
    wx: f64,
    wy: f64,
    cam_x: f64,
    cam_y: f64,
    zoom: f64,
    view_w: f64,
    view_h: f64,
) -> (f64, f64) {
    let (sx, sy) = world_to_screen(wx, wy, cam_x, cam_y, zoom);
    screen_to_ndc(sx, sy, view_w, view_h)
}

/// Grid cell baseline origin in world pixels for terminal dashboards.
#[allow(clippy::too_many_arguments)]
pub fn grid_cell_origin(
    node_x: f64,
    node_y: f64,
    pad_x: f64,
    header_h: f64,
    cell_w: f64,
    line_h: f64,
    col: u32,
    row: u32,
) -> (f64, f64) {
    (
        node_x + pad_x + cell_w * col as f64,
        node_y + header_h + line_h * row as f64,
    )
}

/// Startup dashboard: terminal node left, live-view node right, never
/// overlapping. Wraps the view node below when the viewport is narrow.
pub fn dashboard_layout(
    view_w: f64,
    term_w: f64,
    term_h: f64,
    view_node_w: f64,
    margin: f64,
    gap: f64,
) -> ((f64, f64), (f64, f64)) {
    let tx = margin;
    let ty = margin;
    let mut vx = tx + term_w + gap;
    let mut vy = margin;
    if vx + view_node_w > view_w - margin {
        vx = tx;
        vy = ty + term_h + gap;
    }
    ((tx, ty), (vx, vy))
}

/// Whether two rects overlap with nonzero area. Edge-touching counts as
/// disjoint (nothing visible to draw). Used for viewport culling: nodes
/// fully outside the visible world rect skip the text pass entirely.
#[allow(clippy::too_many_arguments)]
pub fn rects_intersect(
    ax: f64,
    ay: f64,
    aw: f64,
    ah: f64,
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
) -> bool {
    ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by
}

/// Place a new node beside an anchor: try the right side first; if the
/// new node would overflow the viewport width, wrap below the anchor.
/// `anchor` is (x, y, w, h), `size` is (w, h), `view` is (w, h).
pub fn place_beside(
    anchor: (f64, f64, f64, f64),
    size: (f64, f64),
    gap: f64,
    view: (f64, f64),
) -> (f64, f64) {
    let (ax, ay, aw, ah) = anchor;
    let (sw, _sh) = size;
    let rx = ax + aw + gap;
    if rx + sw <= view.0 {
        (rx, ay)
    } else {
        (ax, ay + ah + gap)
    }
}

/// Derive a terminal grid (cols, rows) from a node size and cell metrics.
/// Clamped to 2..=256 so tiny drags never kill the PTY.
pub fn terminal_grid_size(
    node_w: f64,
    node_h: f64,
    cell_w: f64,
    line_h: f64,
    header_h: f64,
    pad_x: f64,
    pad_bottom: f64,
) -> (u32, u32) {
    let cols = ((node_w - 2.0 * pad_x) / cell_w).floor().clamp(2.0, 256.0) as u32;
    let rows = ((node_h - header_h - pad_bottom) / line_h)
        .floor()
        .clamp(2.0, 256.0) as u32;
    (cols, rows)
}

/// Ideal startup grid: keep `rows` rows and choose columns so the node
/// aspect matches the viewport aspect — the camera fit then fills the
/// view with (almost) no letterbox waste on any side.
#[allow(clippy::too_many_arguments)]
pub fn ideal_grid_for_view(
    view_w: f64,
    view_h: f64,
    cell_w: f64,
    line_h: f64,
    header_h: f64,
    pad_x: f64,
    pad_bottom: f64,
    rows: u32,
) -> (u32, u32) {
    let rows = rows.clamp(2, 256);
    let node_h = header_h + rows as f64 * line_h + pad_bottom;
    let target_w = node_h * view_w / view_h.max(1.0);
    let cols = ((target_w - 2.0 * pad_x) / cell_w)
        .round()
        .clamp(2.0, 256.0) as u32;
    (cols, rows)
}

/// Pure resize-handle hit test: is the screen cursor within `threshold`
/// px of the node's bottom-right corner (world -> screen via camera)?
#[allow(clippy::too_many_arguments)]
pub fn resize_handle_hit(
    node_x: f64,
    node_y: f64,
    node_w: f64,
    node_h: f64,
    cam_x: f64,
    cam_y: f64,
    zoom: f64,
    cursor_sx: f64,
    cursor_sy: f64,
    threshold: f64,
) -> bool {
    let hx = (node_x + node_w - cam_x) * zoom;
    let hy = (node_y + node_h - cam_y) * zoom;
    (hx - cursor_sx).abs() <= threshold && (hy - cursor_sy).abs() <= threshold
}

/// A suggested snap: the node edge aligns with a nearby other edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapGuide {
    pub node_id: NodeId,
    /// Snapped x/y position.
    pub x: f64,
    pub y: f64,
    /// Distance from the original position, for guide rendering.
    pub distance: f64,
}

/// Represents a single alignment guide line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuideLine {
    /// Orientation of the guide line.
    pub orientation: GuideOrientation,
    /// Position in world coordinates.
    pub position: f64,
    /// The node edge that triggered this guide.
    pub node_id: NodeId,
    /// Which edge of the node aligned.
    pub edge: Edge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideOrientation {
    Horizontal,
    Vertical,
}

/// Result of snap-to-edges calculation with guide lines for rendering.
#[derive(Debug, Clone)]
pub struct SnapResult {
    /// Snapped position (x, y).
    pub x: f64,
    pub y: f64,
    /// Distance from original position.
    pub distance: f64,
    /// All guide lines that were activated during snapping.
    pub guides: Vec<GuideLine>,
}

/// Snap a moving node's rect to other nodes' edges within `threshold`.
/// Returns snapped position and guide lines for visual feedback.
pub fn snap_to_edges(moving: &Node, others: &[&Node], threshold: f64) -> SnapResult {
    let (mw, mh) = (moving.size.0, moving.size.1);
    let (mx, my) = (moving.transform.x, moving.transform.y);
    let moving_center_x = mx + mw / 2.0;
    let moving_center_y = my + mh / 2.0;

    let mut best: Option<SnapResult> = None;

    for other in others {
        if other.id == moving.id {
            continue;
        }
        let (ox, oy) = (other.transform.x, other.transform.y);
        let (ow, oh) = other.size;
        let other_center_x = ox + ow / 2.0;
        let other_center_y = oy + oh / 2.0;

        let mut guides = Vec::new();
        let mut snapped_x = mx;
        let mut snapped_y = my;
        let mut min_distance = f64::INFINITY;

        // Check edge alignments
        let alignments = [
            // (target_x, target_y, distance, guide_line)
            (
                mx,
                oy,
                (my - oy).abs(),
                GuideLine {
                    orientation: GuideOrientation::Horizontal,
                    position: oy,
                    node_id: other.id,
                    edge: Edge::Top,
                },
            ),
            (
                mx,
                oy + oh,
                (my - (oy + oh)).abs(),
                GuideLine {
                    orientation: GuideOrientation::Horizontal,
                    position: oy + oh,
                    node_id: other.id,
                    edge: Edge::Bottom,
                },
            ),
            (
                mx,
                oy - mh,
                (my - (oy - mh)).abs(),
                GuideLine {
                    orientation: GuideOrientation::Horizontal,
                    position: oy - mh,
                    node_id: other.id,
                    edge: Edge::Top,
                },
            ),
            (
                ox,
                my,
                (mx - ox).abs(),
                GuideLine {
                    orientation: GuideOrientation::Vertical,
                    position: ox,
                    node_id: other.id,
                    edge: Edge::Left,
                },
            ),
            (
                ox + ow,
                my,
                (mx - (ox + ow)).abs(),
                GuideLine {
                    orientation: GuideOrientation::Vertical,
                    position: ox + ow,
                    node_id: other.id,
                    edge: Edge::Right,
                },
            ),
            (
                ox - mw,
                my,
                (mx - (ox - mw)).abs(),
                GuideLine {
                    orientation: GuideOrientation::Vertical,
                    position: ox - mw,
                    node_id: other.id,
                    edge: Edge::Left,
                },
            ),
            // Center alignments
            (
                other_center_x - mw / 2.0,
                my,
                (moving_center_x - other_center_x).abs(),
                GuideLine {
                    orientation: GuideOrientation::Vertical,
                    position: other_center_x,
                    node_id: other.id,
                    edge: Edge::CenterX,
                },
            ),
            (
                mx,
                other_center_y - mh / 2.0,
                (moving_center_y - other_center_y).abs(),
                GuideLine {
                    orientation: GuideOrientation::Horizontal,
                    position: other_center_y,
                    node_id: other.id,
                    edge: Edge::CenterY,
                },
            ),
        ];

        for (cx, cy, d, guide) in alignments {
            if d <= threshold {
                guides.push(guide);
                if d < min_distance {
                    min_distance = d;
                    snapped_x = cx;
                    snapped_y = cy;
                }
            }
        }

        if !guides.is_empty()
            && best
                .as_ref()
                .map(|b| min_distance < b.distance)
                .unwrap_or(true)
        {
            best = Some(SnapResult {
                x: snapped_x,
                y: snapped_y,
                distance: min_distance,
                guides,
            });
        }
    }

    best.unwrap_or(SnapResult {
        x: mx,
        y: my,
        distance: f64::INFINITY,
        guides: Vec::new(),
    })
}

/// Snap a resizing node's bottom-right corner to other nodes' edges within `threshold`.
/// Returns snapped size and guide lines for visual feedback.
pub fn snap_resize_corner(moving: &mut Node, others: &[&Node], threshold: f64) -> SnapResult {
    let (mw, mh) = (moving.size.0, moving.size.1);
    let (mx, my) = (moving.transform.x, moving.transform.y);
    let corner_x = mx + mw;
    let corner_y = my + mh;

    let mut best: Option<SnapResult> = None;

    for other in others {
        if other.id == moving.id {
            continue;
        }
        let (ox, oy) = (other.transform.x, other.transform.y);
        let (ow, oh) = other.size;
        let other_right = ox + ow;
        let other_bottom = oy + oh;

        let mut guides = Vec::new();
        let mut snapped_w = mw;
        let mut snapped_h = mh;
        let mut min_distance = f64::INFINITY;

        // Check corner alignments (right edge to other's right, bottom edge to other's bottom)
        let alignments = [
            // Snap right edge to other's right edge
            (
                other_right - mx,
                mh,
                (corner_x - other_right).abs(),
                GuideLine {
                    orientation: GuideOrientation::Vertical,
                    position: other_right,
                    node_id: other.id,
                    edge: Edge::Right,
                },
            ),
            // Snap right edge to other's left edge
            (
                ox - mx,
                mh,
                (corner_x - ox).abs(),
                GuideLine {
                    orientation: GuideOrientation::Vertical,
                    position: ox,
                    node_id: other.id,
                    edge: Edge::Left,
                },
            ),
            // Snap bottom edge to other's bottom edge
            (
                mw,
                other_bottom - my,
                (corner_y - other_bottom).abs(),
                GuideLine {
                    orientation: GuideOrientation::Horizontal,
                    position: other_bottom,
                    node_id: other.id,
                    edge: Edge::Bottom,
                },
            ),
            // Snap bottom edge to other's top edge
            (
                mw,
                oy - my,
                (corner_y - oy).abs(),
                GuideLine {
                    orientation: GuideOrientation::Horizontal,
                    position: oy,
                    node_id: other.id,
                    edge: Edge::Top,
                },
            ),
        ];

        for (nw, nh, d, guide) in alignments {
            if nw >= 2.0 && nh >= 2.0 && d <= threshold {
                guides.push(guide);
                if d < min_distance {
                    min_distance = d;
                    snapped_w = nw;
                    snapped_h = nh;
                }
            }
        }

        if !guides.is_empty()
            && best
                .as_ref()
                .map(|b| min_distance < b.distance)
                .unwrap_or(true)
        {
            best = Some(SnapResult {
                x: mx,
                y: my,
                distance: min_distance,
                guides,
            });
            // Apply the snapped size to the moving node for the next iteration
            moving.size = (snapped_w, snapped_h);
        }
    }

    if let Some(res) = best {
        moving.size = (moving.size.0, moving.size.1); // Keep position, size already updated in loop
        res
    } else {
        SnapResult {
            x: mx,
            y: my,
            distance: f64::INFINITY,
            guides: Vec::new(),
        }
    }
}

pub fn cascade_from(nodes: &[&Node], step: f64) -> Vec<(NodeId, f64, f64)> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let (ox, oy) = (nodes[0].transform.x, nodes[0].transform.y);
    nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, ox + i as f64 * step, oy + i as f64 * step))
        .collect()
}

pub fn orbit(nodes: &[&Node], radius: f64) -> Vec<(NodeId, f64, f64)> {
    if nodes.is_empty() {
        return Vec::new();
    }
    if nodes.len() == 1 {
        return vec![(nodes[0].id, nodes[0].transform.x, nodes[0].transform.y)];
    }
    let (cx, cy) = centroid(nodes);
    let n = nodes.len() as f64;
    nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / n;
            let (w, h) = nd.size;
            (
                nd.id,
                cx + radius * a.cos() - w / 2.0,
                cy + radius * a.sin() - h / 2.0,
            )
        })
        .collect()
}

pub fn focus_ring(nodes: &[&Node], focused: NodeId, radius: f64) -> Vec<(NodeId, f64, f64)> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let (cx, cy) = centroid(nodes);
    let others: Vec<&&Node> = nodes.iter().filter(|n| n.id != focused).collect();
    let mut out = vec![(focused, cx, cy)];
    let n = others.len() as f64;
    for (i, nd) in others.iter().enumerate() {
        let a = 2.0 * std::f64::consts::PI * i as f64 / n.max(1.0);
        let (w, h) = nd.size;
        out.push((
            nd.id,
            cx + radius * a.cos() - w / 2.0,
            cy + radius * a.sin() - h / 2.0,
        ));
    }
    out
}

fn centroid(nodes: &[&Node]) -> (f64, f64) {
    let (sx, sy) = nodes.iter().fold((0.0, 0.0), |(ax, ay), n| {
        (
            ax + n.transform.x + n.size.0 / 2.0,
            ay + n.transform.y + n.size.1 / 2.0,
        )
    });
    let n = nodes.len() as f64;
    (sx / n, sy / n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurfaceId;

    fn node(id: u64, x: f64, y: f64, w: f64, h: f64) -> Node {
        let mut n = Node::new(NodeId(id), x, y);
        n.size = (w, h);
        n.set_surface(SurfaceId(id));
        n
    }

    fn refs(nodes: &mut [Node]) -> Vec<&Node> {
        nodes.iter().collect()
    }

    #[test]
    fn test_tile_horizontally() {
        let mut nodes = vec![
            node(1, 0.0, 0.0, 100.0, 50.0),
            node(2, 500.0, 0.0, 80.0, 50.0),
        ];
        let r = tile_horizontally(&refs(&mut nodes), 10.0);
        assert_eq!(r[0], (NodeId(1), 0.0, 0.0));
        assert_eq!(r[1], (NodeId(2), 110.0, 0.0));
    }

    #[test]
    fn test_tile_vertically() {
        let mut nodes = vec![
            node(1, 0.0, 0.0, 100.0, 50.0),
            node(2, 0.0, 500.0, 100.0, 30.0),
        ];
        let r = tile_vertically(&refs(&mut nodes), 5.0);
        assert_eq!(r[1], (NodeId(2), 0.0, 55.0));
    }

    #[test]
    fn test_tile_grid() {
        let mut nodes = vec![
            node(1, 0.0, 0.0, 50.0, 50.0),
            node(2, 0.0, 0.0, 60.0, 40.0),
            node(3, 0.0, 0.0, 40.0, 60.0),
        ];
        let r = tile_grid(&refs(&mut nodes), 2, 10.0);
        // cell = max size (60x60); row 1 starts at y=70
        assert_eq!(r[1], (NodeId(2), 70.0, 0.0));
        assert_eq!(r[2], (NodeId(3), 0.0, 70.0));
    }

    #[test]
    fn test_align_edges() {
        let mut nodes = vec![
            node(1, 10.0, 5.0, 100.0, 50.0),
            node(2, 200.0, 60.0, 80.0, 40.0),
        ];
        let left = align(&refs(&mut nodes), Edge::Left);
        assert_eq!(left[1].1, 10.0);
        let top = align(&refs(&mut nodes), Edge::Top);
        assert_eq!(top[1].2, 5.0);
        // Right edge: max right = 280; node 2 (w=80) -> x = 200.
        let right = align(&refs(&mut nodes), Edge::Right);
        assert_eq!(right[1].1, 200.0);
        assert_eq!(right[0].1, 180.0);
    }

    #[test]
    fn test_distribute_horizontal() {
        let mut nodes = vec![
            node(1, 0.0, 0.0, 100.0, 50.0),
            node(2, 250.0, 0.0, 50.0, 50.0),
            node(3, 600.0, 0.0, 100.0, 50.0),
        ];
        let r = distribute(&refs(&mut nodes), Axis::Horizontal);
        // span = 700 - 250 = 450; total = 250; gap = 225
        assert_eq!(r[1].1, 325.0);
        assert_eq!(r[2].1, 600.0); // end fixed
    }

    #[test]
    fn test_snap_to_grid() {
        assert_eq!(snap_to_grid(17, 20), 20);
        assert_eq!(snap_to_grid(11, 20), 20);
        assert_eq!(snap_to_grid(9, 20), 0);
        assert_eq!(snap_to_grid(0, 20), 0);
        assert_eq!(snap_to_grid(5, 0), 5); // no grid
    }

    #[test]
    fn test_snap_to_edges() {
        let mut others = vec![node(1, 100.0, 100.0, 200.0, 100.0)];
        let mut moving = node(2, 108.0, 500.0, 50.0, 50.0);
        let g = snap_to_edges(&moving, &refs(&mut others), 10.0);
        assert_eq!(g.x, 100.0); // left snapped to other's left
        assert_eq!(g.y, 500.0); // y untouched
        assert!(!g.guides.is_empty());

        moving.transform.x = 310.0;
        let g = snap_to_edges(&moving, &refs(&mut others), 15.0);
        assert_eq!(g.x, 300.0); // to other's right edge (100+200)
        assert!(!g.guides.is_empty());
    }

    #[test]
    fn test_cascade() {
        let mut nodes = vec![
            node(1, 0.0, 0.0, 100.0, 50.0),
            node(2, 0.0, 0.0, 100.0, 50.0),
        ];
        let r = cascade(&refs(&mut nodes), 24.0);
        assert_eq!(r[1], (NodeId(2), 24.0, 24.0));
    }

    #[test]
    fn test_world_screen_ndc_roundtrip() {
        let (sx, sy) = world_to_screen(60.0, 60.0, 0.0, 0.0, 1.0);
        assert_eq!((sx, sy), (60.0, 60.0));
        assert_eq!(screen_to_ndc(0.0, 0.0, 1280.0, 720.0), (-1.0, 1.0));
        assert_eq!(screen_to_ndc(1280.0, 720.0, 1280.0, 720.0), (1.0, -1.0));
        assert_eq!(screen_to_ndc(640.0, 360.0, 1280.0, 720.0), (0.0, 0.0));
    }

    #[test]
    fn test_startup_node_inside_clip_space() {
        // Terminal node top-left and bottom-right must land inside NDC.
        let (x0, y0) = world_to_ndc(24.0, 24.0, 0.0, 0.0, 1.0, 1280.0, 720.0);
        let (x1, y1) = world_to_ndc(760.0, 520.0, 0.0, 0.0, 1.0, 1280.0, 720.0);
        for v in [x0, y0, x1, y1] {
            assert!(v > -1.0 && v < 1.0, "out of clip: {v}");
        }
        assert!(x0 < x1 && y0 > y1);
    }

    #[test]
    fn test_grid_cell_origin_monotonic() {
        let a = grid_cell_origin(24.0, 24.0, 8.0, 30.0, 9.0, 18.0, 0, 0);
        let b = grid_cell_origin(24.0, 24.0, 8.0, 30.0, 9.0, 18.0, 79, 23);
        assert_eq!(a, (32.0, 54.0));
        assert!(b.0 > a.0 && b.1 > a.1);
        assert_eq!(b, (32.0 + 9.0 * 79.0, 54.0 + 18.0 * 23.0));
    }

    #[test]
    fn test_dashboard_layout_no_overlap() {
        let (t, v) = dashboard_layout(1280.0, 736.0, 472.0, 360.0, 24.0, 24.0);
        assert_eq!(t, (24.0, 24.0));
        assert!(v.0 >= t.0 + 736.0 + 24.0);
        assert_eq!(v.1, 24.0);
        // Narrow viewport wraps the view node below.
        let (t2, v2) = dashboard_layout(700.0, 736.0, 472.0, 360.0, 24.0, 24.0);
        assert_eq!(t2, (24.0, 24.0));
        assert_eq!(v2, (24.0, 24.0 + 472.0 + 24.0));
    }

    #[test]
    fn test_place_beside_side_by_side() {
        // Anchor 736 wide at x=24; new 736 wide + 24 gap fits in 1280? No:
        // 24+736+24=784, 784+736=1520 > 1280, so wraps below. Use a
        // smaller anchor to test the beside path.
        let pos = place_beside(
            (24.0, 24.0, 200.0, 100.0),
            (200.0, 100.0),
            24.0,
            (1280.0, 720.0),
        );
        assert_eq!(pos, (24.0 + 200.0 + 24.0, 24.0));
        // No overlap: right edge of anchor + gap == left of new.
        assert!(pos.0 >= 24.0 + 200.0 + 24.0);
    }

    #[test]
    fn test_ideal_grid_for_view_matches_aspect() {
        // 1280x720 view, 9x20 cells, header 40, pads 8/10, 24 rows:
        // node_h = 530, target_w = 942.2, cols = round(926.2/9) = 103.
        let (cols, rows) = ideal_grid_for_view(1280.0, 720.0, 9.0, 20.0, 40.0, 8.0, 10.0, 24);
        assert_eq!((cols, rows), (103, 24));
        // Resulting node aspect ≈ view aspect (within half a cell).
        let (nw, nh) = (cols as f64 * 9.0 + 16.0, 40.0 + 24.0 * 20.0 + 10.0);
        assert!((nw / nh - 1280.0 / 720.0).abs() < 0.02);
        // Rows clamp, degenerate view safe.
        assert_eq!(
            ideal_grid_for_view(1280.0, 720.0, 9.0, 20.0, 40.0, 8.0, 10.0, 0).1,
            2
        );
    }

    #[test]
    fn test_place_beside_wraps_on_narrow_viewport() {
        let pos = place_beside(
            (24.0, 24.0, 736.0, 472.0),
            (736.0, 472.0),
            24.0,
            (700.0, 720.0),
        );
        assert_eq!(pos, (24.0, 24.0 + 472.0 + 24.0));
    }

    #[test]
    fn test_terminal_grid_size() {
        // Exact fit: node 736x482, cell 9x18, header 38, pads 8/10.
        // cols = (736-16)/9 = 80, rows = (482-38-10)/18 = 24 (floor).
        let (cols, rows) = terminal_grid_size(736.0, 482.0, 9.0, 18.0, 38.0, 8.0, 10.0);
        assert_eq!((cols, rows), (80, 24));
        // Tiny node clamps to minimum 2x2.
        let (cols, rows) = terminal_grid_size(10.0, 10.0, 9.0, 18.0, 38.0, 8.0, 10.0);
        assert_eq!((cols, rows), (2, 2));
    }

    #[test]
    fn test_resize_handle_hit() {
        // Node 100x100 at origin, cam at origin, zoom 1: corner at (100,100).
        assert!(resize_handle_hit(
            0.0, 0.0, 100.0, 100.0, 0.0, 0.0, 1.0, 105.0, 105.0, 10.0
        ));
        assert!(!resize_handle_hit(
            0.0, 0.0, 100.0, 100.0, 0.0, 0.0, 1.0, 50.0, 50.0, 10.0
        ));
    }

    #[test]
    fn test_cascade_from_steps_diagonally() {
        let mut nodes = vec![
            node(1, 10.0, 20.0, 50.0, 50.0),
            node(2, 99.0, 99.0, 50.0, 50.0),
        ];
        let r = super::cascade_from(&refs(&mut nodes), 48.0);
        assert_eq!(r[0], (NodeId(1), 10.0, 20.0));
        assert_eq!(r[1], (NodeId(2), 58.0, 68.0));
    }

    #[test]
    fn test_orbit_spreads_ring() {
        let mut nodes = vec![
            node(1, 0.0, 0.0, 100.0, 100.0),
            node(2, 200.0, 0.0, 100.0, 100.0),
        ];
        let r = super::orbit(&refs(&mut nodes), 200.0);
        assert_eq!(r.len(), 2);
        assert!((r[0].1 - r[1].1).abs() > 100.0);
    }

    #[test]
    fn test_focus_ring_keeps_focus_first() {
        let mut nodes = vec![
            node(1, 0.0, 0.0, 100.0, 100.0),
            node(2, 300.0, 0.0, 100.0, 100.0),
        ];
        let r = super::focus_ring(&refs(&mut nodes), NodeId(2), 300.0);
        assert_eq!(r[0].0, NodeId(2));
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn test_rects_intersect_culling() {
        // Overlap and containment are visible.
        assert!(super::rects_intersect(
            0.0, 0.0, 10.0, 10.0, 5.0, 5.0, 10.0, 10.0
        ));
        assert!(super::rects_intersect(
            0.0, 0.0, 100.0, 100.0, 10.0, 10.0, 5.0, 5.0
        ));
        // Fully outside on any side is culled.
        assert!(!super::rects_intersect(
            0.0, 0.0, 10.0, 10.0, 20.0, 0.0, 10.0, 10.0
        ));
        assert!(!super::rects_intersect(
            0.0, 0.0, 10.0, 10.0, 0.0, 20.0, 10.0, 10.0
        ));
        assert!(!super::rects_intersect(
            20.0, 20.0, 10.0, 10.0, 0.0, 0.0, 10.0, 10.0
        ));
        // Edge-touching has zero visible area: culled.
        assert!(!super::rects_intersect(
            0.0, 0.0, 10.0, 10.0, 10.0, 0.0, 10.0, 10.0
        ));
    }
}
