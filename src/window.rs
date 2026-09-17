//! Windowed canvas runtime: winit event loop + glutin OpenGL context,
//! camera pan/zoom input, resize and HiDPI handling (README M1).

use std::collections::HashMap;
use std::num::NonZeroU32;

type CellPos = (u32, u32);
type TextSelection = (SurfaceId, crate::NodeId, CellPos, CellPos);
type SelectDrag = (SurfaceId, crate::NodeId, CellPos);
type MenuItems = Vec<(String, String)>;
type MenuSnap = Option<(f64, f64, MenuItems, usize)>;
type ScrollBar = (f64, f64, f64, f64, f64, f64, f64, f64);

struct MenuState {
    x: f64,
    y: f64,
    items: MenuItems,
    index: usize,
}

use glutin::context::{ContextApi, PossiblyCurrentContext};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{GlSurface, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle as _;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::app::App;
use crate::canvas::{Capabilities, PassTarget, RectRenderer, RenderGraphExecutor};
use crate::pty::PtySession;
use crate::terminal::Terminal;
use crate::text::{Atlas, FontSystem, TextRenderer};
use crate::vt::VtEngine;
use crate::SurfaceId;

/// Grid + dashboard constants: the startup terminal is an 80x24 grid
/// rasterized at GRID_PX; cell metrics are measured from the loaded face.
const GRID_COLS: u32 = 80;
const GRID_ROWS: u32 = 24;
const GRID_PX: u32 = 15;
const GRID_PAD_X: f64 = 8.0;
const GRID_PAD_BOTTOM: f64 = 10.0;
const DASH_MARGIN: f64 = 24.0;
const DASH_GAP: f64 = 24.0;

/// One live terminal: VT engine + PTY bound to a workspace node.
struct TermSession {
    surface: SurfaceId,
    node: crate::NodeId,
    engine: VtEngine,
    pty: PtySession,
}

/// Drag state machine: empty-canvas pans the camera, a hit node moves,
/// and a grab on the selected node's corner resizes it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DragState {
    None,
    Pan {
        ax: f64,
        ay: f64,
    },
    Move {
        node: crate::NodeId,
        dx: f64,
        dy: f64,
    },
    Resize {
        node: crate::NodeId,
    },
}

struct CanvasState {
    app: App,
    gl_display: Option<glutin::display::Display>,
    gl_context: Option<PossiblyCurrentContext>,
    gl_surface: Option<glutin::surface::Surface<WindowSurface>>,
    gl: Option<glow::Context>,
    renderer: Option<RectRenderer>,
    graph: Option<RenderGraphExecutor>,
    fonts: Option<FontSystem>,
    font_id: Option<u32>,
    atlas: Option<Atlas>,
    text: Option<TextRenderer>,
    /// Live terminal sessions: one VT engine + PTY per terminal node.
    terms: Vec<TermSession>,
    /// Surface id of the focused terminal (keyboard target).
    focused_term: Option<SurfaceId>,
    /// Whether the focused terminal has keyboard focus.
    term_focus: bool,
    /// Measured grid cell (cell_w, line_h) at GRID_PX.
    grid_cell: (f64, f64),
    next_surface_id: u64,
    next_node_id: u64,
    /// Currently held keyboard modifiers.
    modifiers: winit::event::Modifiers,
    /// Diagnostics: frames drawn, PTY bytes drained.
    diag_frames: u64,
    diag_pty_bytes: u64,
    /// Right-drag rectangle-zoom anchor (screen px).
    rect_anchor: Option<(f64, f64)>,
    /// Last window title applied from the focused session's OSC title.
    last_title: String,
    /// Blink clock for steady-vs-blinking cursor styles.
    blink_epoch: std::time::Instant,
    /// Last frame instant, for camera easing dt.
    last_frame: std::time::Instant,
    capabilities: Option<Capabilities>,
    window: Option<Window>,
    /// Node selected as a whole (accent border + resize handle).
    selected: Option<crate::NodeId>,
    /// Current drag interaction (pan / move / resize).
    drag: DragState,
    last_cursor: (f64, f64),
    /// Current physical viewport size
    viewport_size: (f32, f32),
    /// Terminal child currently owning the mouse (xterm 1000+ active and
    /// the press had no Shift): motion/release go to its PTY, not to
    /// canvas drags. Cleared on left release.
    forwarding: Option<ForwardMouse>,
    middle_down: Option<(f64, f64)>,
    selecting: Option<SelectDrag>,
    selection: Option<TextSelection>,
    font_scales: HashMap<crate::NodeId, f32>,
    menu: Option<MenuState>,
    palette_open: bool,
    palette_query: String,
    palette_index: usize,
    /// Space held: left-drag pans even over a node. Full-bleed startup
    /// leaves no empty canvas, so middle-drag was the only pan — space-pan
    /// restores a one-handed pan anywhere. Only honored when no terminal
    /// has keyboard focus (otherwise space types into the PTY).
    space_down: bool,
    /// Auto-zoom region cache: `detect_zoom_regions` scans the whole grid,
    /// so it only re-runs when the selected terminal's content changed
    /// (`dirty`) or the node/metrics moved. Key is (node, x, y, cw, lh).
    region_cache_node: Option<crate::NodeId>,
    region_cache_key: (u64, u64, u64, u64),
    region_cache: Vec<(f64, f64, f64, f64)>,
}

/// A terminal child that owns the mouse until left-button release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForwardMouse {
    surface: SurfaceId,
    button: crate::vt::MouseButton,
}

/// Fuzzy subsequence score for palette filtering (`None` = no match).
/// Both inputs must already be lowercased. Matches earn a base value
/// plus bonuses for word-boundary hits (`tile` in `arrange.tile` after
/// `.`) and consecutive runs, so `pinl` ranks `view.pinLive` above a
/// scattered cross-word match.
pub fn fuzzy_score(query: &str, target: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let mut score = 0u32;
    let mut qi = query.chars();
    let mut need = qi.next()?;
    let mut prev_match = false;
    let mut prev_boundary = true;
    for ch in target.chars() {
        if ch == need {
            score += 10;
            if prev_boundary {
                score += 15;
            }
            if prev_match {
                score += 5;
            }
            prev_match = true;
            prev_boundary = false;
            match qi.next() {
                Some(c) => need = c,
                None => return Some(score),
            }
        } else {
            prev_match = false;
            prev_boundary = matches!(ch, '.' | '-' | '_' | '/' | ' ' | ':');
        }
    }
    None
}

impl CanvasState {
    fn new(app: App) -> Self {
        Self {
            app,
            gl_display: None,
            gl_context: None,
            gl_surface: None,
            gl: None,
            renderer: None,
            graph: None,
            fonts: None,
            font_id: None,
            atlas: None,
            text: None,
            terms: Vec::new(),
            focused_term: None,
            term_focus: true,
            grid_cell: (9.0, 18.0),
            next_surface_id: SurfaceId(1).0,
            next_node_id: 1,
            modifiers: winit::event::Modifiers::default(),
            diag_frames: 0,
            diag_pty_bytes: 0,
            rect_anchor: None,
            last_title: String::from("fracterm"),
            blink_epoch: std::time::Instant::now(),
            last_frame: std::time::Instant::now(),
            capabilities: None,
            window: None,
            selected: None,
            drag: DragState::None,
            last_cursor: (0.0, 0.0),
            viewport_size: (1280.0, 720.0),
            forwarding: None,
            middle_down: None,
            selecting: None,
            selection: None,
            font_scales: HashMap::new(),
            menu: None,
            palette_open: false,
            palette_query: String::new(),
            palette_index: 0,
            space_down: false,
            region_cache_node: None,
            region_cache_key: (0, 0, 0, 0),
            region_cache: Vec::new(),
        }
    }

    fn zoom_at_cursor(&mut self, delta: f64) {
        let cam = self.app.state.workspace.camera_mut();
        let old = cam.target_zoom;
        let new = (old * delta).clamp(0.05, 64.0);
        if (new - old).abs() < f64::EPSILON {
            return;
        }
        // Keep the world point under the cursor stationary at the target,
        // so repeated wheels ease toward one anchor instead of fighting it.
        let (cx, cy) = self.last_cursor;
        cam.target_x += cx / old - cx / new;
        cam.target_y += cy / old - cy / new;
        cam.target_zoom = new;
        cam.animating = true;
    }

    /// Handle right-click release: rectangle zoom if dragged, autozoom to
    /// object if clicked, context menu with Ctrl held (README §6.2/§23).
    fn finish_right_click(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(anchor) = self.rect_anchor.take() else {
            return;
        };
        let (sx, sy) = self.last_cursor;
        let drag = ((sx - anchor.0).abs() + (sy - anchor.1).abs()) > 12.0;
        let cam = self.app.state.workspace.camera().clone();
        let ctrl = self.modifiers.state().control_key();

        if drag {
            // Rectangle zoom: screen rect -> world rect -> fit.
            let x0 = cam.x + anchor.0.min(sx) / cam.zoom;
            let y0 = cam.y + anchor.1.min(sy) / cam.zoom;
            let x1 = cam.x + anchor.0.max(sx) / cam.zoom;
            let y1 = cam.y + anchor.1.max(sy) / cam.zoom;
            let rect = crate::lens::Rect {
                x: x0,
                y: y0,
                width: (x1 - x0).max(1.0),
                height: (y1 - y0).max(1.0),
            };
            let (vw, vh) = self.viewport_size;
            self.app
                .state
                .workspace
                .camera_mut()
                .fit_rect(rect, vw as f64, vh as f64);
        } else if ctrl {
            let (wx, wy) = (cam.x + sx / cam.zoom, cam.y + sy / cam.zoom);
            let node = self.hit_node(wx, wy).map(|n| n.id);
            self.open_menu(wx, wy, node);
        } else {
            let (wx, wy) = (cam.x + sx / cam.zoom, cam.y + sy / cam.zoom);
            let hit = self.hit_node(wx, wy).map(|n| (n.id, n.transform, n.size));
            if let Some((nid, t, size)) = hit {
                let (vw, vh) = self.viewport_size;
                if let Some(rect) = self.region_zoom_rect(nid, sx, sy) {
                    self.app
                        .state
                        .workspace
                        .camera_mut()
                        .fit_rect(rect, vw as f64, vh as f64);
                } else {
                    self.app
                        .state
                        .workspace
                        .camera_mut()
                        .zoom_to_node(nid, t, size, vw as f64, vh as f64);
                }
            }
        }
    }

    /// Topmost node whose bounds contain the world point.
    fn hit_node(&self, wx: f64, wy: f64) -> Option<&crate::Node> {
        let ids: Vec<_> = self.app.state.workspace.nodes().to_vec();
        for id in ids.iter().rev() {
            if let Some(node) = self.app.state.workspace.get_node(*id) {
                let (w, h) = node.size;
                let w = w.max(1.0);
                let h = h.max(1.0);
                let (nx, ny) = (node.transform.x, node.transform.y);
                if wx >= nx && wx <= nx + w && wy >= ny && wy <= ny + h {
                    return Some(node);
                }
            }
        }
        None
    }

    /// Write bytes to one session's PTY (mouse reports, query replies).
    fn write_to_surface(&mut self, surface: SurfaceId, bytes: &[u8]) {
        if let Some(sess) = self.terms.iter().find(|s| s.surface == surface) {
            let _ = sess.pty.write(bytes);
        }
    }

    /// Paste text into one session, framed for bracketed paste (?2004)
    /// when the child asked for it.
    fn paste_text_into(&mut self, surface: SurfaceId, text: &str) {
        let bytes = self
            .terms
            .iter()
            .find(|s| s.surface == surface)
            .map(|s| s.engine.bracket_paste(text));
        if let Some(bytes) = bytes {
            self.write_to_surface(surface, &bytes);
        }
    }

    /// Paste clipboard (`primary == false`) or X11 primary selection text
    /// into one session. Clipboard failures (headless, empty) are no-ops.
    fn paste_clipboard_into(&mut self, surface: SurfaceId, primary: bool) {
        if let Some(text) = read_clipboard(primary) {
            self.paste_text_into(surface, &text);
        }
    }

    /// Screen px -> clamped 0-based terminal cell for session `idx`.
    /// Returns `None` only when the session or its node is gone.
    fn mouse_cell_for(&self, idx: usize, sx: f64, sy: f64) -> Option<(u32, u32)> {
        let sess = self.terms.get(idx)?;
        let node = self.app.state.workspace.get_node(sess.node)?;
        let cam = self.app.state.workspace.camera();
        Some(screen_to_cell(
            sx,
            sy,
            cam.x,
            cam.y,
            cam.zoom,
            node.transform.x,
            node.transform.y,
            GRID_PAD_X,
            self.header_h(),
            self.grid_cell.0,
            self.grid_cell.1,
            sess.engine.term.grid.cols,
            sess.engine.term.grid.rows,
        ))
    }

    /// Apply an arrange function's placements to the workspace nodes.
    fn apply_arrange(
        &mut self,
        f: impl Fn(&[&crate::Node], usize) -> Vec<(crate::NodeId, f64, f64)>,
        arg: usize,
    ) {
        let owned: Vec<crate::Node> = self
            .app
            .state
            .workspace
            .all_nodes()
            .iter()
            .map(|n| (*n).clone())
            .collect();
        let refs: Vec<&crate::Node> = owned.iter().collect();
        let placements = f(&refs, arg);
        for (id, x, y) in placements {
            if let Some(node) = self.app.state.workspace.get_node_mut(id) {
                node.transform.x = x;
                node.transform.y = y;
            }
        }
    }

