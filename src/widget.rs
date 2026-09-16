//! Widget display list rendering pipeline.
//!
//! This module provides the bridge between plugin widget display lists
//! and the canvas renderer. Widgets produce display lists (DrawCommand)
//! which are then rendered using the canvas RectRenderer and TextRenderer.

use super::*;
use crate::canvas::RectRenderer;
use crate::plugin::{WidgetRenderContext, WidgetRenderer};
use crate::surface::Color;
use crate::text::{Atlas, FontSystem, TextRenderer};
use glow::HasContext;

/// A widget display list - a sequence of draw commands in world space
#[derive(Debug, Clone)]
pub struct WidgetDisplayList {
    pub widget_id: String,
    pub commands: Vec<DrawCommand>,
    pub bounds: (f64, f64, f64, f64), // x, y, w, h
    pub z_index: i32,
}

/// Convert Color to tuple for canvas renderer
fn color_to_tuple(color: Color) -> (f32, f32, f32, f32) {
    (
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    )
}

/// Render a widget display list using the canvas renderer
#[allow(clippy::too_many_arguments)]
pub fn render_widget_display_list(
    gl: &glow::Context,
    rect_renderer: &mut RectRenderer,
    text_renderer: &mut TextRenderer,
    atlas: &mut Atlas,
    fonts: &mut FontSystem,
    font_id: u32,
    display_list: &WidgetDisplayList,
    viewport_size: (f32, f32),
) {
    let (bx, by, bw, bh) = display_list.bounds;
    let scissor_active = std::cell::RefCell::new(false);

    for cmd in &display_list.commands {
        match cmd {
            DrawCommand::Clear { color } => {
                // Widget clear is handled by the widget background rect
                rect_renderer.push_rect(bx, by, bw, bh, color_to_tuple(*color));
            }
            DrawCommand::Rect { x, y, w, h, color } => {
                rect_renderer.push_rect(bx + x, by + y, *w, *h, color_to_tuple(*color));
            }
            DrawCommand::Text {
                x,
                y,
                text,
                scale,
                color,
                align: _,
            } => {
                let tx = bx + x;
                let ty = by + y;

                // Get the font face
                if let Some(_face) = fonts.face_mut(font_id) {
                    unsafe {
                        text_renderer.queue_string(
                            gl,
                            atlas,
                            fonts,
                            font_id,
                            (tx, ty),
                            (0.0, 0.0, 1.0), // camera transform handled by caller
                            *scale as u32,
                            text,
                            color_to_tuple(*color),
                        )
                    };
                }
            }
            DrawCommand::Line {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
            } => {
                // Lines are rendered as thin rects
                let x1w = bx + x1;
                let y1w = by + y1;
                let x2w = bx + x2;
                let y2w = by + y2;
                let dx = x2w - x1w;
                let dy = y2w - y1w;
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.0 {
                    let _angle = dy.atan2(dx);
                    // For simplicity, render as a series of small rects
                    // In a full implementation, this would use a line shader
                    let steps = (len / width.max(1.0)).ceil() as u32;
                    for i in 0..steps {
                        let t = i as f64 / steps.max(1) as f64;
                        let px = x1w + dx * t;
                        let py = y1w + dy * t;
                        rect_renderer.push_rect(px, py, *width, *width, color_to_tuple(*color));
                    }
                }
            }
            DrawCommand::Image {
                x,
                y,
                w,
                h,
                texture_id: _,
            } => {
                // Image rendering would require texture binding
                // Placeholder for future implementation
                rect_renderer.push_rect(
                    bx + x,
                    by + y,
                    *w,
                    *h,
                    color_to_tuple(Color::from_hex("#ff00ff")),
                );
            }
            DrawCommand::Scissor { x, y, w, h } => {
                unsafe {
                    gl.enable(glow::SCISSOR_TEST);
                    let vx = (bx + x) as f32;
                    let vy = (by + y + h) as f32;
                    let vw = *w as f32;
                    let vh = *h as f32;
                    gl.scissor(
                        vx as i32,
                        (viewport_size.1 - vy) as i32,
                        vw as i32,
                        vh as i32,
                    );
                }
                *scissor_active.borrow_mut() = true;
            }
            DrawCommand::ScissorEnd => {
                if *scissor_active.borrow() {
                    unsafe {
                        gl.disable(glow::SCISSOR_TEST);
                    }
                    *scissor_active.borrow_mut() = false;
                }
            }
        }
    }
}

