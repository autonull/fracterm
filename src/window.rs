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
    /// Live terminal session: VT engine + PTY (P3).
    term: Option<(VtEngine, PtySession)>,
    /// Whether the terminal has keyboard focus.
    term_focus: bool,
    /// Currently held keyboard modifiers.
    modifiers: winit::event::Modifiers,
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
            term: None,
            term_focus: true,
            modifiers: winit::event::Modifiers::default(),
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

        // Drain PTY output into the VT engine and sync live projections.
        if let Some((engine, pty)) = &mut self.term {
            let bytes = pty.take_output();
            if !bytes.is_empty() {
                engine.feed(&bytes);
            }
            for node in self.app.state.workspace.scene.all_nodes_mut() {
                if let Some(proj) = &mut node.projection {
                    if proj.source == engine.term.id {
                        proj.update_from_terminal(&engine.term);
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
                    if let Some(face) = fonts.face_mut(font_id) {
                        let (vw, vh) = (self.viewport_size.0, self.viewport_size.1);
                        let _ = (vw, vh);
                        let fg = (0.87, 0.89, 0.93, 1.0);
                        for node in self.app.state.workspace.all_nodes() {
                            let label = format!("node {}", node.id.0);
                            text.queue_string(
                                gl,
                                atlas,
                                face,
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
                        let node_x = self
                            .app
                            .state
                            .workspace
                            .all_nodes()
                            .first()
                            .map(|n| n.transform.x as f64)
                            .unwrap_or(0.0);
                        let node_y = self
                            .app
                            .state
                            .workspace
                            .all_nodes()
                            .first()
                            .map(|n| n.transform.y as f64)
                            .unwrap_or(0.0);
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
                                    face,
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
                        // Terminal grid rows: one glyph per non-blank cell.
                        if let Some((engine, _pty)) = &self.term {
                            let line_h = 16.0f64;
                            let cell_w = 8.0f64;
                            let (rows, cols) = (engine.term.grid.rows, engine.term.grid.cols);
                            for r in 0..rows {
                                for c in 0..cols {
                                    let Some(cell) = engine.term.grid.get(r, c) else {
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
                                        face,
                                        (
                                            node_x + cell_w * c as f64,
                                            node_y + 28.0 + line_h * r as f64,
                                        ),
                                        (cam.x, cam.y, cam.zoom),
                                        13,
                                        &ch,
                                        color,
                                    );
                                }
                            }
                        }
                        text.flush(gl, atlas, self.viewport_size);
                    }
                }
            }
        }
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
            self.font_id = fs.load_family("monospace").ok();
        }
        self.fonts = fonts;
        self.atlas = Some(unsafe { Atlas::new(&gl, 1024) });
        self.text =
            Some(unsafe { TextRenderer::new(&gl) }.expect("failed to create text renderer"));

        // Terminal session (P3): bash in a PTY, 80x24 grid.
        match PtySession::spawn(80, 24, None) {
            Ok(pty) => {
                self.term = Some((VtEngine::new(Terminal::new(SurfaceId(1), 24, 80)), pty));
                let mut node = crate::Node::new(crate::NodeId(1), 60, 60);
                node.size = (640, 400);
                node.set_surface(SurfaceId(1));
                node.set_input(crate::InputBehavior::new("terminal").with_key_focus());
                self.app.state.workspace.add_node(node);

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
                    SurfaceId(1),
                    selector,
                    crate::ProjectionMode::Live,
                );
                let mut pnode = crate::Node::new(crate::NodeId(2), 600, 60);
                pnode.set_surface(SurfaceId(1));
                pnode.projection = Some(proj);
                self.app.state.workspace.add_node(pnode);
            }
            Err(e) => eprintln!("failed to spawn PTY: {e}"),
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
                    // Propagate terminal grid resize to the PTY.
                    if let Some((engine, pty)) = &mut self.term {
                        let cols = (size.width as f64 / 8.0).floor().max(2.0) as u16;
                        let rows = (size.height as f64 / 16.0).floor().max(2.0) as u16;
                        engine.term.grid.resize(rows as u32, cols as u32);
                        engine.term.cursor_row =
                            engine.term.cursor_row.min(rows.saturating_sub(1) as u32);
                        engine.term.cursor_col =
                            engine.term.cursor_col.min(cols.saturating_sub(1) as u32);
                        let _ = pty.resize(cols, rows);
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
                    // Pan on empty canvas; Alt-drag over a node moves the node (later).
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
                let modifiers = self.modifiers;
                if !self.term_focus {
                    return;
                }
                use winit::keyboard::{Key, NamedKey};
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
                        _ => {}
                    }
                }
                let Some(text) = &event.text else {
                    // Named keys map to control sequences.
                    let seq: Option<Vec<u8>> = match &event.logical_key {
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
                    };
                    if let Some(bytes) = seq {
                        if let Some((_, pty)) = &self.term {
                            let _ = pty.write(&bytes);
                        }
                    }
                    return;
                };
                if let Some((_, pty)) = &self.term {
                    let mut bytes = Vec::new();
                    for ch in text.chars() {
                        // Ctrl+key -> control code
                        if modifiers.state().control_key() && ch.is_ascii_alphabetic() {
                            bytes.push((ch.to_ascii_uppercase() as u8) - b'A' + 1);
                        } else {
                            let mut buf = [0u8; 4];
                            bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                    }
                    let _ = pty.write(&bytes);
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

/// Run the app with a real OpenGL window. Returns an error when a 3.3 core
/// context cannot be created so the caller can degrade to headless mode.
pub fn run_windowed(app: App) -> Result<(), String> {
    let event_loop = EventLoop::builder().build().map_err(|e| e.to_string())?;
    let mut handler = CanvasState::new(app);
    event_loop.run_app(&mut handler).map_err(|e| e.to_string())
}
