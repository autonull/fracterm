//! Dashboard arrangement commands (README M8 / §11): tiling, alignment,
//! distribution, and snapping. Pure functions over node transforms/sizes.

use crate::node::Node;
use crate::NodeId;

/// Layout gap in world pixels between tiled nodes.
pub const DEFAULT_GAP: f64 = 16.0;

fn sizes(nodes: &[&Node]) -> Vec<(f64, f64)> {
    nodes
        .iter()
        .map(|n| (n.size.0.max(1) as f64, n.size.1.max(1) as f64))
        .collect()
}

/// Tile nodes left-to-right in one row, preserving vertical order of
/// the first node. Returns (node_id, x, y) placement list.
pub fn tile_horizontally(nodes: &[&Node], gap: f64) -> Vec<(NodeId, i32, i32)> {
    let sizes = sizes(nodes);
    let mut out = Vec::with_capacity(nodes.len());
    let mut x = nodes.first().map(|n| n.transform.x).unwrap_or(0);
    let y = nodes.first().map(|n| n.transform.y).unwrap_or(0);
    for (node, (w, _)) in nodes.iter().zip(sizes) {
        out.push((node.id, x, y));
        x += w as i32 + gap as i32;
    }
    out
}

/// Tile nodes top-to-bottom in one column.
pub fn tile_vertically(nodes: &[&Node], gap: f64) -> Vec<(NodeId, i32, i32)> {
    let sizes = sizes(nodes);
    let mut out = Vec::with_capacity(nodes.len());
    let x = nodes.first().map(|n| n.transform.x).unwrap_or(0);
    let mut y = nodes.first().map(|n| n.transform.y).unwrap_or(0);
    for (node, (_, h)) in nodes.iter().zip(sizes) {
        out.push((node.id, x, y));
        y += h as i32 + gap as i32;
    }
    out
}

/// Tile into a grid with `cols` columns. Cell size is the max of all node
/// sizes; every node fills its cell.
pub fn tile_grid(nodes: &[&Node], cols: usize, gap: f64) -> Vec<(NodeId, i32, i32)> {
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
            origin.0 + (col as f64 * (cell_w + gap)) as i32,
            origin.1 + (row as f64 * (cell_h + gap)) as i32,
        ));
    }
    out
}

/// Cascade: offset each node diagonally by a fixed step.
pub fn cascade(nodes: &[&Node], step: i32) -> Vec<(NodeId, i32, i32)> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                n.id,
                n.transform.x + (i as i32) * step,
                n.transform.y + (i as i32) * step,
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
}