/// Collect all widget display lists from the workspace
pub fn collect_widget_display_lists(
    workspace: &Workspace,
    widget_renderers: &HashMap<String, Box<dyn WidgetRenderer>>,
    theme: &Theme,
    scale: f64,
    camera_x: f64,
    camera_y: f64,
    camera_zoom: f64,
) -> Vec<WidgetDisplayList> {
    let mut lists = Vec::new();

    for node in workspace.all_nodes() {
        let plugin_behavior = node.plugin();
        if plugin_behavior.plugin_managed {
            if let Some(widget_id) = &plugin_behavior.widget_id {
                if let Some(renderer) = widget_renderers.get(widget_id) {
                    let mut ui = UiBuilder::new();
                    let ctx = WidgetRenderContext {
                        widget_id: widget_id.clone(),
                        bounds: (node.transform.x, node.transform.y, node.size.0, node.size.1),
                        theme,
                        scale,
                        camera_x,
                        camera_y,
                        camera_zoom,
                    };
                    renderer.render(&ctx, &mut ui);
                    let commands = ui.take_commands();
                    lists.push(WidgetDisplayList {
                        widget_id: widget_id.clone(),
                        commands,
                        bounds: ctx.bounds,
                        z_index: node.transform.x as i32, // Use x as rough z-order
                    });
                }
            }
        }
    }

    // Sort by z-index for proper layering
    lists.sort_by_key(|l| l.z_index);
    lists
}

/// Widget registry for managing widget renderers
pub struct WidgetRegistry {
    renderers: HashMap<String, Box<dyn WidgetRenderer>>,
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self {
            renderers: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: String, renderer: Box<dyn WidgetRenderer>) {
        self.renderers.insert(id, renderer);
    }

    pub fn unregister(&mut self, id: &str) -> Option<Box<dyn WidgetRenderer>> {
        self.renderers.remove(id)
    }

