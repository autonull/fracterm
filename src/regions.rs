use crate::terminal::TerminalGrid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRect {
    pub col: u32,
    pub row: u32,
    pub cols: u32,
    pub rows: u32,
}

impl CellRect {
    pub fn area(&self) -> u32 {
        self.cols.saturating_mul(self.rows)
    }

    pub fn contains(&self, col: u32, row: u32) -> bool {
        col >= self.col
            && row >= self.row
            && col < self.col.saturating_add(self.cols)
            && row < self.row.saturating_add(self.rows)
    }

    pub fn overlaps(&self, other: &CellRect) -> bool {
        self.col < other.col.saturating_add(other.cols)
            && other.col < self.col.saturating_add(self.cols)
            && self.row < other.row.saturating_add(other.rows)
            && other.row < self.row.saturating_add(self.rows)
    }
}

fn is_box_h(ch: char) -> bool {
    matches!(
        ch,
        '─' | '━'
            | '═'
            | '-'
            | '='
            | '+'
            | '┌'
            | '┐'
            | '└'
            | '┘'
            | '┬'
            | '┴'
            | '├'
            | '┤'
            | '┼'
    )
}

fn is_box_v(ch: char) -> bool {
    matches!(
        ch,
        '│' | '┃' | '|' | '+' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼'
    )
}