    /// Pin the focused terminal as a projection node beside it ("Pin as View").
    /// Snapshot freezes what you saw; live re-syncs from the source every
    /// frame (follow tail) via the existing projection sync.
    fn pin_view(&mut self, live: bool) {
        let source = match self.focused_term {
            Some(s) => s,
            None => return,
        };
        let anchor = self
            .terms
            .iter()
            .find(|t| t.surface == source)
            .map(|t| t.node)
            .and_then(|id| {
                self.app
                    .state
                    .workspace
                    .get_node(id)
                    .map(|n| (n.transform.x, n.transform.y, n.size.0, n.size.1))
            });
        let Some((ax, ay, aw, _ah)) = anchor else {
            return;
        };
        let next_id = crate::NodeId(
            self.app
                .state
                .workspace
                .node_ids()
                .iter()
                .map(|n| n.0)
                .max()
                .unwrap_or(0)
                + 1,
        );
        let selector = crate::ProjectionSelector {
            rows: None,
            columns: None,
            filter: None,
            search: None,
            max_lines: None,
            follow: live,
        };
        let term = match self.terms.iter().find(|t| t.surface == source) {
            Some(t) => &t.engine.term,
            None => return,
        };
        let mode = if live {
            crate::ProjectionMode::Live
        } else {
            crate::ProjectionMode::Snapshot
        };
        let proj = crate::ProjectionSurface::from_terminal(term, selector, mode);
        let (pw, ph) = (aw.max(200.0), 320.0);
        let mut node = crate::Node::new(next_id, ax + aw + 40.0, ay);
        node.size = (pw, ph);
        node.set_surface(source);
        node.projection = Some(proj);
        node.transform.scale = 1.0;
        self.app.state.workspace.add_node(node);
        self.selected = Some(next_id);
        let kind = if live { "live view" } else { "snapshot" };
        eprintln!("pinned {kind} as node {next_id:?}");
    }

    /// Palette rows, sourced from the builtin command catalog so palette,
    /// menus, keys, and help all resolve the same ids. The digit-restore
    /// helper (`camera.bookmarkRestore`) is key-only and stays out.
    ///
    /// Rows are ranked by [`fuzzy_score`] so short queries (`tv`, `pinl`)
    /// find their command without exact-substring typing.
    fn palette_commands() -> Vec<(&'static str, &'static str)> {
        crate::command::builtin_commands()
            .into_iter()
            .filter(|c| c.id != "camera.bookmarkRestore")
            .map(|c| (c.id, c.title))
            .collect()
    }