    pub fn get(&self, id: &str) -> Option<&dyn WidgetRenderer> {
        self.renderers.get(id).map(|b| &**b)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_all(
        &self,
        gl: &glow::Context,
        rect_renderer: &mut RectRenderer,
        text_renderer: &mut TextRenderer,
        atlas: &mut Atlas,
        fonts: &mut FontSystem,
        font_id: u32,
        workspace: &Workspace,
        theme: &Theme,
        scale: f64,
        camera_x: f64,
        camera_y: f64,
        camera_zoom: f64,
        viewport_size: (f32, f32),
    ) {
        let lists = collect_widget_display_lists(
            workspace,
            &self.renderers,
            theme,
            scale,
            camera_x,
            camera_y,
            camera_zoom,
        );

        for list in lists {
            render_widget_display_list(
                gl,
                rect_renderer,
                text_renderer,
                atlas,
                fonts,
                font_id,
                &list,
                viewport_size,
            );
        }
    }
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// One declarative widget timer: `run` mutates state at most once per
/// `every_ms` (driven by `LiveWidget::tick`, which the frame loop calls).
pub struct WidgetTimer<S> {
    pub every_ms: u64,
    pub run: WidgetTimerFn<S>,
}

/// State constructor for a declarative widget.
pub type WidgetInit<S> = Box<dyn Fn() -> S + Send + Sync>;
/// Timer body for a declarative widget: mutates state.
pub type WidgetTimerFn<S> = Box<dyn Fn(&mut S) + Send + Sync>;
/// View function for a declarative widget: emits display-list commands.
pub type WidgetViewFn<S> = Box<dyn Fn(&S, &mut UiBuilder, &WidgetRenderContext) + Send + Sync>;

/// Declarative widget definition mirroring `defineWidget` in
/// `fracterm/widget`: a state constructor, interval timers, and a view
/// function emitting display-list commands. Widgets never touch OpenGL.
pub struct WidgetDefinition<S> {
    pub id: String,
    pub title: String,
    pub default_size: (u32, u32),
    pub init: WidgetInit<S>,
    pub timers: Vec<WidgetTimer<S>>,
    pub view: WidgetViewFn<S>,
}

/// Live declarative widget: owns state, fires due timers on `tick`, and
/// renders through `WidgetRenderer` as display lists.
pub struct LiveWidget<S> {
    def: WidgetDefinition<S>,
    state: S,
    last_tick_ms: Vec<u64>,
}

impl<S> LiveWidget<S> {
    pub fn new(def: WidgetDefinition<S>) -> Self {
        let state = (def.init)();
        let last_tick_ms = vec![0; def.timers.len()];
        Self {
            def,
            state,
            last_tick_ms,
        }
    }

    /// Fire timers due at `now_ms` (monotonic). Each timer fires at most
    /// once per tick; clock jumps backwards never fire.
    pub fn tick(&mut self, now_ms: u64) {
        for (i, timer) in self.def.timers.iter().enumerate() {
            let every = timer.every_ms.max(1);
            if now_ms.saturating_sub(self.last_tick_ms[i]) >= every {
                (timer.run)(&mut self.state);
                self.last_tick_ms[i] = now_ms;
            }
        }
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    pub fn render_into(&self, ctx: &WidgetRenderContext, ui: &mut UiBuilder) {
        (self.def.view)(&self.state, ui, ctx);
    }
}

impl<S: Send + Sync> WidgetRenderer for LiveWidget<S> {
    fn render(&self, ctx: &WidgetRenderContext, ui: &mut UiBuilder) {
        self.render_into(ctx, ui);
    }

    fn handle_event(&self, _event: &WidgetEvent) -> bool {
        false
    }

    fn size(&self) -> (u32, u32) {
        self.def.default_size
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_display_list() {
        let list = WidgetDisplayList {
            widget_id: "test".to_string(),
            commands: vec![DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
                color: Color::from_hex("#ff0000"),
            }],
            bounds: (0.0, 0.0, 100.0, 100.0),
            z_index: 0,
        };
        assert_eq!(list.widget_id, "test");
        assert_eq!(list.commands.len(), 1);
    }

    #[test]
    fn test_widget_registry() {
        let registry = WidgetRegistry::new();
        // Can't easily test without a real renderer implementation
        assert!(registry.get("nonexistent").is_none());
    }

    fn counter_widget() -> LiveWidget<u32> {
        LiveWidget::new(WidgetDefinition {
            id: "example.counter".to_string(),
            title: "Counter".to_string(),
            default_size: (320, 140),
            init: Box::new(|| 0),
            timers: vec![WidgetTimer {
                every_ms: 1000,
                run: Box::new(|n: &mut u32| *n += 1),
            }],
            view: Box::new(|n: &u32, ui: &mut UiBuilder, _ctx: &WidgetRenderContext| {
                ui.clear(Color::from_hex("#101018"));
                ui.rect(0.0, 0.0, 10.0, 10.0, Color::from_hex("#e8e8f0"));
                let _ = n;
            }),
        })
    }

    #[test]
    fn test_declarative_widget_timers_fire_on_interval() {
        let mut w = counter_widget();
        assert_eq!(*w.state(), 0);
        w.tick(0);
        w.tick(999);
        assert_eq!(*w.state(), 0);
        w.tick(1000);
        assert_eq!(*w.state(), 1);
        w.tick(1500);
        assert_eq!(*w.state(), 1);
        w.tick(2000);
        assert_eq!(*w.state(), 2);
    }

    #[test]
    fn test_declarative_widget_renderer_contract() {
        let w = counter_widget();
        assert_eq!(w.size(), (320, 140));
        assert!(!w.handle_event(&WidgetEvent::FocusGained));
        let theme = Theme::default();
        let ctx = WidgetRenderContext {
            widget_id: "example.counter".to_string(),
            bounds: (0.0, 0.0, 320.0, 140.0),
            theme: &theme,
            scale: 1.0,
            camera_x: 0.0,
            camera_y: 0.0,
            camera_zoom: 1.0,
        };
        let mut ui = UiBuilder::new();
        w.render(&ctx, &mut ui);
        assert_eq!(ui.take_commands().len(), 2);
    }

    #[test]
    fn test_registry_collects_declarative_widget() {
        let mut ws = Workspace::new();
        let mut node = Node::new(NodeId(3), 10.0, 20.0);
        node.size = (320.0, 140.0);
        node.plugin = PluginBehavior::with_widget("example.plugin", "example.counter");
        ws.add_node(node);
        let mut renderers: HashMap<String, Box<dyn WidgetRenderer>> = HashMap::new();
        renderers.insert("example.counter".to_string(), Box::new(counter_widget()));
        let lists =
            collect_widget_display_lists(&ws, &renderers, &Theme::default(), 1.0, 0.0, 0.0, 1.0);
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].widget_id, "example.counter");
        assert_eq!(lists[0].commands.len(), 2);
    }
}