/// Align all nodes to the extreme edge of the current bounding box.
pub fn align(nodes: &[&Node], edge: Edge) -> Vec<(NodeId, i32, i32)> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let sizes = sizes(nodes);
    let max_right = nodes
        .iter()
        .zip(&sizes)
        .map(|(n, (w, _))| n.transform.x + *w as i32)
        .max()
        .unwrap_or(0);
    let max_bottom = nodes
        .iter()
        .zip(&sizes)
        .map(|(n, (_, h))| n.transform.y + *h as i32)
        .max()
        .unwrap_or(0);
    let min_left = nodes.iter().map(|n| n.transform.x).min().unwrap_or(0);
    let min_top = nodes.iter().map(|n| n.transform.y).min().unwrap_or(0);
    nodes
        .iter()
        .zip(&sizes)
        .map(|(n, (w, h))| {
            let (x, y) = match edge {
                Edge::Left => (min_left, n.transform.y),
                Edge::Right => (max_right - *w as i32, n.transform.y),
                Edge::Top => (n.transform.x, min_top),
                Edge::Bottom => (n.transform.x, max_bottom - *h as i32),
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
pub fn distribute(nodes: &[&Node], axis: Axis) -> Vec<(NodeId, i32, i32)> {
    if nodes.len() < 3 {
        return Vec::new();
    }
    let sizes = sizes(nodes);
    let last = nodes.len() - 1;
    let (start, end, total_size): (i32, i32, i32) = match axis {
        Axis::Horizontal => (
            nodes[0].transform.x,
            nodes[last].transform.x + sizes[last].0 as i32,
            sizes.iter().map(|(w, _)| *w as i32).sum(),
        ),
        Axis::Vertical => (
            nodes[0].transform.y,
            nodes[last].transform.y + sizes[last].1 as i32,
            sizes.iter().map(|(_, h)| *h as i32).sum(),
        ),
    };
    let span = (end - start - total_size).max(0);
    let gap = span / (nodes.len() - 1) as i32;
    let mut out = Vec::with_capacity(nodes.len());
    let mut cursor = start;
    for (node, (w, h)) in nodes.iter().zip(sizes) {
        let size = match axis {
            Axis::Horizontal => w,
            Axis::Vertical => h,
        } as i32;
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
) -> ((i32, i32), (i32, i32)) {
    let tx = margin;
    let ty = margin;
    let mut vx = tx + term_w + gap;
    let mut vy = margin;
    if vx + view_node_w > view_w - margin {
        vx = tx;
        vy = ty + term_h + gap;
    }
    ((tx as i32, ty as i32), (vx as i32, vy as i32))
}

/// A suggested snap: the node edge aligns with a nearby other edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapGuide {
    pub node_id: NodeId,
    /// Snapped x/y position.
    pub x: i32,
    pub y: i32,
    /// Distance from the original position, for guide rendering.
    pub distance: f64,
}

/// Snap a moving node's rect to other nodes' edges within `threshold`.
pub fn snap_to_edges(moving: &Node, others: &[&Node], threshold: i32) -> SnapGuide {
    let (mw, mh) = (moving.size.0, moving.size.1);
    let (mx, my) = (moving.transform.x, moving.transform.y);
    let mut best: Option<SnapGuide> = None;
    for other in others {
        if other.id == moving.id {
            continue;
        }
        let (ox, oy) = (other.transform.x, other.transform.y);
        let (ow, oh) = other.size;
        let candidates = [
            (mx, oy),      // top edge to other's top
            (mx, oy + oh), // top edge to other's bottom
            (mx, oy - mh), // bottom edge to other's top
            (ox, my),      // left edge to other's left
            (ox + ow, my), // left edge to other's right
            (ox - mw, my), // right edge to other's left
        ];
        for (cx, cy) in candidates {
            let dx = (cx - mx).abs();
            let dy = (cy - my).abs();
            let d = dx.max(dy);
            if d > threshold {
                continue;
            }
            if best
                .as_ref()
                .map(|b| (d as f64) < b.distance)
                .unwrap_or(true)
            {
                best = Some(SnapGuide {
                    node_id: moving.id,
                    x: cx,
                    y: cy,
                    distance: d as f64,
                });
            }
        }
    }
    best.unwrap_or(SnapGuide {
        node_id: moving.id,
        x: mx,
        y: my,
        distance: f64::INFINITY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurfaceId;

    fn node(id: u64, x: i32, y: i32, w: i32, h: i32) -> Node {
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
        let mut nodes = vec![node(1, 0, 0, 100, 50), node(2, 500, 0, 80, 50)];
        let r = tile_horizontally(&refs(&mut nodes), 10.0);
        assert_eq!(r[0], (NodeId(1), 0, 0));
        assert_eq!(r[1], (NodeId(2), 110, 0));
    }

    #[test]
    fn test_tile_vertically() {
        let mut nodes = vec![node(1, 0, 0, 100, 50), node(2, 0, 500, 100, 30)];
        let r = tile_vertically(&refs(&mut nodes), 5.0);
        assert_eq!(r[1], (NodeId(2), 0, 55));
    }

    #[test]
    fn test_tile_grid() {
        let mut nodes = vec![
            node(1, 0, 0, 50, 50),
            node(2, 0, 0, 60, 40),
            node(3, 0, 0, 40, 60),
        ];
        let r = tile_grid(&refs(&mut nodes), 2, 10.0);
        // cell = max size (60x60); row 1 starts at y=70
        assert_eq!(r[1], (NodeId(2), 70, 0));
        assert_eq!(r[2], (NodeId(3), 0, 70));
    }

    #[test]
    fn test_align_edges() {
        let mut nodes = vec![node(1, 10, 5, 100, 50), node(2, 200, 60, 80, 40)];
        let left = align(&refs(&mut nodes), Edge::Left);
        assert_eq!(left[1].1, 10);
        let top = align(&refs(&mut nodes), Edge::Top);
        assert_eq!(top[1].2, 5);
        // Right edge: max right = 280; node 2 (w=80) -> x = 200.
        let right = align(&refs(&mut nodes), Edge::Right);
        assert_eq!(right[1].1, 200);
        assert_eq!(right[0].1, 280 - 100);
    }

    #[test]
    fn test_distribute_horizontal() {
        let mut nodes = vec![
            node(1, 0, 0, 100, 50),
            node(2, 250, 0, 50, 50),
            node(3, 600, 0, 100, 50),
        ];
        let r = distribute(&refs(&mut nodes), Axis::Horizontal);
        // span = 700 - 250 = 450; total = 250; gap = 225
        assert_eq!(r[1].1, 325);
        assert_eq!(r[2].1, 600); // end fixed
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
        let mut others = vec![node(1, 100, 100, 200, 100)];
        let mut moving = node(2, 108, 500, 50, 50);
        let g = snap_to_edges(&moving, &refs(&mut others), 10);
        assert_eq!(g.x, 100); // left snapped to other's left
        assert_eq!(g.y, 500); // y untouched

        moving.transform.x = 310;
        let g = snap_to_edges(&moving, &refs(&mut others), 15);
        assert_eq!(g.x, 300); // to other's right edge (100+200)
    }

    #[test]
    fn test_cascade() {
        let mut nodes = vec![node(1, 0, 0, 100, 50), node(2, 0, 0, 100, 50)];
        let r = cascade(&refs(&mut nodes), 24);
        assert_eq!(r[1], (NodeId(2), 24, 24));
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
        assert_eq!(t, (24, 24));
        assert!(v.0 as f64 >= t.0 as f64 + 736.0 + 24.0);
        assert_eq!(v.1, 24);
        // Narrow viewport wraps the view node below.
        let (t2, v2) = dashboard_layout(700.0, 736.0, 472.0, 360.0, 24.0, 24.0);
        assert_eq!(t2, (24, 24));
        assert_eq!(v2, (24, 24 + 472 + 24));
    }
}