    fn palette_filtered(&self) -> Vec<(&'static str, &'static str)> {
        let q = self.palette_query.to_lowercase();
        if q.is_empty() {
            return Self::palette_commands();
        }
        let mut scored: Vec<(u32, &'static str, &'static str)> = Self::palette_commands()
            .into_iter()
            .filter_map(|(id, title)| {
                let a = fuzzy_score(&q, &id.to_lowercase());
                let b = fuzzy_score(&q, &title.to_lowercase());
                match (a, b) {
                    (None, None) => None,
                    (x, y) => Some((x.unwrap_or(0).max(y.unwrap_or(0)), id, title)),
                }
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        scored
            .into_iter()
            .map(|(_, id, title)| (id, title))
            .collect()
    }

    fn run_palette_command(&mut self, id: &str) {
        match id {
            "terminal.new" => {
                if self.terms.len() < 8 {
                    let size = self.terminal_node_size(GRID_COLS, GRID_ROWS);
                    let view = (self.viewport_size.0 as f64, self.viewport_size.1 as f64);
                    let anchor = self
                        .focused_term
                        .and_then(|s| self.terms.iter().find(|t| t.surface == s))
                        .map(|s| s.node)
                        .or_else(|| self.terms.last().map(|s| s.node))
                        .and_then(|nid| {
                            self.app
                                .state
                                .workspace
                                .get_node(nid)
                                .map(|n| (n.transform.x, n.transform.y, n.size.0, n.size.1))
                        })
                        .unwrap_or((DASH_MARGIN, DASH_MARGIN, size.0, size.1));
                    let pos = crate::arrange::place_beside(anchor, size, DASH_GAP, view);
                    if let Err(e) = self.spawn_terminal_node(GRID_COLS, GRID_ROWS, pos) {
                        eprintln!("failed to spawn terminal: {e}");
                    }
                } else {
                    eprintln!("terminal limit reached");
                }
            }
            "layout.tileH" => self.apply_arrange(
                |ns, _| crate::arrange::tile_horizontally(ns, crate::arrange::DEFAULT_GAP),
                0,
            ),
            "layout.tileV" => self.apply_arrange(
                |ns, _| crate::arrange::tile_vertically(ns, crate::arrange::DEFAULT_GAP),
                0,
            ),
            "layout.tileGrid" => self.apply_arrange(
                |ns, _| crate::arrange::tile_grid(ns, 3, crate::arrange::DEFAULT_GAP),
                0,
            ),
            "camera.fit" => self.fit_dashboard(false),
            "camera.workspaceFit" => self
                .app
                .state
                .workspace
                .camera_mut()
                .zoom_to_workspace_fit(),
            "view.pin" => self.pin_view(false),
            "view.pinLive" => self.pin_view(true),
            "camera.bookmarkSave" => {
                let name = self.app.state.workspace.camera_mut().save_next_bookmark();
                eprintln!("bookmark {name} saved");
            }
            "layout.dashboard" => {
                if let Some(first) = self.app.state.workspace.all_nodes().first().map(|n| n.id) {
                    if let Some(n) = self.app.state.workspace.get_node_mut(first) {
                        n.transform.x = DASH_MARGIN;
                        n.transform.y = DASH_MARGIN;
                    }
                }
                self.apply_arrange(
                    |ns, _| crate::arrange::tile_grid(ns, 2, crate::arrange::DEFAULT_GAP),
                    0,
                );
                self.fit_dashboard(false);
            }
            "layout.save" => self.save_layout(),
            "layout.restore" => self.restore_layout(),
            "view.reduceMotion" => {
                let on = !self.app.state.config.accessibility.reduce_motion;
                self.app.state.config.accessibility.reduce_motion = on;
                eprintln!("reduce motion {}", if on { "on" } else { "off" });
            }
            "terminal.close" => {
                self.close_selected();
            }
            "terminal.copy" => self.copy_selection_to_clipboard(false),
            "terminal.varied" => self.spawn_varied_terminal(),
            "style.fontBigger" => self.adjust_selected_font(0.15),
            "style.fontSmaller" => self.adjust_selected_font(-0.15),
            "style.opacity" => self.cycle_selected_opacity(),
            "style.tint" => self.cycle_selected_tint(),
            "layout.cascade" => {
                self.apply_arrange(|ns, _| crate::arrange::cascade_from(ns, 48.0), 0)
            }
            "layout.alignLeft" => self.apply_arrange(
                |ns, _| crate::arrange::align(ns, crate::arrange::Edge::Left),
                0,
            ),
            "layout.orbit" => self.apply_arrange(|ns, _| crate::arrange::orbit(ns, 420.0), 0),
            "layout.focus" => {
                if let Some(id) = self.selected {
                    let owned: Vec<crate::Node> = self
                        .app
                        .state
                        .workspace
                        .all_nodes()
                        .iter()
                        .map(|n| (*n).clone())
                        .collect();
                    let refs: Vec<&crate::Node> = owned.iter().collect();
                    for (nid, x, y) in crate::arrange::focus_ring(&refs, id, 460.0) {
                        if let Some(n) = self.app.state.workspace.get_node_mut(nid) {
                            n.transform.x = x;
                            n.transform.y = y;
                        }
                    }
                }
            }
            "help.open" => {
                let mut parts: Vec<String> = crate::command::builtin_commands()
                    .into_iter()
                    .map(|c| {
                        if c.key.is_empty() {
                            format!("{} ({})", c.title, c.id)
                        } else {
                            format!("{}: {}", c.key, c.title)
                        }
                    })
                    .collect();
                parts.push("Ctrl+K / :: palette".to_string());
                eprintln!("keys: {}", parts.join(", "));
            }
            _ => {}
        }
    }

    fn spawn_varied_terminal(&mut self) {
        if self.terms.len() >= 8 {
            return;
        }
        const PRESETS: [(u32, u32); 4] = [(80, 24), (100, 32), (60, 16), (120, 28)];
        let (cols, rows) = PRESETS[self.terms.len() % PRESETS.len()];
        let size = self.terminal_node_size(cols, rows);
        let view = (self.viewport_size.0 as f64, self.viewport_size.1 as f64);
        let anchor = self
            .focused_term
            .and_then(|s| self.terms.iter().find(|t| t.surface == s))
            .map(|s| s.node)
            .or_else(|| self.terms.last().map(|s| s.node))
            .and_then(|nid| {
                self.app
                    .state
                    .workspace
                    .get_node(nid)
                    .map(|n| (n.transform.x, n.transform.y, n.size.0, n.size.1))
            })
            .unwrap_or((DASH_MARGIN, DASH_MARGIN, size.0, size.1));
        let pos = crate::arrange::place_beside(anchor, size, DASH_GAP, view);
        if self.spawn_terminal_node(cols, rows, pos).is_ok() {
            if let Some(sess) = self.terms.last() {
                let nid = sess.node;
                let scale = match PRESETS[(self.terms.len() - 1) % PRESETS.len()] {
                    (60, _) => 0.8,
                    (100, _) => 1.0,
                    (120, _) => 1.25,
                    _ => 1.0,
                };
                self.font_scales.insert(nid, scale);
            }
        }
    }

    fn adjust_selected_font(&mut self, delta: f32) {
        if let Some(id) = self.selected {
            let cur = self.font_scales.get(&id).copied().unwrap_or(1.0);
            self.font_scales.insert(id, (cur + delta).clamp(0.7, 2.5));
        }
    }

    fn cycle_selected_opacity(&mut self) {
        if let Some(id) = self.selected {
            if let Some(n) = self.app.state.workspace.get_node_mut(id) {
                const STEPS: [u8; 4] = [255, 235, 205, 170];
                let cur = n.style.background.a;
                let next = STEPS.iter().find(|a| **a < cur).copied().unwrap_or(255);
                n.style.background.a = next;
            }
        }
    }

    fn cycle_selected_tint(&mut self) {
        if let Some(id) = self.selected {
            if let Some(n) = self.app.state.workspace.get_node_mut(id) {
                const TINTS: [&str; 4] = ["#0b0d12", "#0d1410", "#101322", "#1a1214"];
                let cur = (
                    n.style.background.r,
                    n.style.background.g,
                    n.style.background.b,
                );
                let idx = TINTS
                    .iter()
                    .position(|h| {
                        let c = crate::surface::Color::from_hex(h);
                        (c.r, c.g, c.b) == cur
                    })
                    .map(|i| i + 1)
                    .unwrap_or(1)
                    % TINTS.len();
                n.style.background.a = n.style.background.a.max(170);
                let rgb = crate::surface::Color::from_hex(TINTS[idx]);
                n.style.background.r = rgb.r;
                n.style.background.g = rgb.g;
                n.style.background.b = rgb.b;
            }
        }
    }

    fn menu_items_for(&self, node: Option<crate::NodeId>) -> MenuItems {
        let kind = node
            .and_then(|id| self.app.state.workspace.get_node(id))
            .map(|n| {
                if n.projection.is_some() {
                    "projection"
                } else if n.surface_id().is_some() {
                    "terminal"
                } else {
                    "workspace"
                }
            })
            .unwrap_or("workspace");
        let ids: &[&str] = match kind {
            "terminal" => &[
                "terminal.copy",
                "view.pin",
                "view.pinLive",
                "style.fontBigger",
                "style.fontSmaller",
                "style.opacity",
                "style.tint",
                "layout.focus",
                "terminal.close",
            ],
            "projection" => &["layout.focus", "terminal.close"],
            _ => &[
                "terminal.new",
                "terminal.varied",
                "layout.tileH",
                "layout.tileV",
                "layout.tileGrid",
                "layout.cascade",
                "layout.orbit",
                "layout.focus",
                "layout.save",
                "layout.restore",
                "view.reduceMotion",
                "camera.fit",
                "help.open",
            ],
        };
        Self::palette_commands()
            .into_iter()
            .filter(|(id, _)| ids.contains(id))
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn open_menu(&mut self, x: f64, y: f64, node: Option<crate::NodeId>) {
        let items = self.menu_items_for(node);
        if items.is_empty() {
            return;
        }
        self.menu = Some(MenuState {
            x,
            y,
            items,
            index: 0,
        });
    }

    fn region_zoom_rect(&self, node: crate::NodeId, sx: f64, sy: f64) -> Option<crate::lens::Rect> {
        let sess = self.terms.iter().find(|s| s.node == node)?;
        let grid = &sess.engine.term.grid;
        let regions = crate::regions::detect_zoom_regions(grid);
        if regions.is_empty() {
            return None;
        }
        let cam = self.app.state.workspace.camera();
        let node_pos = self
            .app
            .state
            .workspace
            .get_node(node)
            .map(|n| (n.transform.x, n.transform.y))?;
        let (cell_w, line_h) = self.grid_cell;
        let header = self.header_h();
        let (col, row) = screen_to_cell(
            sx, sy, cam.x, cam.y, cam.zoom, node_pos.0, node_pos.1, GRID_PAD_X, header, cell_w,
            line_h, grid.cols, grid.rows,
        );
        let hit = regions.iter().find(|r| r.contains(col, row))?;
        let (ox, oy) = crate::arrange::grid_cell_origin(
            node_pos.0, node_pos.1, GRID_PAD_X, header, cell_w, line_h, hit.col, hit.row,
        );
        Some(crate::lens::Rect {
            x: ox,
            y: oy,
            width: (hit.cols as f64 * cell_w).max(1.0),
            height: (hit.rows as f64 * line_h).max(1.0),
        })
    }

    fn copy_selection_to_clipboard(&mut self, primary: bool) {
        let Some((surface, _, a, b)) = self.selection else {
            return;
        };
        let text = self
            .terms
            .iter()
            .find(|t| t.surface == surface)
            .map(|t| t.engine.term.selected_text(a, b))
            .unwrap_or_default();
        if text.is_empty() {
            return;
        }
        if primary {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                use arboard::{LinuxClipboardKind, SetExtLinux};
                let _ = cb.set().clipboard(LinuxClipboardKind::Primary).text(text);
            }
        } else if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(text);
        }
    }

    /// Header height above the grid: label row + clear padding so the
    /// node label and the first grid row never touch.
    fn header_h(&self) -> f64 {
        self.grid_cell.1 + 20.0
    }

    fn snap_camera(&mut self, x: f64, y: f64, zoom: f64) {
        let cam = self.app.state.workspace.camera_mut();
        cam.x = x;
        cam.y = y;
        cam.zoom = zoom;
        cam.target_x = x;
        cam.target_y = y;
        cam.target_zoom = zoom;
        cam.animating = false;
    }

    /// Fit every node into the viewport with a margin. `instant` snaps
    /// (startup: app must be ready immediately); otherwise set targets +
    /// `animating` for a smooth fly-to (`f` key).
    fn fit_dashboard(&mut self, instant: bool) {
        let nodes = self.app.state.workspace.all_nodes();
        if nodes.is_empty() {
            return;
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
        let (vw, vh) = (self.viewport_size.0 as f64, self.viewport_size.1 as f64);
        let bw = (x1 - x0 + 2.0 * DASH_MARGIN).max(1.0);
        let bh = (y1 - y0 + 2.0 * DASH_MARGIN).max(1.0);
        let zoom = (vw / bw).min(vh / bh).min(1.0).clamp(0.05, 64.0);
        let (tx, ty) = (x0 - DASH_MARGIN, y0 - DASH_MARGIN);
        if instant {
            self.snap_camera(tx, ty, zoom);
        } else {
            let cam = self.app.state.workspace.camera_mut();
            cam.target_x = tx;
            cam.target_y = ty;
            cam.target_zoom = zoom;
            cam.animating = true;
        }
    }

    /// Terminal node pixel size for a grid: measured cells + header + pads.
    fn terminal_node_size(&self, cols: u32, rows: u32) -> (f64, f64) {
        let (cell_w, line_h) = self.grid_cell;
        let w = cols as f64 * cell_w + 2.0 * GRID_PAD_X;
        let h = self.header_h() + rows as f64 * line_h + GRID_PAD_BOTTOM;
        (w, h)
    }

    /// Re-derive a session's grid + PTY from its node size (resize drag and
    /// window-resize share this; SIGWINCH flows via `pty.resize`).
    fn sync_session_grid(&mut self, node_id: crate::NodeId) {
        let (cell_w, line_h) = self.grid_cell;
        let header_h = self.header_h();
        let Some((nw, nh)) = self
            .app
            .state
            .workspace
            .get_node(node_id)
            .map(|n| (n.size.0.max(1.0), n.size.1.max(1.0)))
        else {
            return;
        };
        let (cols, rows) = crate::arrange::terminal_grid_size(
            nw,
            nh,
            cell_w,
            line_h,
            header_h,
            GRID_PAD_X,
            GRID_PAD_BOTTOM,
        );
        if let Some(sess) = self.terms.iter_mut().find(|s| s.node == node_id) {
            sess.engine.term.grid.resize(rows, cols);
            sess.engine.term.cursor_row = sess.engine.term.cursor_row.min(rows.saturating_sub(1));
            sess.engine.term.cursor_col = sess.engine.term.cursor_col.min(cols.saturating_sub(1));
            let _ = sess.pty.resize(cols as u16, rows as u16);
        }
    }

    /// Spawn a terminal node at `pos` and register its PTY session.
    fn spawn_terminal_node(&mut self, cols: u32, rows: u32, pos: (f64, f64)) -> Result<(), String> {
        let pty = PtySession::spawn(cols as u16, rows as u16, None)?;
        let surface = SurfaceId(self.next_surface_id);
        self.next_surface_id += 1;
        let node_id = crate::NodeId(self.next_node_id);
        self.next_node_id += 1;
        let engine = VtEngine::new(Terminal::new(surface, rows, cols));
        let (w, h) = self.terminal_node_size(cols, rows);
        let mut node = crate::Node::new(node_id, pos.0, pos.1);
        node.size = (w, h);
        node.set_surface(surface);
        node.set_input(crate::InputBehavior::new("terminal").with_key_focus());
        self.app.state.workspace.add_node(node);
        self.terms.push(TermSession {
            surface,
            node: node_id,
            engine,
            pty,
        });
        if self.focused_term.is_none() {
            self.focused_term = Some(surface);
        }
        Ok(())
    }

    fn focused_session_mut(&mut self) -> Option<&mut TermSession> {
        let id = self.focused_term?;
        self.terms.iter_mut().find(|s| s.surface == id)
    }

    /// Close a terminal node: drop its PTY session (the child sees SIGHUP),
    /// remove the node, and clear every reference to both. Returns false
    /// when `node_id` has no live session.
    fn close_terminal_node(&mut self, node_id: crate::NodeId) -> bool {
        let Some(pos) = self.terms.iter().position(|s| s.node == node_id) else {
            return false;
        };
        let sess = self.terms.remove(pos);
        self.app.state.workspace.remove_node(node_id);
        if self.selected == Some(node_id) {
            self.selected = None;
        }
        if self.focused_term == Some(sess.surface) {
            self.focused_term = None;
            self.term_focus = false;
        }
        if self.forwarding.map(|f| f.surface) == Some(sess.surface) {
            self.forwarding = None;
        }
        true
    }

    /// Close the selected node: full session close for terminals, plain
    /// node removal for projections/widgets. Returns false with nothing
    /// selected.
    fn close_selected(&mut self) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        if self.close_terminal_node(id) {
            return true;
        }
        self.app.state.workspace.remove_node(id);
        self.selected = None;
        true
    }

    /// Save the whole scene (nodes, groups, camera, bookmarks) to the
    /// XDG layout path. Terminal content is arrangement-only by design:
    /// shells re-spawn fresh on restore.
    fn save_layout(&mut self) {
        let path = crate::config::Config::layout_path();
        match self.save_layout_to(&path) {
            Ok(n) => eprintln!("layout saved ({} nodes) to {}", n, path.display()),
            Err(e) => eprintln!("layout save failed: {e}"),
        }
    }

    fn save_layout_to(&mut self, path: &std::path::Path) -> Result<usize, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let n = self.app.state.workspace.all_nodes().len();
        self.app
            .state
            .workspace
            .save_to_file(path)
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    /// Restore the scene from the XDG layout path: live sessions are
    /// dropped (children see SIGHUP) and every restored terminal node gets
    /// a fresh PTY, with projection sources remapped onto the new surfaces.
    fn restore_layout(&mut self) {
        let path = crate::config::Config::layout_path();
        match self.restore_layout_from(&path) {
            Ok(n) => eprintln!("layout restored ({} sessions) from {}", n, path.display()),
            Err(e) => eprintln!("layout restore failed: {e}"),
        }
    }

    fn restore_layout_from(&mut self, path: &std::path::Path) -> Result<usize, String> {
        self.terms.clear();
        self.focused_term = None;
        self.term_focus = false;
        self.forwarding = None;
        self.selecting = None;
        self.selection = None;
        self.app
            .state
            .workspace
            .load_from_file(path)
            .map_err(|e| e.to_string())?;
        // Terminal nodes (surface, no projection) get fresh sessions; the
        // remap table rewires projection sources onto the new surfaces.
        let terminals: Vec<(crate::NodeId, f64, f64, SurfaceId)> = self
            .app
            .state
            .workspace
            .all_nodes()
            .iter()
            .filter(|n| n.surface_id().is_some() && n.projection.is_none())
            .map(|n| (n.id, n.size.0, n.size.1, n.surface_id().unwrap()))
            .collect();
        let (cell_w, line_h) = self.grid_cell;
        let header_h = self.header_h();
        let mut remap: std::collections::HashMap<SurfaceId, SurfaceId> =
            std::collections::HashMap::new();
        let mut failed: Vec<crate::NodeId> = Vec::new();
        for (node_id, nw, nh, old_surface) in terminals {
            let (cols, rows) = crate::arrange::terminal_grid_size(
                nw.max(1.0),
                nh.max(1.0),
                cell_w,
                line_h,
                header_h,
                GRID_PAD_X,
                GRID_PAD_BOTTOM,
            );
            let surface = SurfaceId(self.next_surface_id);
            self.next_surface_id += 1;
            match PtySession::spawn(cols as u16, rows as u16, None) {
                Ok(pty) => {
                    let engine = VtEngine::new(Terminal::new(surface, rows, cols));
                    if let Some(node) = self.app.state.workspace.get_node_mut(node_id) {
                        node.set_surface(surface);
                    }
                    remap.insert(old_surface, surface);
                    self.terms.push(TermSession {
                        surface,
                        node: node_id,
                        engine,
                        pty,
                    });
                }
                Err(_) => failed.push(node_id),
            }
        }
        for dead in failed {
            self.app.state.workspace.remove_node(dead);
        }
        // Rewire projections; sources that match no revived session keep
        // their (now dangling) id and simply render stale content.
        let mut max_surface = self.next_surface_id;
        for node in self.app.state.workspace.scene.all_nodes_mut() {
            if let Some(s) = node.surface_id() {
                max_surface = max_surface.max(s.0 + 1);
            }
            if let Some(proj) = node.projection.as_mut() {
                if let Some(&fresh) = remap.get(&proj.source) {
                    proj.source = fresh;
                }
                max_surface = max_surface.max(proj.source.0 + 1);
            }
        }
        self.next_surface_id = max_surface;
        let max_node = self
            .app
            .state
            .workspace
            .node_ids()
            .iter()
            .map(|n| n.0)
            .max()
            .unwrap_or(0);
        self.next_node_id = max_node + 1;
        if let Some(first) = self.terms.first() {
            self.focused_term = Some(first.surface);
        }
        if let Some(sel) = self.selected {
            if self.app.state.workspace.get_node(sel).is_none() {
                self.selected = None;
            }
        }
        Ok(self.terms.len())
    }

    /// Resize a terminal node to an exact grid: node size follows from
    /// measured cells, then grid + PTY follow via SIGWINCH. Clamped 2..=256
    /// like drag resize. Returns false when `node_id` has no live session.
    fn resize_terminal_grid(&mut self, node_id: crate::NodeId, cols: u32, rows: u32) -> bool {
        if !self.terms.iter().any(|s| s.node == node_id) {
            return false;
        }
        let (cols, rows) = (cols.clamp(2, 256), rows.clamp(2, 256));
        let (w, h) = self.terminal_node_size(cols, rows);
        if let Some(node) = self.app.state.workspace.get_node_mut(node_id) {
            node.size = (w, h);
        }
        self.sync_session_grid(node_id);
        true
    }

    /// Step the selected terminal's grid by whole cells (border/menu grid
    /// control without pixel dragging). Returns false with no session.
    fn step_selected_grid(&mut self, dcols: i32, drows: i32) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        let Some(sess) = self.terms.iter().find(|s| s.node == id) else {
            return false;
        };
        let cols = (sess.engine.term.grid.cols as i32 + dcols).clamp(2, 256) as u32;
        let rows = (sess.engine.term.grid.rows as i32 + drows).clamp(2, 256) as u32;
        self.resize_terminal_grid(id, cols, rows)
    }

