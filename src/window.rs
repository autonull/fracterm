//! Windowed canvas runtime: winit event loop + glutin OpenGL context,
//! camera pan/zoom input, resize and HiDPI handling (README M1).

use std::num::NonZeroU32;

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
const VIEW_NODE_W: f64 = 360.0;

/// One live terminal: VT engine + PTY bound to a workspace node.
struct TermSession {
    surface: SurfaceId,
    node: crate::NodeId,
    engine: VtEngine,
    pty: PtySession,
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
    /// Last frame instant, for camera easing dt.
    last_frame: std::time::Instant,
    capabilities: Option<Capabilities>,
    window: Option<Window>,
    /// Start of a pan drag, in screen coordinates
    pan_anchor: Option<(f64, f64)>,
    last_cursor: (f64, f64),
    /// Current physical viewport size
    viewport_size: (f32, f32),
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
            last_frame: std::time::Instant::now(),
            capabilities: None,
            window: None,
            pan_anchor: None,
            last_cursor: (0.0, 0.0),
            viewport_size: (1280.0, 720.0),
        }
    }

    fn zoom_at_cursor(&mut self, delta: f64) {
        let cam = self.app.state.workspace.camera_mut();
        let old = cam.zoom;
        let new = (old * delta).clamp(0.05, 64.0);
        if (new - old).abs() < f64::EPSILON {
            return;
        }
        // Keep the world point under the cursor stationary.
        let (wx, wy) = self.last_cursor;
        cam.x += wx / old - wx / new;
        cam.y += wy / old - wy / new;
        cam.zoom = new;
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
                x: x0 as i32,
                y: y0 as i32,
                width: (x1 - x0).max(1.0) as u32,
                height: (y1 - y0).max(1.0) as u32,
            };
            let (vw, vh) = self.viewport_size;
            self.app
                .state
                .workspace
                .camera_mut()
                .fit_rect(rect, vw as f64, vh as f64);
        } else if ctrl {
            // Context menu stub: report the hit target.
            let (wx, wy) = (cam.x + sx / cam.zoom, cam.y + sy / cam.zoom);
            if let Some(node) = self.hit_node(wx, wy) {
                eprintln!("context menu target: node {}", node.id.0);
            } else {
                eprintln!("context menu target: workspace");
            }
        } else {
            // Autozoom to the object under the cursor.
            let (wx, wy) = (cam.x + sx / cam.zoom, cam.y + sy / cam.zoom);
            let hit = self.hit_node(wx, wy).map(|n| (n.id, n.transform, n.size));
            if let Some((nid, t, size)) = hit {
                let (vw, vh) = self.viewport_size;
                self.app
                    .state
                    .workspace
                    .camera_mut()
                    .zoom_to_node(nid, t, size, vw as f64, vh as f64);
            }
        }
    }

    /// Topmost node whose bounds contain the world point.
    fn hit_node(&self, wx: f64, wy: f64) -> Option<&crate::Node> {
        let ids: Vec<_> = self.app.state.workspace.nodes().to_vec();
        for id in ids.iter().rev() {
            if let Some(node) = self.app.state.workspace.get_node(*id) {
                let (w, h) = node.size;
                let w = w.max(1) as f64;
                let h = h.max(1) as f64;
                let (nx, ny) = (node.transform.x as f64, node.transform.y as f64);
                if wx >= nx && wx <= nx + w && wy >= ny && wy <= ny + h {
                    return Some(node);
                }
            }
        }
        None
    }

    /// Apply an arrange function's placements to the workspace nodes.
    fn apply_arrange(
        &mut self,
        f: impl Fn(&[&crate::Node], usize) -> Vec<(crate::NodeId, i32, i32)>,
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

    /// Pin the current viewport as a snapshot projection node ("Pin as View").
    fn pin_current_view(&mut self) {
        let cam = self.app.state.workspace.camera().clone();
        let (vw, vh) = self.viewport_size;
        let rect = crate::lens::Rect {
            x: cam.x as i32,
            y: cam.y as i32,
            width: (vw as f64 / cam.zoom) as u32,
            height: (vh as f64 / cam.zoom) as u32,
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
            max_lines: Some(24),
            follow: false,
        };
        let proj = crate::ProjectionSurface::new(
            crate::SurfaceId(1),
            selector,
            crate::ProjectionMode::Snapshot,
        );
        let mut node = crate::Node::new(next_id, rect.x + rect.width as i32 + 40, rect.y);
        node.set_surface(crate::SurfaceId(1));
        node.projection = Some(proj);
        node.transform.scale = 1.0;
        let _ = rect;
        self.app.state.workspace.add_node(node);
        eprintln!("pinned view as node {next_id:?}");
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

    /// Fit every node into the viewport with a margin, instantly (no
    /// animation): the app starts aligned and ready, never mid-flight.
    fn fit_dashboard(&mut self) {
        let nodes = self.app.state.workspace.all_nodes();
        if nodes.is_empty() {
            return;
        }
        let mut x0 = f64::INFINITY;
        let mut y0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut y1 = f64::NEG_INFINITY;
        for n in nodes {
            x0 = x0.min(n.transform.x as f64);
            y0 = y0.min(n.transform.y as f64);
            x1 = x1.max(n.transform.x as f64 + n.size.0.max(1) as f64);
            y1 = y1.max(n.transform.y as f64 + n.size.1.max(1) as f64);
        }
        let (vw, vh) = (self.viewport_size.0 as f64, self.viewport_size.1 as f64);
        let bw = (x1 - x0 + 2.0 * DASH_MARGIN).max(1.0);
        let bh = (y1 - y0 + 2.0 * DASH_MARGIN).max(1.0);
        let zoom = (vw / bw).min(vh / bh).min(1.0).clamp(0.05, 64.0);
        self.snap_camera(x0 - DASH_MARGIN, y0 - DASH_MARGIN, zoom);
    }

    /// Spawn a terminal node at `pos` and register its PTY session.
    fn spawn_terminal_node(&mut self, cols: u32, rows: u32, pos: (i32, i32)) -> Result<(), String> {
        let pty = PtySession::spawn(cols as u16, rows as u16, None)?;
        let surface = SurfaceId(self.next_surface_id);
        self.next_surface_id += 1;
        let node_id = crate::NodeId(self.next_node_id);
        self.next_node_id += 1;
        let engine = VtEngine::new(Terminal::new(surface, rows, cols));
        let (cell_w, line_h) = self.grid_cell;
        let w = (cols as f64 * cell_w + 2.0 * GRID_PAD_X).round() as i32;
        let h = (self.header_h() + rows as f64 * line_h + GRID_PAD_BOTTOM).round() as i32;
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

    fn draw_frame(&mut self) {
        let (Some(surface), Some(context), Some(gl)) =
            (&self.gl_surface, &self.gl_context, &self.gl)
        else {
            return;
        };
        // Camera easing toward its animated target.
        let dt = self.last_frame.elapsed().as_secs_f64().max(0.001);
        self.last_frame = std::time::Instant::now();
        self.app
            .state
            .workspace
            .camera_mut()
            .update(dt.min(0.1) * 10.0);

        // Drain PTY output into each VT engine and sync live projections.
        for sess in &mut self.terms {
            let bytes = sess.pty.take_output();
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
            }
            // Answer terminal queries (DA etc.): the child may hold all
            // output until we reply.
            let reply = sess.engine.take_reply();
            if !reply.is_empty() {
                let _ = sess.pty.write(&reply);
            }
        }
        for sess in &self.terms {
            for node in self.app.state.workspace.scene.all_nodes_mut() {
                if let Some(proj) = &mut node.projection {
                    if proj.source == sess.surface {
                        proj.update_from_terminal(&sess.engine.term);
                    }
                }
            }
        }

        if let (Some(renderer), Some(graph)) = (&mut self.renderer, &self.graph) {
            let cam = self.app.state.workspace.camera();
            renderer.set_camera(cam.x, cam.y, cam.zoom);

            // ContentPass: one rect per node, batched into a single draw call.
            for node in self.app.state.workspace.all_nodes() {
                let bg = node.style.background;
                let (w, h) = node.size;
                let width = w.max(1) as f64;
                let height = h.max(1) as f64;
                renderer.push_rect(
                    node.transform.x as f64,
                    node.transform.y as f64,
                    width,
                    height,
                    (
                        bg.r as f32 / 255.0,
                        bg.g as f32 / 255.0,
                        bg.b as f32 / 255.0,
                        bg.a as f32 / 255.0,
                    ),
                );
                let border = node.style.border;
                renderer.push_border(
                    node.transform.x as f64,
                    node.transform.y as f64,
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
                        let fg = (0.87, 0.89, 0.93, 1.0);
                        for node in self.app.state.workspace.all_nodes() {
                            let label = format!("node {}", node.id.0);
                            text.queue_string(
                                gl,
                                atlas,
                                fonts,
                                fid,
                                (
                                    node.transform.x as f64 + 8.0,
                                    node.transform.y as f64 + 20.0,
                                ),
                                (cam.x, cam.y, cam.zoom),
                                14,
                                &label,
                                fg,
                            );
                        }
                        // Terminal grids: one glyph per non-blank cell, set in
                        // measured cells under each session's node.
                        let (cell_w, line_h) = self.grid_cell;
                        let header_h = line_h + 20.0;
                        for i in 0..self.terms.len() {
                            let (nx, ny) = self
                                .app
                                .state
                                .workspace
                                .get_node(self.terms[i].node)
                                .map(|n| (n.transform.x as f64, n.transform.y as f64))
                                .unwrap_or((0.0, 0.0));
                            let rows = self.terms[i].engine.term.grid.rows;
                            let cols = self.terms[i].engine.term.grid.cols;
                            for r in 0..rows {
                                for c in 0..cols {
                                    let Some(cell) = self.terms[i].engine.term.grid.get(r, c)
                                    else {
                                        continue;
                                    };
                                    if cell.character == ' ' || cell.width == 0 {
                                        continue;
                                    }
                                    let ch = cell.character.to_string();
                                    let color = (
                                        cell.fg.r as f32 / 255.0,
                                        cell.fg.g as f32 / 255.0,
                                        cell.fg.b as f32 / 255.0,
                                        1.0,
                                    );
                                    text.queue_string(
                                        gl,
                                        atlas,
                                        fonts,
                                        fid,
                                        crate::arrange::grid_cell_origin(
                                            nx, ny, GRID_PAD_X, header_h, cell_w, line_h, c, r,
                                        ),
                                        (cam.x, cam.y, cam.zoom),
                                        GRID_PX,
                                        &ch,
                                        color,
                                    );
                                }
                            }
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
                                        node.transform.x as f64 + 8.0,
                                        node.transform.y as f64 + 20.0 + 16.0 * li as f64,
                                    ),
                                    (cam.x, cam.y, cam.zoom),
                                    12,
                                    line,
                                    (0.55, 0.85, 0.65, 1.0),
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
        let term_w = GRID_COLS as f64 * cell_w + 2.0 * GRID_PAD_X;
        let term_h = line_h + 12.0 + GRID_ROWS as f64 * line_h + GRID_PAD_BOTTOM;
        let ((tx, ty), (vx, vy)) = crate::arrange::dashboard_layout(
            width as f64,
            term_w,
            term_h,
            VIEW_NODE_W,
            DASH_MARGIN,
            DASH_GAP,
        );
        // Terminal session (P3): bash in a PTY, 80x24 grid.
        match self.spawn_terminal_node(GRID_COLS, GRID_ROWS, (tx, ty)) {
            Ok(()) => {
                let source = self.terms.last().map(|s| s.surface).unwrap_or(SurfaceId(1));
                // Demo projection: live last-20-lines view of the terminal (P4).
                let selector = crate::ProjectionSelector {
                    rows: None,
                    columns: None,
                    filter: None,
                    search: None,
                    max_lines: Some(20),
                    follow: true,
                };
                let proj = crate::ProjectionSurface::new(
                    source,
                    selector,
                    crate::ProjectionMode::Live,
                );
                let pid = crate::NodeId(self.next_node_id);
                self.next_node_id += 1;
                let mut pnode = crate::Node::new(pid, vx, vy);
                pnode.size = (VIEW_NODE_W as i32, term_h as i32);
                pnode.set_surface(source);
                pnode.projection = Some(proj);
                self.app.state.workspace.add_node(pnode);
            }
            Err(e) => eprintln!("failed to spawn PTY: {e}"),
        }
        self.viewport_size = (width as f32, height as f32);
        self.fit_dashboard();
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
                    let (cell_w, line_h) = self.grid_cell;
                    let header_h = self.header_h();
                    for i in 0..self.terms.len() {
                        let node_id = self.terms[i].node;
                        let (nw, nh) = self
                            .app
                            .state
                            .workspace
                            .get_node(node_id)
                            .map(|n| (n.size.0.max(1) as f64, n.size.1.max(1) as f64))
                            .unwrap_or((640.0, 400.0));
                        let cols = ((nw - 2.0 * GRID_PAD_X) / cell_w)
                            .floor()
                            .clamp(2.0, 256.0) as u16;
                        let rows = ((nh - header_h - GRID_PAD_BOTTOM) / line_h)
                            .floor()
                            .clamp(2.0, 256.0) as u16;
                        let sess = &mut self.terms[i];
                        sess.engine.term.grid.resize(rows as u32, cols as u32);
                        sess.engine.term.cursor_row = sess
                            .engine
                            .term
                            .cursor_row
                            .min(rows.saturating_sub(1) as u32);
                        sess.engine.term.cursor_col = sess
                            .engine
                            .term
                            .cursor_col
                            .min(cols.saturating_sub(1) as u32);
                        let _ = sess.pty.resize(cols, rows);
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
                let zoom = self.app.state.workspace.camera().zoom;
                if let Some((ax, ay)) = self.pan_anchor {
                    let cam = self.app.state.workspace.camera_mut();
                    cam.x -= (new_cursor.0 - ax) / zoom;
                    cam.y -= (new_cursor.1 - ay) / zoom;
                    self.pan_anchor = Some(new_cursor);
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
                (ElementState::Pressed, MouseButton::Left)
                | (ElementState::Pressed, MouseButton::Middle) => {
                    // Click focuses the terminal under the cursor; clicking
                    // empty canvas focuses the workspace. Pan still applies.
                    let cam = self.app.state.workspace.camera().clone();
                    let (wx, wy) = (
                        cam.x + self.last_cursor.0 / cam.zoom,
                        cam.y + self.last_cursor.1 / cam.zoom,
                    );
                    let hit = self.hit_node(wx, wy).map(|n| {
                        (
                            n.id,
                            n.surface_id(),
                            n.input.mode.clone(),
                            n.input.focus_key,
                        )
                    });
                    match hit {
                        Some((_, Some(surface), mode, _))
                            if mode == "terminal"
                                && self.terms.iter().any(|s| s.surface == surface) =>
                        {
                            self.focused_term = Some(surface);
                            self.term_focus = true;
                        }
                        _ => {
                            self.term_focus = false;
                        }
                    }
                    self.pan_anchor = Some(self.last_cursor);
                }
                (ElementState::Released, MouseButton::Left)
                | (ElementState::Released, MouseButton::Middle) => {
                    self.pan_anchor = None;
                }
                _ => {}
            },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: _,
                ..
            } => {
                // Releases carry no text and must never write to the PTY
                // (otherwise Enter/Backspace fire twice: press + release).
                if event.state != ElementState::Pressed {
                    return;
                }
                let modifiers = self.modifiers;
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
                            self.pin_current_view();
                            return;
                        }
                        Key::Character(c) if c == "t" => {
                            self.apply_arrange(
                                |ns, _| {
                                    crate::arrange::tile_grid(ns, 3, crate::arrange::DEFAULT_GAP)
                                },
                                0,
                            );
                            return;
                        }
                        Key::Character(c) if c == "h" => {
                            self.apply_arrange(
                                |ns, _| {
                                    crate::arrange::tile_horizontally(
                                        ns,
                                        crate::arrange::DEFAULT_GAP,
                                    )
                                },
                                0,
                            );
                            return;
                        }
                        Key::Character(c) if c == "v" => {
                            self.apply_arrange(
                                |ns, _| {
                                    crate::arrange::tile_vertically(ns, crate::arrange::DEFAULT_GAP)
                                },
                                0,
                            );
                            return;
                        }
                        Key::Character(c) if c == "a" => {
                            self.apply_arrange(
                                |ns, _| crate::arrange::align(ns, crate::arrange::Edge::Left),
                                0,
                            );
                            return;
                        }
                        Key::Character(c) if c == "b" => {
                            self.app.state.workspace.camera_mut().save_bookmark("bm");
                            eprintln!("bookmark saved");
                            return;
                        }
                        Key::Character(c)
                            if c.len() == 1 && c.chars().next().unwrap().is_ascii_digit() =>
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
                            return;
                        }
                        Key::Character(c) if c == "d" => {
                            // Dashboard: anchor at the margin, tile 2-up, fit.
                            if let Some(first) =
                                self.app.state.workspace.all_nodes().first().map(|n| n.id)
                            {
                                if let Some(n) = self.app.state.workspace.get_node_mut(first) {
                                    n.transform.x = DASH_MARGIN as i32;
                                    n.transform.y = DASH_MARGIN as i32;
                                }
                            }
                            self.apply_arrange(
                                |ns, _| {
                                    crate::arrange::tile_grid(ns, 2, crate::arrange::DEFAULT_GAP)
                                },
                                0,
                            );
                            self.fit_dashboard();
                            return;
                        }
                        Key::Character(c) if c == "f" => {
                            self.fit_dashboard();
                            return;
                        }
                        Key::Character(c) if c == "n" => {
                            // New terminal view on the dashboard (up to 8).
                            if self.terms.len() < 8 {
                                let k = self.terms.len() as i32;
                                let anchor = self
                                    .app
                                    .state
                                    .workspace
                                    .all_nodes()
                                    .first()
                                    .map(|n| (n.transform.x, n.transform.y))
                                    .unwrap_or((DASH_MARGIN as i32, DASH_MARGIN as i32));
                                let pos = (anchor.0 + k * 48, anchor.1 + k * 48);
                                if let Err(e) =
                                    self.spawn_terminal_node(GRID_COLS, GRID_ROWS, pos)
                                {
                                    eprintln!("failed to spawn terminal: {e}");
                                }
                            } else {
                                eprintln!("terminal limit reached");
                            }
                            return;
                        }
                        Key::Character(c) if c == "0" => {
                            self.app.state.workspace.camera_mut().zoom_to_workspace_fit();
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.term_focus = true;
                            return;
                        }
                        Key::Character(c) if c == "i" => {
                            self.term_focus = true;
                            return;
                        }
                        _ => {}
                    }
                    return;
                } else {
                    // Terminal input handling.
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
                    let bytes =
                        encode_text_input(text, modifiers.state().control_key());
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
                self.zoom_at_cursor(1.15f64.powf(lines));
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
}