pub fn detect_zoom_regions(grid: &TerminalGrid) -> Vec<CellRect> {
    let mut out: Vec<CellRect> = Vec::new();
    let (cols, rows) = (grid.cols, grid.rows);
    if cols < 8 || rows < 4 {
        return out;
    }
    let occupied = |c: u32, r: u32| -> bool {
        grid.get(r, c)
            .map(|cell| cell.character != ' ' && cell.width != 0)
            .unwrap_or(false)
    };
    let mut bands: Vec<(u32, u32)> = Vec::new();
    let mut start: Option<u32> = None;
    for r in 0..rows {
        let mut fill = 0u32;
        for c in 0..cols {
            if occupied(c, r) {
                fill += 1;
            }
        }
        let dense = fill as f64 / cols.max(1) as f64 > 0.12;
        match (dense, start) {
            (true, None) => start = Some(r),
            (false, Some(s)) => {
                bands.push((s, r));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        bands.push((s, rows));
    }
    for (r0, r1) in bands {
        let h = r1.saturating_sub(r0);
        if h < 4 {
            continue;
        }
        let mut c0 = cols;
        let mut c1 = 0u32;
        let mut count = 0u32;
        for r in r0..r1 {
            for c in 0..cols {
                if occupied(c, r) {
                    c0 = c0.min(c);
                    c1 = c1.max(c);
                    count += 1;
                }
            }
        }
        if c1 <= c0 {
            continue;
        }
        let w = c1.saturating_sub(c0).saturating_add(1);
        if w < 20 {
            continue;
        }
        if count as f64 / (w as f64 * h as f64) < 0.08 {
            continue;
        }
        out.push(CellRect {
            col: c0.saturating_sub(1),
            row: r0.saturating_sub(1),
            cols: (w + 2).min(cols),
            rows: (h + 2).min(rows),
        });
    }
    for r in 0..rows {
        let mut hc = 0u32;
        for c in 0..cols {
            if grid
                .get(r, c)
                .map(|cell| is_box_h(cell.character))
                .unwrap_or(false)
            {
                hc += 1;
            }
        }
        if hc < 8 {
            continue;
        }
        let mut left: Option<u32> = None;
        let mut right: Option<u32> = None;
        for c in 0..cols {
            if grid
                .get(r, c)
                .map(|cell| is_box_h(cell.character))
                .unwrap_or(false)
            {
                if left.is_none() {
                    left = Some(c);
                }
                right = Some(c);
            }
        }
        let (l, rr) = match (left, right) {
            (Some(a), Some(b)) if b.saturating_sub(a) >= 8 => (a, b),
            _ => continue,
        };
        for end in (r + 4)..rows.min(r + 40) {
            let mut ehc = 0u32;
            for c in l..=rr {
                if grid
                    .get(end, c)
                    .map(|cell| is_box_h(cell.character))
                    .unwrap_or(false)
                {
                    ehc += 1;
                }
            }
            if ehc < (rr.saturating_sub(l)).min(8) {
                continue;
            }
            let mut walls = 0u32;
            let span = end.saturating_sub(r).saturating_sub(1).max(1);
            for mid in (r + 1)..end {
                let a = grid
                    .get(mid, l)
                    .map(|cell| is_box_v(cell.character))
                    .unwrap_or(false);
                let b = grid
                    .get(mid, rr)
                    .map(|cell| is_box_v(cell.character))
                    .unwrap_or(false);
                if a || b {
                    walls += 1;
                }
            }
            if walls as f64 / span as f64 > 0.4 {
                out.push(CellRect {
                    col: l.min(cols.saturating_sub(1)),
                    row: r.min(rows.saturating_sub(1)),
                    cols: (rr.saturating_sub(l).saturating_add(1)).min(cols),
                    rows: (end.saturating_sub(r).saturating_add(1)).min(rows),
                });
                break;
            }
        }
    }
    let mut merged: Vec<CellRect> = Vec::new();
    for cand in out {
        let mut absorbed = false;
        for m in merged.iter_mut() {
            if m.overlaps(&cand) {
                let c0 = m.col.min(cand.col);
                let r0 = m.row.min(cand.row);
                let c1 = (m.col + m.cols).max(cand.col + cand.cols);
                let r1 = (m.row + m.rows).max(cand.row + cand.rows);
                m.col = c0;
                m.row = r0;
                m.cols = (c1 - c0).min(cols);
                m.rows = (r1 - r0).min(rows);
                absorbed = true;
                break;
            }
        }
        if !absorbed {
            merged.push(cand);
        }
    }
    merged.sort_by_key(|r| std::cmp::Reverse(r.area()));
    merged.truncate(8);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{TerminalCell, TerminalGrid};

    fn blank(rows: u32, cols: u32) -> TerminalGrid {
        TerminalGrid::new(rows, cols)
    }

    fn fill_block(grid: &mut TerminalGrid, c0: u32, r0: u32, c1: u32, r1: u32) {
        for r in r0..=r1 {
            for c in c0..=c1 {
                grid.set(
                    r,
                    c,
                    TerminalCell {
                        character: 'x',
                        ..TerminalCell::default()
                    },
                );
            }
        }
    }

    #[test]
    fn test_empty_grid_has_no_regions() {
        let g = blank(24, 80);
        assert!(detect_zoom_regions(&g).is_empty());
    }

    #[test]
    fn test_dense_block_becomes_region() {
        let mut g = blank(24, 80);
        fill_block(&mut g, 10, 5, 50, 14);
        let out = detect_zoom_regions(&g);
        assert_eq!(out.len(), 1);
        assert!(out[0].contains(30, 10));
    }

    #[test]
    fn test_two_separated_blocks_become_two() {
        let mut g = blank(30, 80);
        fill_block(&mut g, 5, 2, 45, 8);
        fill_block(&mut g, 5, 18, 45, 26);
        let out = detect_zoom_regions(&g);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_box_frame_becomes_region() {
        let mut g = blank(24, 80);
        let top = "┌──────────────────────────────────────┐";
        for (i, ch) in top.chars().enumerate() {
            g.set(
                3,
                5 + i as u32,
                TerminalCell {
                    character: ch,
                    ..TerminalCell::default()
                },
            );
        }
        for r in 4..10 {
            for (c, ch) in [(5, '│'), (44, '│')] {
                g.set(
                    r,
                    c,
                    TerminalCell {
                        character: ch,
                        ..TerminalCell::default()
                    },
                );
            }
        }
        let bot = "└──────────────────────────────────────┘";
        for (i, ch) in bot.chars().enumerate() {
            g.set(
                10,
                5 + i as u32,
                TerminalCell {
                    character: ch,
                    ..TerminalCell::default()
                },
            );
        }
        let out = detect_zoom_regions(&g);
        assert!(!out.is_empty());
        assert!(out.iter().any(|r| r.contains(20, 6)));
    }

    #[test]
    fn test_tiny_grid_returns_none() {
        let g = blank(2, 4);
        assert!(detect_zoom_regions(&g).is_empty());
    }
}