    fn draw_frame(&mut self) {
        let (Some(surface), Some(context), Some(gl)) =
            (&self.gl_surface, &self.gl_context, &self.gl)
        else {
            return;
        };
        // Camera easing toward its animated target (real seconds).
        let dt = self.last_frame.elapsed().as_secs_f64().max(0.001);
        self.last_frame = std::time::Instant::now();
        self.app.state.workspace.camera_mut().update(dt.min(0.1));
        // Reduce motion: every animated move lands instantly instead of
        // easing (config `accessibility.reduce_motion`, live-toggleable).
        if self.app.state.config.accessibility.reduce_motion {
            self.app.state.workspace.camera_mut().snap_to_targets();
        }

        // Drain PTY output into each VT engine and sync live projections.
        // Bounded per frame (128KB/session): a huge dump flows through
        // over frames instead of stalling one.
        for sess in &mut self.terms {
            let bytes = sess.pty.take_output_capped(128 * 1024);
            if !bytes.is_empty() {
                if self.diag_pty_bytes == 0 {
                    let sample: String = bytes
                        .iter()
                        .take(120)
                        .map(|b| b.escape_ascii().to_string())
                        .collect();
                    eprintln!("[fracterm] first PTY bytes ({}): {sample}", bytes.len());
                }
                self.diag_pty_bytes += bytes.len() as u64;
                sess.engine.feed(&bytes);
                sess.engine.term.reset_scroll();
            }
            // Answer terminal queries (DA etc.): the child may hold all
            // output until we reply.
            let reply = sess.engine.take_reply();
            if !reply.is_empty() {
                let _ = sess.pty.write(&reply);
            }
            // OSC 52 clipboard offers: the child (the user's own shell)
            // writes host clipboard/primary. Headless/empty is a no-op.
            if let Some((primary, text)) = sess.engine.take_clipboard() {
                if !text.is_empty() {
                    write_clipboard(primary, &text);
                }
            }
        }
        // OSC window title: the focused session's title wins, else the
        // default. Applied only on change.
        let want_title = self
            .focused_term
            .and_then(|f| self.terms.iter().find(|s| s.surface == f))
            .map(|s| s.engine.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "fracterm".to_string());
        if want_title != self.last_title {
            if let Some(window) = &self.window {
                window.set_title(&want_title);
            }
            self.last_title = want_title;
        }
        for sess in &self.terms {
            if !sess.engine.term.dirty {
                continue;
            }
            for node in self.app.state.workspace.scene.all_nodes_mut() {
                if let Some(proj) = &mut node.projection {
                    if proj.source == sess.surface {
                        proj.update_from_terminal(&sess.engine.term);
                    }
                }
            }
        }

        let palette_open = self.palette_open;
        let palette_query = self.palette_query.clone();
        let palette_index = self.palette_index;
        // The catalog is filtered only while open: every frame otherwise
        // paid filter + String clones for a hidden popup. Single
        // implementation via `palette_filtered` (fuzzy-ranked).
        let palette_items: Vec<(String, String)> = if palette_open {
            self.palette_filtered()
                .into_iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect()
        } else {
            Vec::new()
        };
        // Auto-zoom cues re-run the grid scan only when the selected
        // terminal's content changed or the node/metrics moved; idle
        // frames reuse the cached world rects. The `dirty` flag clears
        // below, after this frame's consumers ran.
        let region_cues: Vec<(f64, f64, f64, f64)> = match self.selected {
            Some(id) => {
                let idx = self.terms.iter().position(|s| s.node == id);
                let pos = self
                    .app
                    .state
                    .workspace
                    .get_node(id)
                    .map(|n| (n.transform.x, n.transform.y));
                let (cw, lh) = self.grid_cell;
                match (idx, pos) {
                    (Some(i), Some((nx, ny))) => {
                        let key = (nx.to_bits(), ny.to_bits(), cw.to_bits(), lh.to_bits());
                        let dirty = self.terms[i].engine.term.dirty;
                        if !dirty
                            && self.region_cache_node == Some(id)
                            && self.region_cache_key == key
                        {
                            self.region_cache.clone()
                        } else {
                            let regions = crate::regions::detect_zoom_regions(
                                &self.terms[i].engine.term.grid,
                            );
                            let header = self.header_h();
                            let cues: Vec<(f64, f64, f64, f64)> = regions
                                .into_iter()
                                .map(|r| {
                                    let (ox, oy) = crate::arrange::grid_cell_origin(
                                        nx, ny, GRID_PAD_X, header, cw, lh, r.col, r.row,
                                    );
                                    (ox, oy, r.cols as f64 * cw, r.rows as f64 * lh)
                                })
                                .collect();
                            self.region_cache_node = Some(id);
                            self.region_cache_key = key;
                            self.region_cache = cues.clone();
                            cues
                        }
                    }
                    _ => Vec::new(),
                }
            }
            None => Vec::new(),
        };
        for sess in &mut self.terms {
            sess.engine.term.dirty = false;
        }
        let scrollbars: Vec<ScrollBar> = {
            let zoom = self.app.state.workspace.camera().zoom.max(0.05);
            let mut out = Vec::new();
            for sess in &self.terms {
                let sb = sess.engine.term.grid.scrollback.len();
                if sb == 0 {
                    continue;
                }
                let Some(n) = self.app.state.workspace.get_node(sess.node) else {
                    continue;
                };
                let tw = 6.0 / zoom;
                let tx = n.transform.x + n.size.0.max(1.0) - tw - 3.0 / zoom;
                let ty = n.transform.y + self.header_h();
                let th = (n.size.1 - self.header_h() - GRID_PAD_BOTTOM).max(1.0);
                let rows = sess.engine.term.grid.rows.max(1) as f64;
                let frac = rows / (rows + sb as f64);
                let hh = (th * frac).max(12.0 / zoom);
                let off = (sess.engine.term.scroll_offset as f64).min(sb as f64);
                let hy = ty + (th - hh) * (1.0 - off / (sb as f64).max(1.0));
                out.push((tx, ty, tw, th, tx, hy, tw, hh));
            }
            out
        };
        let menu_snap: MenuSnap = self
            .menu
            .as_ref()
            .map(|m| (m.x, m.y, m.items.clone(), m.index));
        if let (Some(renderer), Some(graph)) = (&mut self.renderer, &self.graph) {
            let cam = self.app.state.workspace.camera();
            renderer.set_camera(cam.x, cam.y, cam.zoom);

            // ContentPass: one rect per node, batched into a single draw call.
            // The selected node gets an accent border; others keep theirs.
            let selected = self.selected;
            for node in self.app.state.workspace.all_nodes() {
                let bg = node.style.background;
                let (w, h) = node.size;
                let width = w.max(1.0);
                let height = h.max(1.0);
                renderer.push_rect(
                    node.transform.x,
                    node.transform.y,
                    width,
                    height,
                    (
                        bg.r as f32 / 255.0,
                        bg.g as f32 / 255.0,
                        bg.b as f32 / 255.0,
                        bg.a as f32 / 255.0,
                    ),
                );
                if Some(node.id) == selected {
                    renderer.push_border(
                        node.transform.x,
                        node.transform.y,
                        width,
                        height,
                        2.0,
                        (0.35, 0.7, 1.0, 1.0),
                    );
                } else {
                    let border = node.style.border;
                    renderer.push_border(
                        node.transform.x,
                        node.transform.y,
                        width,
                        height,
                        1.0,
                        (
                            border.r as f32 / 255.0,
                            border.g as f32 / 255.0,
                            border.b as f32 / 255.0,
                            border.a as f32 / 255.0,
                        ),
                    );
                }
            }
            // Resize handle: filled accent square at the selected node's
            // bottom-right corner, 12 screen px in world units.
            // Close button: same size at the top-right corner.
            if let Some(sel) = selected {
                if let Some(node) = self.app.state.workspace.get_node(sel) {
                    let handle = 12.0 / cam.zoom.max(0.05);
                    let (w, h) = (node.size.0.max(1.0), node.size.1.max(1.0));
                    renderer.push_overlay_rect(
                        node.transform.x + w - handle,
                        node.transform.y + h - handle,
                        handle,
                        handle,
                        (0.35, 0.7, 1.0, 1.0),
                    );
                    let (cx, cy, cs, _) =
                        close_button_rect(node.transform.x, node.transform.y, w, cam.zoom);
                    renderer.push_overlay_rect(cx, cy, cs, cs, (0.9, 0.35, 0.35, 1.0));
                    let (mx, my, ms) =
                        menu_button_rect(node.transform.x, node.transform.y, cam.zoom);
                    renderer.push_overlay_rect(mx, my, ms, ms, (0.35, 0.7, 1.0, 1.0));
                    for (rx, ry, rw, rh) in &region_cues {
                        renderer.push_border(*rx, *ry, *rw, *rh, 1.5, (0.4, 0.8, 1.0, 0.9));
                    }
                    for (tx, ty, tw, th, hx, hy, hw, hh) in &scrollbars {
                        renderer.push_overlay_rect(*tx, *ty, *tw, *th, (0.2, 0.22, 0.28, 0.8));
                        renderer.push_overlay_rect(*hx, *hy, *hw, *hh, (0.5, 0.65, 0.9, 0.9));
                    }
                }
            }

            unsafe {
                // ContentPass -> FBO (or direct screen when degraded).
                graph.begin_pass(gl, PassTarget::Content);
                renderer.draw(gl);
                graph.end_pass(gl, PassTarget::Content);

                // OverlayPass -> FBO, composited over content.
                graph.begin_pass(gl, PassTarget::Overlay);
                renderer.draw_overlay(gl);
                graph.end_pass(gl, PassTarget::Overlay);

                // PostProcessPass placeholder (effects land later).
                graph.present(gl);

                // Text pass: node labels + terminal grid, crisp at any zoom (P2/P3).
                if let (Some(atlas), Some(text), Some(fonts), Some(font_id)) = (
                    &mut self.atlas,
                    &mut self.text,
                    &mut self.fonts,
                    self.font_id,
                ) {
                    if let Some(fid) = Some(font_id) {
                        let (vw, vh) = (self.viewport_size.0, self.viewport_size.1);
                        let _ = (vw, vh);
                        // Terminal grids: one glyph per non-blank cell, set in
                        // measured cells under each session's node. Per-cell
                        // `queue_char_at` (no String alloc) with the pixel
                        // size hoisted per node; rows borrow short-lived so
                        // no per-frame line clone is needed.
                        let (cell_w, line_h) = self.grid_cell;
                        let header_h = line_h + 20.0;
                        let (camx, camy, zoom) = (cam.x, cam.y, cam.zoom);
                        // Visible world rect: terminals (then cells) fully
                        // outside it skip queueing entirely. Matters once
                        // dashboards hold many nodes or zoom deep.
                        let vzoom = zoom.max(0.05);
                        let (vx0, vy0) = (camx, camy);
                        let (vx1, vy1) = (
                            camx + self.viewport_size.0 as f64 / vzoom,
                            camy + self.viewport_size.1 as f64 / vzoom,
                        );
                        for i in 0..self.terms.len() {
                            let (nx, ny, nw, nh, surf, rows, cols, scale) = self
                                .app
                                .state
                                .workspace
                                .get_node(self.terms[i].node)
                                .map(|n| {
                                    (
                                        n.transform.x,
                                        n.transform.y,
                                        n.size.0.max(1.0),
                                        n.size.1.max(1.0),
                                        self.terms[i].surface,
                                        self.terms[i].engine.term.grid.rows,
                                        self.terms[i].engine.term.grid.cols,
                                        self.font_scales
                                            .get(&self.terms[i].node)
                                            .copied()
                                            .unwrap_or(1.0),
                                    )
                                })
                                .unwrap_or((0.0, 0.0, 1.0, 1.0, self.terms[i].surface, 0, 0, 1.0));
                            if !crate::arrange::rects_intersect(
                                nx,
                                ny,
                                nw,
                                nh,
                                vx0,
                                vy0,
                                vx1 - vx0,
                                vy1 - vy0,
                            ) {
                                continue;
                            }
                            // On-screen raster size, once per node: every
                            // cell shares this terminal's zoom + font scale.
                            let screen_px = crate::text::choose_pixel_size(
                                (GRID_PX as f32 * scale) as u32,
                                zoom,
                            );
                            let sel_range: Option<((u32, u32), (u32, u32))> = self
                                .selection
                                .filter(|(ss, _, _, _)| *ss == surf)
                                .map(|(_, _, a, b)| {
                                    if (a.1, a.0) <= (b.1, b.0) {
                                        (a, b)
                                    } else {
                                        (b, a)
                                    }
                                });
                            if let Some(((r0, c0), (r1, c1))) = sel_range {
                                let sc = crate::surface::Color::from_hex(
                                    &self.app.state.config.theme.selection,
                                );
                                let rgba = (
                                    sc.r as f32 / 255.0,
                                    sc.g as f32 / 255.0,
                                    sc.b as f32 / 255.0,
                                    0.85,
                                );
                                for r in r0..=r1.min(rows.saturating_sub(1)) {
                                    let cs = if r == r0 { c0 } else { 0 };
                                    let ce = if r == r1 { c1 } else { cols.saturating_sub(1) };
                                    let (ox, oy) = crate::arrange::grid_cell_origin(
                                        nx, ny, GRID_PAD_X, header_h, cell_w, line_h, cs, r,
                                    );
                                    renderer.push_overlay_rect(
                                        ox,
                                        oy,
                                        (ce - cs + 1) as f64 * cell_w,
                                        line_h,
                                        rgba,
                                    );
                                }
                            }
                            for r in 0..rows {
                                for c in 0..cols {
                                    let cell = self.terms[i]
                                        .engine
                                        .term
                                        .visible_line(r)
                                        .and_then(|l| l.get(c as usize))
                                        .map(|cell| (cell.character, cell.fg, cell.width));
                                    let Some((ch, fg, w)) = cell else {
                                        continue;
                                    };
                                    if ch == ' ' || w == 0 {
                                        continue;
                                    }
                                    let (ox, oy) = crate::arrange::grid_cell_origin(
                                        nx, ny, GRID_PAD_X, header_h, cell_w, line_h, c, r,
                                    );
                                    if ox < vx0 - cell_w
                                        || ox > vx1
                                        || oy < vy0 - line_h
                                        || oy > vy1
                                    {
                                        continue;
                                    }
                                    let color = (
                                        fg.r as f32 / 255.0,
                                        fg.g as f32 / 255.0,
                                        fg.b as f32 / 255.0,
                                        1.0,
                                    );
                                    text.queue_char_at(
                                        gl,
                                        atlas,
                                        fonts,
                                        fid,
                                        ((ox - camx) * zoom, (oy - camy) * zoom),
                                        screen_px,
                                        ch,
                                        color,
                                    );
                                }
                            }
                        }
                        // Terminal caret: shaped by DECSCUSR, hidden when the
                        // child asked (?25l), blinking styles gated by a
                        // ~530ms phase. Shown on the focused session only.
                        // The text pass runs after the overlay, so glyphs
                        // stay legible over a block caret.
                        let blink_on = self.blink_epoch.elapsed().as_millis() % 1060 < 530;
                        let cc =
                            crate::surface::Color::from_hex(&self.app.state.config.theme.cursor);
                        let cursor_rgba = (
                            cc.r as f32 / 255.0,
                            cc.g as f32 / 255.0,
                            cc.b as f32 / 255.0,
                            1.0,
                        );
                        let thin = 2.0 / cam.zoom.max(0.05);
                        let focused_surface = self.focused_term;
                        for sess in &self.terms {
                            if !self.term_focus
                                || Some(sess.surface) != focused_surface
                                || !sess.engine.cursor_visible
                            {
                                continue;
                            }
                            let style = sess.engine.cursor_style;
                            if style.blink && !blink_on {
                                continue;
                            }
                            let Some(node) = self
                                .app
                                .state
                                .workspace
                                .get_node(sess.node)
                                .map(|n| (n.transform.x, n.transform.y))
                            else {
                                continue;
                            };
                            let (ox, oy) = crate::arrange::grid_cell_origin(
                                node.0,
                                node.1,
                                GRID_PAD_X,
                                header_h,
                                cell_w,
                                line_h,
                                sess.engine.term.cursor_col,
                                sess.engine.term.cursor_row,
                            );
                            let (cx, cy, cw, ch) = match style.shape {
                                crate::vt::CursorShape::Block => (ox, oy, cell_w, line_h),
                                crate::vt::CursorShape::Bar => (ox, oy, thin, line_h),
                                crate::vt::CursorShape::Underline => {
                                    (ox, oy + line_h - thin, cell_w, thin)
                                }
                            };
                            renderer.push_overlay_rect(cx, cy, cw, ch, cursor_rgba);
                        }
                        for i in 0..self.terms.len() {
                            let (nx, ny, nid, cols, rows, off, scale) =
                                match self.app.state.workspace.get_node(self.terms[i].node).map(
                                    |n| {
                                        (
                                            n.transform.x,
                                            n.transform.y,
                                            n.id,
                                            self.terms[i].engine.term.grid.cols,
                                            self.terms[i].engine.term.grid.rows,
                                            self.terms[i].engine.term.scroll_offset,
                                            self.font_scales
                                                .get(&self.terms[i].node)
                                                .copied()
                                                .unwrap_or(1.0),
                                        )
                                    },
                                ) {
                                    Some(v) => v,
                                    None => continue,
                                };
                            let _ = nid;
                            let mut label = format!("{cols}x{rows} @{scale:.2}");
                            if off > 0 {
                                label.push_str(&format!(" +{off}"));
                            }
                            if !self.terms[i].engine.term.grid.scrollback.is_empty() {
                                label.push_str(&format!(
                                    " sb{}",
                                    self.terms[i].engine.term.grid.scrollback.len()
                                ));
                            }
                            text.queue_string(
                                gl,
                                atlas,
                                fonts,
                                fid,
                                (nx + 8.0, ny + 5.0),
                                (cam.x, cam.y, cam.zoom),
                                11,
                                &label,
                                (0.55, 0.62, 0.72, 1.0),
                            );
                        }
                        // Projection nodes: render their content lines.
                        for node in self.app.state.workspace.all_nodes() {
                            let Some(proj) = &node.projection else {
                                continue;
                            };
                            for (li, line) in proj.content.iter().enumerate() {
                                if line.trim().is_empty() {
                                    continue;
                                }
                                text.queue_string(
                                    gl,
                                    atlas,
                                    fonts,
                                    fid,
                                    (
                                        node.transform.x + 8.0,
                                        node.transform.y + 20.0 + 16.0 * li as f64,
                                    ),
                                    (cam.x, cam.y, cam.zoom),
                                    12,
                                    line,
                                    (0.55, 0.85, 0.65, 1.0),
                                );
                            }
                        }
                        if palette_open {
                            let items = &palette_items;
                            let sel = palette_index.min(items.len().saturating_sub(1));
                            let vw = self.viewport_size.0 as f64;
                            let vh = self.viewport_size.1 as f64;
                            let bw = 460.0f64;
                            let bh = (items.len().min(8) as f64 * 22.0 + 52.0).max(52.0);
                            let bx = cam.x + (vw / cam.zoom - bw) / 2.0;
                            let by = cam.y + (vh / cam.zoom - bh) / 2.0;
                            renderer.push_overlay_rect(bx, by, bw, bh, (0.07, 0.08, 0.11, 0.96));
                            renderer.push_border(bx, by, bw, bh, 1.5, (0.35, 0.7, 1.0, 1.0));
                            let prompt = format!("> {}", palette_query);
                            text.queue_string(
                                gl,
                                atlas,
                                fonts,
                                fid,
                                (bx + 12.0, by + 10.0),
                                (cam.x, cam.y, cam.zoom),
                                GRID_PX,
                                &prompt,
                                (0.9, 0.93, 1.0, 1.0),
                            );
                            for (i, (_id, title)) in items.iter().take(8).enumerate() {
                                let row = format!("{} {}", if i == sel { ">" } else { " " }, title);
                                let col = if i == sel {
                                    (0.55, 0.85, 1.0, 1.0)
                                } else {
                                    (0.75, 0.78, 0.85, 1.0)
                                };
                                text.queue_string(
                                    gl,
                                    atlas,
                                    fonts,
                                    fid,
                                    (bx + 12.0, by + 32.0 + i as f64 * 22.0),
                                    (cam.x, cam.y, cam.zoom),
                                    12,
                                    &row,
                                    col,
                                );
                            }
                        }
                        if let Some((mx, my, items, sel)) = &menu_snap {
                            let mw = 300.0f64;
                            let ih = 22.0f64;
                            let mh = (items.len() as f64 * ih + 20.0).max(20.0);
                            renderer.push_overlay_rect(*mx, *my, mw, mh, (0.07, 0.08, 0.11, 0.97));
                            renderer.push_border(*mx, *my, mw, mh, 1.5, (0.35, 0.7, 1.0, 1.0));
                            for (i, (_, title)) in items.iter().enumerate() {
                                let row =
                                    format!("{} {}", if i == *sel { ">" } else { " " }, title);
                                let col = if i == *sel {
                                    (0.55, 0.85, 1.0, 1.0)
                                } else {
                                    (0.75, 0.78, 0.85, 1.0)
                                };
                                text.queue_string(
                                    gl,
                                    atlas,
                                    fonts,
                                    fid,
                                    (mx + 12.0, my + 10.0 + i as f64 * ih),
                                    (cam.x, cam.y, cam.zoom),
                                    12,
                                    &row,
                                    col,
                                );
                            }
                        }
                        text.flush(gl, atlas, self.viewport_size);
                    }
                }
            }
        }
        self.diag_frames += 1;
        surface
            .swap_buffers(context)
            .expect("failed to swap GL buffers");
    }
}

impl ApplicationHandler for CanvasState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes()
            .with_title("fracterm")
            .with_inner_size(PhysicalSize::new(1280u32, 720u32));

        let (window, gl_config) = DisplayBuilder::new()
            .with_window_attributes(Some(window_attributes))
            .build(
                event_loop,
                glutin::config::ConfigTemplateBuilder::new(),
                |configs| {
                    configs
                        .reduce(|accum, config| {
                            let prefers = config.supports_transparency().unwrap_or(false)
                                & !accum.supports_transparency().unwrap_or(false)
                                || config.num_samples() > accum.num_samples();
                            if prefers {
                                config
                            } else {
                                accum
                            }
                        })
                        .expect("no suitable GL config found")
                },
            )
            .expect("failed to build GL display/window");

        self.gl_display = Some(gl_config.display());

        let window = window.expect("GL window was not created");
        let raw_window_handle = window.window_handle().expect("window handle").as_raw();
        let context_attributes = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(glutin::context::Version::new(
                3, 3,
            ))))
            .build(Some(raw_window_handle));
        let fallback =
            glutin::context::ContextAttributesBuilder::new().build(Some(raw_window_handle));

        let not_current = unsafe {
            gl_config
                .display()
                .create_context(&gl_config, &context_attributes)
                .or_else(|_| gl_config.display().create_context(&gl_config, &fallback))
                .expect("failed to create OpenGL 3.3 core context")
        };

        let (width, height): (u32, u32) = window.inner_size().into();
        let attrs = window
            .build_surface_attributes(
                glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new(),
            )
            .expect("failed to build surface attributes");
        let surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &attrs)
                .expect("failed to create GL window surface")
        };

        let context = not_current
            .make_current(&surface)
            .expect("failed to make GL context current");
        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|s| {
                gl_config.display().get_proc_address(s) as *const _
            })
        };

        let capabilities = Capabilities::detect(&gl);
        let mut renderer = unsafe { RectRenderer::new(&gl) }
            .expect("failed to create renderer (OpenGL 3.3 core required)");
        let bg = crate::surface::Color::from_hex(&self.app.state.config.theme.background);
        renderer.set_viewport(width as f32, height as f32);
        renderer.set_clear_color((
            bg.r as f32 / 255.0,
            bg.g as f32 / 255.0,
            bg.b as f32 / 255.0,
            bg.a as f32 / 255.0,
        ));

        let mut graph = unsafe { RenderGraphExecutor::new(&gl, capabilities.fbo_supported) }
            .expect("failed to create render graph executor");
        unsafe { graph.resize(&gl, width.max(1), height.max(1)) };

        self.graph = Some(graph);
        self.renderer = Some(renderer);

        // Text pipeline (P2): fontconfig discovery + glyph atlas.
        let mut fonts = FontSystem::new().ok();
        if let Some(fs) = &mut fonts {
            self.font_id = fs
                .load_family_stack(&["monospace", "DejaVu Sans Mono", "Noto Sans Symbols"])
                .ok()
                .and_then(|ids| ids.into_iter().next());
        }
        self.fonts = fonts;
        self.atlas = Some(unsafe { Atlas::new(&gl, 1024) });
        self.text =
            Some(unsafe { TextRenderer::new(&gl) }.expect("failed to create text renderer"));

        // Grid metrics from the real face: crisp cells, aligned startup.
        if let (Some(fonts), Some(font_id)) = (&mut self.fonts, self.font_id) {
            if let Some(face) = fonts.face_mut(font_id) {
                if let Some(m) = face.font_metrics(GRID_PX) {
                    self.grid_cell = m.cell_size();
                } else {
                    eprintln!("font metrics unavailable, using fallback cell");
                }
            }
        } else {
            eprintln!("font system unavailable, using fallback cell");
        }
        let (cell_w, line_h) = self.grid_cell;
        let _ = (cell_w, line_h);
        // Frame the terminal with the camera: a centering fit, snapped
        // instantly, so no viewport space is wasted on empty canvas.
        // `p` remains the on-demand way to create views.
        // Startup grid: rows anchored at GRID_ROWS, columns derived from
        // the real window aspect so the node fills the view (no side bars).
        let (cols, rows) = crate::arrange::ideal_grid_for_view(
            width as f64,
            height as f64,
            self.grid_cell.0,
            self.grid_cell.1,
            self.header_h(),
            GRID_PAD_X,
            GRID_PAD_BOTTOM,
            GRID_ROWS,
        );
        let (term_w, term_h) = self.terminal_node_size(cols, rows);
        let tx = (width as f64 - term_w) / 2.0;
        let ty = (height as f64 - term_h) / 2.0;
        self.viewport_size = (width as f32, height as f32);
        // Terminal session (P3): bash in a PTY sized to the window.
        match self.spawn_terminal_node(cols, rows, (tx, ty)) {
            Ok(()) => {}
            Err(e) => eprintln!("failed to spawn PTY: {e}"),
        }
        {
            let rect = crate::lens::Rect {
                x: tx,
                y: ty,
                width: term_w.max(1.0),
                height: term_h.max(1.0),
            };
            let (fx, fy, fz) = {
                let cam = self.app.state.workspace.camera_mut();
                cam.fit_rect(rect, width as f64, height as f64);
                (cam.target_x, cam.target_y, cam.target_zoom)
            };
            self.snap_camera(fx, fy, fz);
        }
        {
            let cam = self.app.state.workspace.camera();
            eprintln!(
                "[fracterm] startup viewport={}x{} cell={:.2}x{:.2} terms={} cam=({:.0},{:.0},x{:.2})",
                width,
                height,
                self.grid_cell.0,
                self.grid_cell.1,
                self.terms.len(),
                cam.x,
                cam.y,
                cam.zoom,
            );
            for n in self.app.state.workspace.all_nodes() {
                eprintln!(
                    "[fracterm] node {} at ({},{}) size {:?} surf {:?}",
                    n.id.0,
                    n.transform.x,
                    n.transform.y,
                    n.size,
                    n.surface_id(),
                );
            }
        }
        self.capabilities = Some(capabilities);
        self.gl = Some(gl);
        self.viewport_size = (width as f32, height as f32);
        self.gl_context = Some(context);
        self.gl_surface = Some(surface);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window_id != window.id() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw_frame(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let (Some(surface), Some(context)) = (&self.gl_surface, &self.gl_context) {
                        surface.resize(
                            context,
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        );
                    }
                    if let Some(r) = &mut self.renderer {
                        r.set_viewport(size.width as f32, size.height as f32);
                    }
                    self.viewport_size = (size.width as f32, size.height as f32);
                    if let (Some(g), Some(gl)) = (&mut self.graph, &self.gl) {
                        unsafe { g.resize(gl, size.width, size.height) };
                    }
                    // Resize each terminal grid from its own node size, so
                    // window resizes never wipe content (resize preserves).
                    let node_ids: Vec<crate::NodeId> = self.terms.iter().map(|s| s.node).collect();
                    for node_id in node_ids {
                        self.sync_session_grid(node_id);
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // HiDPI: the renderer works in physical pixels; only the
                // input mapping needs the scale factor.
                self.app.state.hidpi_scale = scale_factor;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let new_cursor = (position.x, position.y);
                // A terminal child owning the mouse gets motion reports
                // (1002/1003 drag encoding), never canvas drags.
                if let Some(fwd) = self.forwarding {
                    let pending = self
                        .terms
                        .iter()
                        .position(|s| s.surface == fwd.surface)
                        .and_then(|i| {
                            let (col, row) = self.mouse_cell_for(i, new_cursor.0, new_cursor.1)?;
                            let st = self.modifiers.state();
                            self.terms[i]
                                .engine
                                .mouse_mode
                                .encode(&crate::vt::MouseReport {
                                    button: fwd.button,
                                    col,
                                    row,
                                    shift: st.shift_key(),
                                    alt: st.alt_key(),
                                    ctrl: st.control_key(),
                                    release: false,
                                    motion: true,
                                    dragging: true,
                                })
                        });
                    if let Some(bytes) = pending {
                        self.write_to_surface(fwd.surface, &bytes);
                    }
                    self.last_cursor = new_cursor;
                    return;
                }
                // Deferred middle pan: start panning only past the click
                // threshold, so middle-click stays a primary paste.
                if let Some(anchor) = self.middle_down {
                    let dx = new_cursor.0 - anchor.0;
                    let dy = new_cursor.1 - anchor.1;
                    if dx.hypot(dy) > 5.0 {
                        self.middle_down = None;
                        self.drag = DragState::Pan {
                            ax: anchor.0,
                            ay: anchor.1,
                        };
                    } else {
                        self.last_cursor = new_cursor;
                        return;
                    }
                }
                if let Some((surface, node, anchor)) = self.selecting {
                    if let Some(idx) = self.terms.iter().position(|t| t.surface == surface) {
                        if let Some(cell) = self.mouse_cell_for(idx, new_cursor.0, new_cursor.1) {
                            self.selection = Some((surface, node, anchor, (cell.0, cell.1)));
                        }
                    }
                    self.last_cursor = new_cursor;
                    return;
                }
                let cam = self.app.state.workspace.camera().clone();
                let zoom = cam.zoom;
                match self.drag {
                    DragState::Pan { ax, ay } => {
                        // Direct manipulation: route through `pan` so the
                        // stale target can never steer the camera back.
                        let (dx, dy) = (-(new_cursor.0 - ax) / zoom, -(new_cursor.1 - ay) / zoom);
                        self.app.state.workspace.camera_mut().pan(dx, dy);
                        self.drag = DragState::Pan {
                            ax: new_cursor.0,
                            ay: new_cursor.1,
                        };
                    }
                    DragState::Move { node, dx, dy } => {
                        let (wx, wy) = (cam.x + new_cursor.0 / zoom, cam.y + new_cursor.1 / zoom);
                        let (nx, ny) = (wx + dx, wy + dy);
                        // Snap the moved rect to nearby edges (8 world px).
                        let tmp = self.app.state.workspace.get_node(node).map(|n| {
                            let mut c = n.clone();
                            c.transform.x = nx;
                            c.transform.y = ny;
                            c
                        });
                        let others: Vec<crate::Node> = self
                            .app
                            .state
                            .workspace
                            .all_nodes()
                            .iter()
                            .filter(|n| n.id != node)
                            .map(|n| (*n).clone())
                            .collect();
                        let (fx, fy) = match tmp {
                            Some(ref t) => {
                                let refs: Vec<&crate::Node> = others.iter().collect();
                                let g = crate::arrange::snap_to_edges(t, &refs, 8.0);
                                if g.distance.is_finite() {
                                    (g.x, g.y)
                                } else {
                                    (nx, ny)
                                }
                            }
                            None => (nx, ny),
                        };
                        if let Some(n) = self.app.state.workspace.get_node_mut(node) {
                            n.transform.x = fx;
                            n.transform.y = fy;
                        }
                    }
                    DragState::Resize { node } => {
                        let (wx, wy) = (cam.x + new_cursor.0 / zoom, cam.y + new_cursor.1 / zoom);
                        let (pos, min) = {
                            let grid = self.grid_cell;
                            let header = self.header_h();
                            let p = self
                                .app
                                .state
                                .workspace
                                .get_node(node)
                                .map(|n| (n.transform.x, n.transform.y));
                            let min_w = 2.0 * grid.0 + 2.0 * GRID_PAD_X;
                            let min_h = header + 2.0 * grid.1 + GRID_PAD_BOTTOM;
                            (p, (min_w, min_h))
                        };
                        if let Some((nx, ny)) = pos {
                            let nw = (wx - nx).max(min.0);
                            let nh = (wy - ny).max(min.1);
                            if let Some(n) = self.app.state.workspace.get_node_mut(node) {
                                n.size = (nw.max(1.0), nh.max(1.0));
                            }
                            self.sync_session_grid(node);
                        }
                    }
                    DragState::None => {}
                }
                self.last_cursor = new_cursor;
            }
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Right) => {
                    self.rect_anchor = Some(self.last_cursor);
                }
                (ElementState::Released, MouseButton::Right) => {
                    self.finish_right_click(event_loop);
                }
                (ElementState::Pressed, MouseButton::Middle) => {
                    // Deferred pan: a release without drag pastes the
                    // primary selection (X11); movement beyond the click
                    // threshold becomes a pan in CursorMoved.
                    self.middle_down = Some(self.last_cursor);
                }
                (ElementState::Pressed, MouseButton::Left) => {
                    // Copy hit data into owned values before mutating self.
                    let cam = self.app.state.workspace.camera().clone();
                    let (wx, wy) = (
                        cam.x + self.last_cursor.0 / cam.zoom,
                        cam.y + self.last_cursor.1 / cam.zoom,
                    );
                    if self.menu.is_some() {
                        let (mx, my) = (wx, wy);
                        let (mmx, mmy, mitems, _) = self
                            .menu
                            .as_ref()
                            .map(|m| (m.x, m.y, m.items.clone(), m.index))
                            .unwrap();
                        let mw = 300.0f64;
                        let ih = 22.0f64;
                        let mh = mitems.len() as f64 * ih + 20.0;
                        self.menu = None;
                        if mx >= mmx
                            && mx <= mmx + mw
                            && my >= mmy
                            && my <= mmy + mh
                            && my >= mmy + 10.0
                        {
                            let idx = ((my - mmy - 10.0) / ih) as usize;
                            if let Some((id, _)) = mitems.get(idx) {
                                let owned = id.clone();
                                self.run_palette_command(&owned);
                            }
                            return;
                        }
                    }
                    if let Some(sel) = self.selected {
                        if let Some(n) = self.app.state.workspace.get_node(sel) {
                            let (mbx, mby, mbs) =
                                menu_button_rect(n.transform.x, n.transform.y, cam.zoom);
                            if wx >= mbx && wx <= mbx + mbs && wy >= mby && wy <= mby + mbs {
                                self.open_menu(n.transform.x, n.transform.y, Some(sel));
                                return;
                            }
                        }
                    }
                    // Selected node's close button wins over everything.
                    if let Some(sel) = self.selected {
                        if let Some(n) = self.app.state.workspace.get_node(sel) {
                            let (cx, cy, cs, _) =
                                close_button_rect(n.transform.x, n.transform.y, n.size.0, cam.zoom);
                            if wx >= cx && wx <= cx + cs && wy >= cy && wy <= cy + cs {
                                self.close_selected();
                                return;
                            }
                        }
                    }
                    let handle_hit = match self.selected {
                        Some(sel) => self
                            .app
                            .state
                            .workspace
                            .get_node(sel)
                            .map(|n| {
                                crate::arrange::resize_handle_hit(
                                    n.transform.x,
                                    n.transform.y,
                                    n.size.0.max(1.0),
                                    n.size.1.max(1.0),
                                    cam.x,
                                    cam.y,
                                    cam.zoom,
                                    self.last_cursor.0,
                                    self.last_cursor.1,
                                    10.0,
                                )
                            })
                            .unwrap_or(false),
                        None => false,
                    };
                    if handle_hit {
                        if let Some(sel) = self.selected {
                            self.drag = DragState::Resize { node: sel };
                        }
                    } else {
                        let hit = self.hit_node(wx, wy).map(|n| {
                            (
                                n.id,
                                n.surface_id(),
                                n.input.mode.clone(),
                                n.transform.x,
                                n.transform.y,
                            )
                        });
                        // Child mouse forwarding (xterm 1000+): an unshifted
                        // press on a reporting terminal goes to its PTY
                        // instead of starting a Move-drag. Shift forces host
                        // behavior; Alt forces a host move-drag (spec §5.1);
                        // space forces a host pan (see below). The resize
                        // handle (checked above) always wins.
                        let mst = self.modifiers.state();
                        let space_pan = self.space_down && !self.term_focus;
                        let forward_press: Option<(crate::NodeId, SurfaceId, Vec<u8>)> =
                            if mst.shift_key() || mst.alt_key() || space_pan {
                                None
                            } else if let Some((id, Some(surface), mode, _, _)) = &hit {
                                if mode.as_str() != "terminal" {
                                    None
                                } else {
                                    self.terms
                                        .iter()
                                        .position(|s| s.surface == *surface)
                                        .and_then(|i| {
                                            let (col, row) = self.mouse_cell_for(
                                                i,
                                                self.last_cursor.0,
                                                self.last_cursor.1,
                                            )?;
                                            let st = self.modifiers.state();
                                            let bytes = self.terms[i].engine.mouse_mode.encode(
                                                &crate::vt::MouseReport {
                                                    button: crate::vt::MouseButton::Left,
                                                    col,
                                                    row,
                                                    shift: false,
                                                    alt: st.alt_key(),
                                                    ctrl: st.control_key(),
                                                    release: false,
                                                    motion: false,
                                                    dragging: false,
                                                },
                                            )?;
                                            Some((*id, *surface, bytes))
                                        })
                                }
                            } else {
                                None
                            };
                        if let Some((id, surface, bytes)) = forward_press {
                            self.write_to_surface(surface, &bytes);
                            self.selected = Some(id);
                            self.focused_term = Some(surface);
                            self.term_focus = true;
                            self.forwarding = Some(ForwardMouse {
                                surface,
                                button: crate::vt::MouseButton::Left,
                            });
                            return;
                        }
                        if space_pan {
                            // Space-drag pans even when grabbed on a node:
                            // full-bleed startup leaves no empty canvas for
                            // a plain pan-grab, and middle-drag is not
                            // discoverable. Selection/focus stay untouched.
                            self.drag = DragState::Pan {
                                ax: self.last_cursor.0,
                                ay: self.last_cursor.1,
                            };
                            return;
                        }
                        if self.modifiers.state().shift_key() {
                            if let Some((id, Some(surface), mode, _, _)) = &hit {
                                if mode.as_str() == "terminal" {
                                    if let Some(idx) =
                                        self.terms.iter().position(|t| t.surface == *surface)
                                    {
                                        if let Some(cell) = self.mouse_cell_for(
                                            idx,
                                            self.last_cursor.0,
                                            self.last_cursor.1,
                                        ) {
                                            self.selecting =
                                                Some((*surface, *id, (cell.0, cell.1)));
                                            self.selection = None;
                                            self.selected = Some(*id);
                                            self.focused_term = Some(*surface);
                                            self.term_focus = true;
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        match hit {
                            Some((id, surface, mode, nx, ny)) => {
                                self.selected = Some(id);
                                match (surface, mode.as_str()) {
                                    (Some(surface), "terminal")
                                        if self.terms.iter().any(|s| s.surface == surface) =>
                                    {
                                        self.focused_term = Some(surface);
                                        self.term_focus = true;
                                    }
                                    _ => {}
                                }
                                self.drag = DragState::Move {
                                    node: id,
                                    dx: nx - wx,
                                    dy: ny - wy,
                                };
                            }
                            None => {
                                self.selected = None;
                                self.term_focus = false;
                                self.drag = DragState::Pan {
                                    ax: self.last_cursor.0,
                                    ay: self.last_cursor.1,
                                };
                            }
                        }
                    }
                }
                (ElementState::Released, MouseButton::Left) => {
                    // End child mouse ownership with a release report.
                    if let Some(fwd) = self.forwarding {
                        let pending = self
                            .terms
                            .iter()
                            .position(|s| s.surface == fwd.surface)
                            .and_then(|i| {
                                let (col, row) =
                                    self.mouse_cell_for(i, self.last_cursor.0, self.last_cursor.1)?;
                                let st = self.modifiers.state();
                                self.terms[i]
                                    .engine
                                    .mouse_mode
                                    .encode(&crate::vt::MouseReport {
                                        button: fwd.button,
                                        col,
                                        row,
                                        shift: st.shift_key(),
                                        alt: st.alt_key(),
                                        ctrl: st.control_key(),
                                        release: true,
                                        motion: false,
                                        dragging: false,
                                    })
                            });
                        if let Some(bytes) = pending {
                            self.write_to_surface(fwd.surface, &bytes);
                        }
                        self.forwarding = None;
                    }
                    if let Some((surface, _node, a)) = self.selecting.take() {
                        if let Some(sel) = self.selection {
                            let text = self
                                .terms
                                .iter()
                                .find(|t| t.surface == surface)
                                .map(|t| t.engine.term.selected_text(a, sel.3))
                                .unwrap_or_default();
                            if !text.is_empty() {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    let _ = cb.set_text(text.clone());
                                }
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    use arboard::{LinuxClipboardKind, SetExtLinux};
                                    let _ =
                                        cb.set().clipboard(LinuxClipboardKind::Primary).text(text);
                                }
                            }
                        }
                    }
                    self.drag = DragState::None;
                }
                (ElementState::Released, MouseButton::Middle) => {
                    if self.middle_down.take().is_some() {
                        // Middle-click (no drag): paste primary at the
                        // cursor session, if any.
                        let cam = self.app.state.workspace.camera().clone();
                        let (wx, wy) = (
                            cam.x + self.last_cursor.0 / cam.zoom,
                            cam.y + self.last_cursor.1 / cam.zoom,
                        );
                        if let Some(surface) = self
                            .hit_node(wx, wy)
                            .and_then(|n| n.surface_id())
                            .filter(|s| self.terms.iter().any(|t| t.surface == *s))
                        {
                            self.paste_clipboard_into(surface, true);
                        }
                    } else {
                        self.drag = DragState::None;
                    }
                }
                _ => {}
            },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: _,
                ..
            } => {
                // Space doubles as a pan modifier (space-drag pans even
                // over a node). Track it on press AND release, before the
                // release early-return: typing still works because the
                // flag is only honored when no terminal has focus.
                if event.logical_key
                    == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space)
                {
                    self.space_down = event.state == ElementState::Pressed;
                }
                // Releases carry no text and must never write to the PTY
                // (otherwise Enter/Backspace fire twice: press + release).
                if event.state != ElementState::Pressed {
                    return;
                }
                let modifiers = self.modifiers;
                let st = modifiers.state();
                if st.control_key()
                    && matches!(&event.logical_key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("k"))
                {
                    self.palette_open = !self.palette_open;
                    self.palette_query.clear();
                    self.palette_index = 0;
                    return;
                }
                if self.menu.is_some() {
                    use winit::keyboard::{Key, NamedKey};
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.menu = None;
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(m) = self.menu.take() {
                                if let Some((id, _)) = m.items.get(m.index) {
                                    let owned = id.clone();
                                    self.run_palette_command(&owned);
                                }
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(m) = self.menu.as_mut() {
                                m.index = m.index.saturating_sub(1);
                            }
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(m) = self.menu.as_mut() {
                                let n = m.items.len().saturating_sub(1);
                                m.index = (m.index + 1).min(n);
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if self.palette_open {
                    use winit::keyboard::{Key, NamedKey};
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.palette_open = false;
                            self.palette_query.clear();
                        }
                        Key::Named(NamedKey::Enter) => {
                            let items = self.palette_filtered();
                            if let Some((id, _)) = items.get(self.palette_index).copied() {
                                self.palette_open = false;
                                self.palette_query.clear();
                                self.palette_index = 0;
                                let owned = id.to_string();
                                self.run_palette_command(&owned);
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.palette_index = self.palette_index.saturating_sub(1);
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            let n = self.palette_filtered().len().saturating_sub(1);
                            self.palette_index = (self.palette_index + 1).min(n);
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.palette_query.pop();
                            self.palette_index = 0;
                        }
                        Key::Character(ch)
                            if event.text.as_ref().is_some_and(|t| !t.is_empty()) =>
                        {
                            let t = event.text.clone().unwrap_or_default();
                            for ch in t.chars() {
                                if !ch.is_control() {
                                    self.palette_query.push(ch);
                                }
                            }
                            let _ = ch;
                            self.palette_index = 0;
                        }
                        _ => {}
                    }
                    return;
                }
                if st.control_key()
                    && st.shift_key()
                    && matches!(&event.logical_key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("c"))
                {
                    self.copy_selection_to_clipboard(false);
                    return;
                }
                use winit::keyboard::{Key, NamedKey};
                // Esc always drops back to workspace control.
                if self.term_focus && event.logical_key == Key::Named(NamedKey::Escape) {
                    self.term_focus = false;
                    return;
                }
                // Workspace shortcuts (workspace mode, not terminal focus).
                if !self.term_focus {
                    match &event.logical_key {
                        Key::Character(c) if c == "p" => {
                            self.run_palette_command("view.pin");
                        }
                        Key::Character(c) if c == "P" => {
                            self.run_palette_command("view.pinLive");
                        }
                        Key::Character(c) if c == "t" => {
                            self.run_palette_command("layout.tileGrid");
                        }
                        Key::Character(c) if c == "h" => {
                            self.run_palette_command("layout.tileH");
                        }
                        Key::Character(c) if c == "v" => {
                            self.run_palette_command("layout.tileV");
                        }
                        Key::Character(c) if c == "a" => {
                            self.run_palette_command("layout.alignLeft");
                        }
                        Key::Character(c) if c == "b" => {
                            self.run_palette_command("camera.bookmarkSave");
                        }
                        Key::Character(c)
                            if c.len() == 1
                                && c.chars().next().unwrap().is_ascii_digit()
                                && c != "0" =>
                        {
                            let n = c.chars().next().unwrap();
                            let name = format!("bm{n}");
                            if self
                                .app
                                .state
                                .workspace
                                .camera_mut()
                                .restore_bookmark(&name)
                            {
                                eprintln!("bookmark {name} restored");
                            }
                        }
                        Key::Character(c) if c == "d" => {
                            self.run_palette_command("layout.dashboard");
                        }
                        Key::Character(c) if c == "f" => {
                            self.run_palette_command("camera.fit");
                        }
                        Key::Character(c) if c == "n" => {
                            self.run_palette_command("terminal.new");
                        }
                        Key::Character(c) if c == "x" => {
                            self.run_palette_command("terminal.close");
                        }
                        Key::Character(c) if c == "[" => {
                            self.step_selected_grid(-1, 0);
                        }
                        Key::Character(c) if c == "]" => {
                            self.step_selected_grid(1, 0);
                        }
                        Key::Character(c) if c == "-" => {
                            self.step_selected_grid(0, -1);
                        }
                        Key::Character(c) if c == "=" => {
                            self.step_selected_grid(0, 1);
                        }
                        Key::Character(c) if c == "C" => {
                            self.run_palette_command("layout.cascade");
                        }
                        Key::Character(c) if c == "O" => {
                            self.run_palette_command("layout.orbit");
                        }
                        Key::Character(c) if c == "F" => {
                            self.run_palette_command("layout.focus");
                        }
                        Key::Character(c) if c == "N" => {
                            self.run_palette_command("terminal.varied");
                        }
                        Key::Character(c) if c == "S" => {
                            self.run_palette_command("layout.save");
                        }
                        Key::Character(c) if c == "," => {
                            self.run_palette_command("style.fontSmaller");
                        }
                        Key::Character(c) if c == "." => {
                            self.run_palette_command("style.fontBigger");
                        }
                        Key::Character(c) if c == "o" => {
                            self.run_palette_command("style.opacity");
                        }
                        Key::Character(c) if c == "c" => {
                            self.run_palette_command("style.tint");
                        }
                        Key::Character(c) if c == ":" => {
                            self.palette_open = true;
                            self.palette_query.clear();
                            self.palette_index = 0;
                        }
                        Key::Character(c) if c == "?" => {
                            self.run_palette_command("help.open");
                        }
                        Key::Character(c) if c == "0" => {
                            // `0` restores slot bm0 when one was saved,
                            // otherwise it zooms to workspace fit.
                            if !self
                                .app
                                .state
                                .workspace
                                .camera_mut()
                                .restore_bookmark("bm0")
                            {
                                self.app
                                    .state
                                    .workspace
                                    .camera_mut()
                                    .zoom_to_workspace_fit();
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.term_focus = true;
                        }
                        Key::Character(c) if c == "i" => {
                            self.term_focus = true;
                        }
                        _ => {}
                    }
                } else {
                    // Terminal input handling.
                    let st = self.modifiers.state();
                    // Paste: Ctrl+Shift+V or Shift+Insert (clipboard,
                    // bracket-framed when the child asked for ?2004).
                    let paste_key = (matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v"))
                        && st.control_key()
                        && st.shift_key())
                        || (event.logical_key == Key::Named(NamedKey::Insert) && st.shift_key());
                    if paste_key {
                        if self.term_focus {
                            if let Some(surface) = self.focused_term {
                                self.paste_clipboard_into(surface, false);
                            }
                        }
                        return;
                    }
                    let Some(text) = &event.text else {
                        // Named keys map to control sequences.
                        let seq = named_key_sequence(&event.logical_key);
                        if let Some(bytes) = seq {
                            if let Some(sess) = self.focused_session_mut() {
                                let _ = sess.pty.write(&bytes);
                            }
                        }
                        return;
                    };
                    if let Some(sess) = self.focused_session_mut() {
                        let bytes = encode_text_input(text, modifiers.state().control_key());
                        if !bytes.is_empty() {
                            let _ = sess.pty.write(&bytes);
                        }
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m,
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, dy) => dy as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                let st = self.modifiers.state();
                if st.control_key() {
                    self.zoom_at_cursor(1.15f64.powf(lines));
                } else if st.shift_key() {
                    if let Some(id) = self.focused_term {
                        if let Some(sess) = self.terms.iter_mut().find(|t| t.surface == id) {
                            sess.engine.term.scroll(-lines.round() as i32 * 3);
                        }
                    }
                } else {
                    let cam = self.app.state.workspace.camera().clone();
                    let (wx, wy) = (
                        cam.x + self.last_cursor.0 / cam.zoom,
                        cam.y + self.last_cursor.1 / cam.zoom,
                    );
                    let node = self.hit_node(wx, wy).map(|n| n.id);
                    let sess_node =
                        node.and_then(|id| self.terms.iter().position(|s| s.node == id));
                    if let Some(i) = sess_node {
                        // A reporting child owns the wheel (vim/less/tmux
                        // scroll); otherwise it scrolls host scrollback.
                        // Shift already branched above (host bypass, xterm
                        // style); Ctrl+wheel always zooms.
                        let forwarded = wheel_button(lines).and_then(|(button, count)| {
                            let (col, row) =
                                self.mouse_cell_for(i, self.last_cursor.0, self.last_cursor.1)?;
                            let st = self.modifiers.state();
                            let report = crate::vt::MouseReport {
                                button,
                                col,
                                row,
                                shift: false,
                                alt: st.alt_key(),
                                ctrl: false,
                                release: false,
                                motion: false,
                                dragging: false,
                            };
                            if !self.terms.get(i)?.engine.mouse_mode.wants(&report) {
                                return None;
                            }
                            let mut out = Vec::new();
                            for _ in 0..count {
                                out.extend(self.terms.get(i)?.engine.mouse_mode.encode(&report)?);
                            }
                            Some(out)
                        });
                        match forwarded {
                            Some(bytes) => {
                                let surface = self.terms[i].surface;
                                self.write_to_surface(surface, &bytes);
                            }
                            None => self.terms[i].engine.term.scroll(-lines.round() as i32 * 3),
                        }
                    } else {
                        self.zoom_at_cursor(1.15f64.powf(lines));
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

/// Read clipboard (or X11 primary selection) text. `None` covers every
/// failure mode — no display, empty, non-text — so callers treat it as
/// a no-op (this is also why headless CI never pastes).
fn read_clipboard(primary: bool) -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    if primary {
        {
            use arboard::{GetExtLinux, LinuxClipboardKind};
            cb.get().clipboard(LinuxClipboardKind::Primary).text().ok()
        }
    } else {
        cb.get_text().ok()
    }
}

/// Write text to the host clipboard (or X11 primary selection).
/// Failures (headless, no display) are silent no-ops.
fn write_clipboard(primary: bool, text: &str) {
    if primary {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            use arboard::{LinuxClipboardKind, SetExtLinux};
            let _ = cb.set().clipboard(LinuxClipboardKind::Primary).text(text);
        }
    } else if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_string());
    }
}

/// Close-button rect (world units) at a node's top-right corner, 12
/// screen px square like the resize handle. Pure: unit-tested.
fn close_button_rect(node_x: f64, node_y: f64, node_w: f64, zoom: f64) -> (f64, f64, f64, f64) {
    let s = 12.0 / zoom.max(0.05);
    (node_x + node_w.max(1.0) - s, node_y, s, s)
}

fn menu_button_rect(node_x: f64, node_y: f64, zoom: f64) -> (f64, f64, f64) {
    let s = 12.0 / zoom.max(0.05);
    (node_x, node_y, s)
}

/// Wheel delta (scroll lines, +up) -> child mouse button + repeat count.
/// Positive scrolls up (button 64), negative down (65); sub-notch deltas
/// are no-ops, floods clamp at 8 reports. Pure: unit-tested.
fn wheel_button(lines: f64) -> Option<(crate::vt::MouseButton, u32)> {
    let n = lines.abs().round() as u32;
    if n == 0 {
        return None;
    }
    let button = if lines > 0.0 {
        crate::vt::MouseButton::WheelUp
    } else {
        crate::vt::MouseButton::WheelDown
    };
    Some((button, n.min(8)))
}

/// Screen px -> 0-based terminal cell, clamped into the grid (clicks on
/// padding/header report the edge cell). Pure: unit-tested.
#[allow(clippy::too_many_arguments)]
fn screen_to_cell(
    sx: f64,
    sy: f64,
    cam_x: f64,
    cam_y: f64,
    zoom: f64,
    node_x: f64,
    node_y: f64,
    pad_x: f64,
    header_h: f64,
    cell_w: f64,
    line_h: f64,
    cols: u32,
    rows: u32,
) -> (u32, u32) {
    let zoom = zoom.max(0.05);
    let gx = (cam_x + sx / zoom) - node_x - pad_x;
    let gy = (cam_y + sy / zoom) - node_y - header_h;
    let col = (gx / cell_w.max(1.0)).floor().max(0.0) as u32;
    let row = (gy / line_h.max(1.0)).floor().max(0.0) as u32;
    (
        col.min(cols.saturating_sub(1)),
        row.min(rows.saturating_sub(1)),
    )
}

/// Ctrl+key -> ASCII control code (Ctrl+C = 0x03). Pure: unit-tested.
fn control_code(ch: char) -> Option<u8> {
    if ch.is_ascii_alphabetic() {
        Some(ch.to_ascii_uppercase() as u8 - b'A' + 1)
    } else {
        None
    }
}

/// Named keys -> terminal control sequences. Pure: unit-tested.
fn named_key_sequence(key: &winit::keyboard::Key) -> Option<Vec<u8>> {
    use winit::keyboard::{Key, NamedKey};
    match key {
        Key::Named(NamedKey::Enter) => Some(b"\r".to_vec()),
        Key::Named(NamedKey::Tab) => Some(b"\t".to_vec()),
        Key::Named(NamedKey::Escape) => Some(b"\x1b".to_vec()),
        Key::Named(NamedKey::Backspace) => Some(b"\x7f".to_vec()),
        Key::Named(NamedKey::ArrowUp) => Some(b"\x1b[A".to_vec()),
        Key::Named(NamedKey::ArrowDown) => Some(b"\x1b[B".to_vec()),
        Key::Named(NamedKey::ArrowRight) => Some(b"\x1b[C".to_vec()),
        Key::Named(NamedKey::ArrowLeft) => Some(b"\x1b[D".to_vec()),
        Key::Named(NamedKey::Home) => Some(b"\x1b[H".to_vec()),
        Key::Named(NamedKey::End) => Some(b"\x1b[F".to_vec()),
        _ => None,
    }
}

/// Encode typed text for the PTY, applying the Ctrl mapping.
/// Pure: unit-tested.
fn encode_text_input(text: &str, ctrl: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for ch in text.chars() {
        if ctrl {
            if let Some(code) = control_code(ch) {
                bytes.push(code);
                continue;
            }
        }
        let mut buf = [0u8; 4];
        bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    }
    bytes
}

/// Run the app with a real OpenGL window. Returns an error when a 3.3 core
/// context cannot be created so the caller can degrade to headless mode.
pub fn run_windowed(app: App) -> Result<(), String> {
    let event_loop = EventLoop::builder().build().map_err(|e| e.to_string())?;
    let mut handler = CanvasState::new(app);
    event_loop.run_app(&mut handler).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, NamedKey};

    #[test]
    fn test_fuzzy_score_ranks_boundaries_first() {
        assert!(fuzzy_score("", "anything").is_some());
        assert!(fuzzy_score("xyz", "terminal.new").is_none());
        // Exact subsequence in one word outranks a scattered match.
        let tight = fuzzy_score("pinl", "view.pinlive").unwrap();
        let loose = fuzzy_score("pinl", "hopscotch.inland").unwrap();
        assert!(tight > loose);
        // Case must already be folded by the caller convention: scoring
        // itself is literal.
        assert!(fuzzy_score("PIN", "pin").is_none());
    }

    #[test]
    fn test_control_codes() {
        assert_eq!(control_code('c'), Some(3));
        assert_eq!(control_code('C'), Some(3));
        assert_eq!(control_code('z'), Some(26));
        assert_eq!(control_code('5'), None);
    }

    #[test]
    fn test_named_sequences() {
        assert_eq!(
            named_key_sequence(&Key::Named(NamedKey::Enter)),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            named_key_sequence(&Key::Named(NamedKey::ArrowUp)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            named_key_sequence(&Key::Named(NamedKey::Backspace)),
            Some(b"\x7f".to_vec())
        );
        assert_eq!(named_key_sequence(&Key::Character("a".into())), None);
    }

    #[test]
    fn test_text_encoding() {
        assert_eq!(encode_text_input("hi", false), b"hi");
        assert_eq!(encode_text_input("c", true), vec![3]);
        assert_eq!(encode_text_input("5", true), b"5");
        assert_eq!(encode_text_input("é", false), "é".as_bytes());
    }

    #[test]
    fn test_key_bytes_reach_terminal_grid() {
        // Full input path: encoded key bytes -> VT engine -> grid cell.
        let bytes = encode_text_input("hi", false);
        let mut engine = VtEngine::new(Terminal::new(SurfaceId(99), 24, 80));
        engine.feed(&bytes);
        assert_eq!(engine.term.grid.get(0, 0).unwrap().character, 'h');
        assert_eq!(engine.term.grid.get(0, 1).unwrap().character, 'i');
        let enter = named_key_sequence(&Key::Named(NamedKey::Enter)).unwrap();
        assert_eq!(enter, b"\r");
    }

    #[test]
    fn test_screen_to_cell_clamps_into_grid() {
        // Node at origin, camera at origin zoom 1, 9px cells, 8px pad,
        // 38px header, 80x24 grid.
        let cell = |sx: f64, sy: f64| {
            screen_to_cell(
                sx, sy, 0.0, 0.0, 1.0, 0.0, 0.0, 8.0, 38.0, 9.0, 18.0, 80, 24,
            )
        };
        assert_eq!(cell(8.0, 38.0), (0, 0));
        assert_eq!(cell(17.0, 56.0), (1, 1));
        // Padding/header clamp to the edge cell; far points to the last.
        assert_eq!(cell(0.0, 0.0), (0, 0));
        assert_eq!(cell(5000.0, 5000.0), (79, 23));
        // Camera offset shifts the mapping.
        let shifted = screen_to_cell(
            8.0, 38.0, 90.0, 0.0, 1.0, 0.0, 0.0, 8.0, 38.0, 9.0, 18.0, 80, 24,
        );
        assert_eq!(shifted, (10, 0));
    }

    #[test]
    fn test_close_button_rect_geometry() {
        // 12 screen px at zoom 2 -> 6 world px, top-right corner.
        let (x, y, s, _) = close_button_rect(100.0, 50.0, 400.0, 2.0);
        assert_eq!((x, y, s), (494.0, 50.0, 6.0));
    }

    #[test]
    fn test_wheel_button_direction_and_clamp() {
        use crate::vt::MouseButton::{WheelDown, WheelUp};
        assert_eq!(wheel_button(0.0), None);
        assert_eq!(wheel_button(0.4), None);
        assert_eq!(wheel_button(1.0), Some((WheelUp, 1)));
        assert_eq!(wheel_button(-2.0), Some((WheelDown, 2)));
        assert_eq!(wheel_button(100.0), Some((WheelUp, 8)));
        assert_eq!(wheel_button(-100.0), Some((WheelDown, 8)));
    }

    #[test]
    fn test_menu_button_rect_geometry() {
        let (x, y, s) = menu_button_rect(100.0, 50.0, 2.0);
        assert_eq!((x, y, s), (100.0, 50.0, 6.0));
    }

    #[test]
    fn test_palette_keys_menus_share_command_ids() {
        use std::collections::HashSet;
        let palette: Vec<_> = CanvasState::palette_commands();
        let catalog = crate::command::builtin_commands();
        let palette_ids: HashSet<_> = palette.iter().map(|(id, _)| *id).collect();
        // Palette is the catalog minus the key-only digit-restore helper.
        assert_eq!(palette.len(), catalog.len() - 1);
        for spec in &catalog {
            if spec.id == "camera.bookmarkRestore" {
                assert!(!palette_ids.contains(spec.id));
            } else {
                assert!(palette_ids.contains(spec.id), "palette missing {}", spec.id);
            }
        }
        // Every single-char key binding resolves to a palette command.
        for key in [
            "n", "N", "x", "h", "v", "t", "C", "O", "F", "d", "a", "f", "0", "b", "p", "P", ".",
            ",", "o", "c", "?", "S",
        ] {
            let id = crate::command::command_for_key(key).unwrap();
            assert!(palette_ids.contains(id), "key {key} -> {id} not in palette");
        }
    }

    #[test]
    fn test_save_restore_round_trip_respawns_sessions() {
        use std::collections::HashSet;
        let mut st = CanvasState::new(App::new(crate::config::Config::new()));
        st.spawn_terminal_node(80, 24, (0.0, 0.0)).unwrap();
        st.pin_view(true);
        let path =
            std::env::temp_dir().join(format!("fracterm-test-layout-{}.json", std::process::id()));
        st.save_layout_to(&path).unwrap();
        // Diverge, then restore: one terminal node revives one session.
        st.spawn_terminal_node(80, 24, (900.0, 0.0)).unwrap();
        assert_eq!(st.terms.len(), 2);
        assert_eq!(st.restore_layout_from(&path).unwrap(), 1);
        assert_eq!(st.terms.len(), 1);
        let surfaces: HashSet<_> = st.terms.iter().map(|s| s.surface).collect();
        for node in st.app.state.workspace.all_nodes() {
            if let Some(proj) = &node.projection {
                assert!(surfaces.contains(&proj.source));
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_pin_snapshot_vs_live_modes() {
        let mut st = CanvasState::new(App::new(crate::config::Config::new()));
        st.spawn_terminal_node(80, 24, (0.0, 0.0)).unwrap();
        let before = st.app.state.workspace.all_nodes().len();
        st.pin_view(false);
        st.pin_view(true);
        let nodes = st.app.state.workspace.all_nodes();
        assert_eq!(nodes.len(), before + 2);
        let modes: Vec<(crate::ProjectionMode, bool)> = nodes
            .iter()
            .filter_map(|n| n.projection.as_ref().map(|p| (p.mode, p.selector.follow)))
            .collect();
        assert!(modes.contains(&(crate::ProjectionMode::Snapshot, false)));
        assert!(modes.contains(&(crate::ProjectionMode::Live, true)));
    }

    #[test]
    fn test_spawn_resize_close_bookkeeping() {
        let mut st = CanvasState::new(App::new(crate::config::Config::new()));
        st.spawn_terminal_node(80, 24, (0.0, 0.0)).unwrap();
        assert_eq!(st.terms.len(), 1);
        let node = st.terms[0].node;
        st.selected = Some(node);
        assert_eq!(
            (
                st.terms[0].engine.term.grid.cols,
                st.terms[0].engine.term.grid.rows
            ),
            (80, 24)
        );
        assert!(st.resize_terminal_grid(node, 100, 30));
        assert_eq!(
            (
                st.terms[0].engine.term.grid.cols,
                st.terms[0].engine.term.grid.rows
            ),
            (100, 30)
        );
        let (w, h) = st.terminal_node_size(100, 30);
        let n = st.app.state.workspace.get_node(node).unwrap();
        assert!((n.size.0 - w).abs() < 1e-9 && (n.size.1 - h).abs() < 1e-9);
        assert!(st.step_selected_grid(-1, 0));
        assert_eq!(st.terms[0].engine.term.grid.cols, 99);
        assert!(st.close_selected());
        assert!(st.terms.is_empty());
        assert!(st.app.state.workspace.get_node(node).is_none());
        assert_eq!(st.selected, None);
        assert_eq!(st.focused_term, None);
        assert!(!st.close_selected());
        assert!(!st.resize_terminal_grid(node, 80, 24));
    }
}
