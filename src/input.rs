//! Input system - interaction modes, keybindings, context menus, and event handling.

use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Interaction mode for input handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum InteractionMode {
    /// Default canvas navigation
    #[default]
    Workspace,
    /// Terminal focused mode
    Terminal,
    /// Reading accessibility mode
    Reading,
    /// Dashboard arrangement mode
    Dashboard,
}

impl InteractionMode {
    pub fn as_str(&self) -> &str {
        match self {
            InteractionMode::Workspace => "workspace",
            InteractionMode::Terminal => "terminal",
            InteractionMode::Reading => "reading",
            InteractionMode::Dashboard => "dashboard",
        }
    }
}

/// Mouse button types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Input event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    KeyDown {
        key: String,
        modifiers: Modifiers,
    },
    KeyUp {
        key: String,
        modifiers: Modifiers,
    },
    MouseMove {
        x: f64,
        y: f64,
    },
    MouseDown {
        x: f64,
        y: f64,
        button: MouseButton,
        modifiers: Modifiers,
    },
    MouseUp {
        x: f64,
        y: f64,
        button: MouseButton,
        modifiers: Modifiers,
    },
    MouseWheel {
        delta_x: f64,
        delta_y: f64,
        modifiers: Modifiers,
    },
    TouchStart {
        id: u64,
        x: f64,
        y: f64,
    },
    TouchMove {
        id: u64,
        x: f64,
        y: f64,
    },
    TouchEnd {
        id: u64,
    },
}

/// Input handler trait for different interaction modes
pub trait InputHandler: Send + Sync {
    fn handle_event(&mut self, event: InputEvent, ctx: &mut InputContext) -> InputResult;
    fn mode(&self) -> InteractionMode;
}

/// Result of input handling
#[derive(Debug, Clone)]
pub enum InputResult {
    /// Event was handled
    Handled,
    /// Event was not handled, pass to next handler
    PassThrough,
    /// Request mode change
    ChangeMode(InteractionMode),
    /// Execute a command
    ExecuteCommand {
        command: String,
        args: HashMap<String, CommandValue>,
    },
}

/// Context for input handlers
pub struct InputContext {
    pub workspace: Arc<Workspace>,
    pub camera: Arc<Mutex<Camera>>,
    pub command_registry: Arc<Mutex<CommandRegistry>>,
    pub event_bus: Arc<Mutex<EventBus>>,
    pub current_mode: InteractionMode,
    pub selected_node: Option<NodeId>,
    pub drag_state: Option<DragState>,
    pub viewport_size: (f32, f32),
    pub hidpi_scale: f64,
}

impl InputContext {
    pub fn new(
        workspace: Arc<Workspace>,
        camera: Arc<Mutex<Camera>>,
        command_registry: Arc<Mutex<CommandRegistry>>,
        event_bus: Arc<Mutex<EventBus>>,
        viewport_size: (f32, f32),
    ) -> Self {
        Self {
            workspace,
            camera,
            command_registry,
            event_bus,
            current_mode: InteractionMode::Workspace,
            selected_node: None,
            drag_state: None,
            viewport_size,
            hidpi_scale: 1.0,
        }
    }
}

/// Drag state for mouse interactions
#[derive(Debug, Clone, PartialEq)]
pub enum DragState {
    None,
    Pan {
        start_x: f64,
        start_y: f64,
    },
    Move {
        node: NodeId,
        offset_x: f64,
        offset_y: f64,
    },
    Resize {
        node: NodeId,
    },
    RectSelect {
        start_x: f64,
        start_y: f64,
    },
}

/// Workspace input handler - default canvas navigation
pub struct WorkspaceInputHandler;

impl InputHandler for WorkspaceInputHandler {
    fn mode(&self) -> InteractionMode {
        InteractionMode::Workspace
    }

    fn handle_event(&mut self, event: InputEvent, ctx: &mut InputContext) -> InputResult {
        match event {
            InputEvent::MouseWheel {
                delta_y, modifiers, ..
            } => {
                if modifiers.ctrl || modifiers.shift {
                    return InputResult::PassThrough;
                }
                let mut camera = ctx.camera.lock().unwrap();
                let factor = 1.15_f64.powf(delta_y / 40.0);
                camera.target_zoom = (camera.target_zoom * factor).clamp(0.05, 64.0);
                camera.animating = true;
                InputResult::Handled
            }
            InputEvent::MouseDown {
                x,
                y,
                button,
                modifiers,
            } => {
                match button {
                    MouseButton::Middle => {
                        ctx.drag_state = Some(DragState::Pan {
                            start_x: x,
                            start_y: y,
                        });
                        InputResult::Handled
                    }
                    MouseButton::Right => {
                        if modifiers.ctrl {
                            // Context menu
                            InputResult::ExecuteCommand {
                                command: "context.menu".to_string(),
                                args: HashMap::from([
                                    ("x".to_string(), CommandValue::Number(x)),
                                    ("y".to_string(), CommandValue::Number(y)),
                                ]),
                            }
                        } else {
                            ctx.drag_state = Some(DragState::RectSelect {
                                start_x: x,
                                start_y: y,
                            });
                            InputResult::Handled
                        }
                    }
                    MouseButton::Left => {
                        // Hit test and select
                        InputResult::Handled
                    }
                }
            }
            InputEvent::MouseUp { button, .. } => {
                if matches!(ctx.drag_state, Some(DragState::RectSelect { .. }))
                    && button == MouseButton::Right
                {
                    // Finish rectangle zoom
                    ctx.drag_state = None;
                    InputResult::ExecuteCommand {
                        command: "camera.zoomToRect".to_string(),
                        args: HashMap::new(),
                    }
                } else {
                    ctx.drag_state = None;
                    InputResult::Handled
                }
            }
            InputEvent::MouseMove { x, y } => {
                if let Some(DragState::Pan { start_x, start_y }) = ctx.drag_state {
                    let dx = (start_x - x) / ctx.hidpi_scale;
                    let dy = (start_y - y) / ctx.hidpi_scale;
                    let mut camera = ctx.camera.lock().unwrap();
                    camera.pan(dx, dy);
                    ctx.drag_state = Some(DragState::Pan {
                        start_x: x,
                        start_y: y,
                    });
                    InputResult::Handled
                } else {
                    InputResult::PassThrough
                }
            }
            InputEvent::KeyDown { key, modifiers } => {
                // Workspace shortcuts
                if !modifiers.ctrl && !modifiers.alt && !modifiers.meta {
                    match key.as_str() {
                        "p" => InputResult::ExecuteCommand {
                            command: "view.pin".to_string(),
                            args: HashMap::new(),
                        },
                        "t" => InputResult::ExecuteCommand {
                            command: "layout.tileGrid".to_string(),
                            args: HashMap::from([("cols".to_string(), CommandValue::Number(3.0))]),
                        },
                        "h" => InputResult::ExecuteCommand {
                            command: "layout.tileHorizontal".to_string(),
                            args: HashMap::new(),
                        },
                        "v" => InputResult::ExecuteCommand {
                            command: "layout.tileVertical".to_string(),
                            args: HashMap::new(),
                        },
                        "a" => InputResult::ExecuteCommand {
                            command: "layout.alignLeft".to_string(),
                            args: HashMap::new(),
                        },
                        "b" => InputResult::ExecuteCommand {
                            command: "camera.saveBookmark".to_string(),
                            args: HashMap::new(),
                        },
                        "f" => InputResult::ExecuteCommand {
                            command: "camera.fitDashboard".to_string(),
                            args: HashMap::new(),
                        },
                        "d" => InputResult::ExecuteCommand {
                            command: "layout.dashboard".to_string(),
                            args: HashMap::new(),
                        },
                        "n" => InputResult::ExecuteCommand {
                            command: "terminal.new".to_string(),
                            args: HashMap::new(),
                        },
                        "0" => InputResult::ExecuteCommand {
                            command: "camera.zoomToFit".to_string(),
                            args: HashMap::new(),
                        },
                        "Enter" | "i" => InputResult::ChangeMode(InteractionMode::Terminal),
                        c if c.len() == 1
                            && matches!(c, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") =>
                        {
                            InputResult::ExecuteCommand {
                                command: "camera.restoreBookmark".to_string(),
                                args: HashMap::from([(
                                    "name".to_string(),
                                    format!("bm{}", c).into(),
                                )]),
                            }
                        }
                        _ => InputResult::PassThrough,
                    }
                } else {
                    InputResult::PassThrough
                }
            }
            _ => InputResult::PassThrough,
        }
    }
}

/// Terminal input handler - focused terminal mode
pub struct TerminalInputHandler;

impl InputHandler for TerminalInputHandler {
    fn mode(&self) -> InteractionMode {
        InteractionMode::Terminal
    }

    fn handle_event(&mut self, event: InputEvent, _ctx: &mut InputContext) -> InputResult {
        match event {
            InputEvent::KeyDown { key, modifiers } => {
                if key == "Escape" {
                    return InputResult::ChangeMode(InteractionMode::Workspace);
                }
                // Forward to terminal
                InputResult::ExecuteCommand {
                    command: "terminal.sendKey".to_string(),
                    args: HashMap::from([
                        ("key".to_string(), key.into()),
                        ("ctrl".to_string(), modifiers.ctrl.into()),
                        ("shift".to_string(), modifiers.shift.into()),
                        ("alt".to_string(), modifiers.alt.into()),
                        ("meta".to_string(), modifiers.meta.into()),
                    ]),
                }
            }
            InputEvent::MouseWheel {
                delta_x: _,
                delta_y,
                modifiers,
            } => {
                if modifiers.shift {
                    // Scroll terminal
                    InputResult::ExecuteCommand {
                        command: "terminal.scroll".to_string(),
                        args: HashMap::from([(
                            "lines".to_string(),
                            CommandValue::Number(-delta_y),
                        )]),
                    }
                } else {
                    InputResult::PassThrough // Let workspace handle zoom
                }
            }
            _ => InputResult::PassThrough,
        }
    }
}

/// Reading mode input handler
pub struct ReadingInputHandler;

impl InputHandler for ReadingInputHandler {
    fn mode(&self) -> InteractionMode {
        InteractionMode::Reading
    }

    fn handle_event(&mut self, event: InputEvent, _ctx: &mut InputContext) -> InputResult {
        match event {
            InputEvent::KeyDown { key, .. } => {
                if key == "Escape" || key == "q" {
                    InputResult::ChangeMode(InteractionMode::Workspace)
                } else {
                    InputResult::PassThrough
                }
            }
            _ => InputResult::PassThrough,
        }
    }
}

/// Dashboard mode input handler
pub struct DashboardInputHandler;

impl InputHandler for DashboardInputHandler {
    fn mode(&self) -> InteractionMode {
        InteractionMode::Dashboard
    }

    fn handle_event(&mut self, event: InputEvent, _ctx: &mut InputContext) -> InputResult {
        match event {
            InputEvent::KeyDown { key, .. } => {
                if key == "Escape" {
                    InputResult::ChangeMode(InteractionMode::Workspace)
                } else {
                    InputResult::PassThrough
                }
            }
            _ => InputResult::PassThrough,
        }
    }
}

/// Input manager - routes events to appropriate handler based on mode
pub struct InputManager {
    handlers: HashMap<InteractionMode, Box<dyn InputHandler>>,
    current_mode: InteractionMode,
}

impl InputManager {
    pub fn new() -> Self {
        let mut handlers: HashMap<InteractionMode, Box<dyn InputHandler>> = HashMap::new();
        handlers.insert(InteractionMode::Workspace, Box::new(WorkspaceInputHandler));
        handlers.insert(InteractionMode::Terminal, Box::new(TerminalInputHandler));
        handlers.insert(InteractionMode::Reading, Box::new(ReadingInputHandler));
        handlers.insert(InteractionMode::Dashboard, Box::new(DashboardInputHandler));

        Self {
            handlers,
            current_mode: InteractionMode::Workspace,
        }
    }

    pub fn handle_event(&mut self, event: InputEvent, ctx: &mut InputContext) -> InputResult {
        // Update current mode from context
        self.current_mode = ctx.current_mode;

        if let Some(handler) = self.handlers.get_mut(&self.current_mode) {
            let result = handler.handle_event(event, ctx);

            // Handle mode changes
            if let InputResult::ChangeMode(new_mode) = &result {
                self.current_mode = *new_mode;
                ctx.current_mode = *new_mode;
            }

            result
        } else {
            InputResult::PassThrough
        }
    }

    pub fn set_mode(&mut self, mode: InteractionMode) {
        self.current_mode = mode;
    }

    pub fn current_mode(&self) -> InteractionMode {
        self.current_mode
    }
}

/// Context menu item kinds for rich options popover
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextMenuItemKind {
    /// Simple command action
    Command { command: String, args: HashMap<String, CommandValue> },
    /// Toggle boolean setting
    Toggle { get: String, set: String, label_on: String, label_off: String },
    /// Slider for numeric value
    Slider { get: String, set: String, min: f64, max: f64, step: f64 },
    /// Color picker
    ColorPicker { get: String, set: String },
    /// Select from options
    Select { get: String, set: String, options: Vec<(String, String)> },
    /// Submenu
    Submenu { items: Vec<ContextMenuItem> },
    /// Separator
    Separator,
    /// Read-only info display
    Info { label: String, value: String },
}

/// Rich context menu item for options popover
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMenuItem {
    pub id: String,
    pub label: String,
    pub kind: ContextMenuItemKind,
    pub enabled: bool,
    pub icon: Option<String>,
}

impl ContextMenuItem {
    pub fn command(id: &str, label: &str, command: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::Command {
                command: command.to_string(),
                args: HashMap::new(),
            },
            enabled: true,
            icon: None,
        }
    }

    pub fn command_with_args(id: &str, label: &str, command: &str, args: HashMap<String, CommandValue>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::Command { command: command.to_string(), args },
            enabled: true,
            icon: None,
        }
    }

    pub fn toggle(id: &str, label: &str, get_cmd: &str, set_cmd: &str, label_on: &str, label_off: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::Toggle {
                get: get_cmd.to_string(),
                set: set_cmd.to_string(),
                label_on: label_on.to_string(),
                label_off: label_off.to_string(),
            },
            enabled: true,
            icon: None,
        }
    }

    pub fn slider(id: &str, label: &str, get_cmd: &str, set_cmd: &str, min: f64, max: f64, step: f64) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::Slider { get: get_cmd.to_string(), set: set_cmd.to_string(), min, max, step },
            enabled: true,
            icon: None,
        }
    }

    pub fn color_picker(id: &str, label: &str, get_cmd: &str, set_cmd: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::ColorPicker { get: get_cmd.to_string(), set: set_cmd.to_string() },
            enabled: true,
            icon: None,
        }
    }

    pub fn select(id: &str, label: &str, get_cmd: &str, set_cmd: &str, options: Vec<(String, String)>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::Select { get: get_cmd.to_string(), set: set_cmd.to_string(), options },
            enabled: true,
            icon: None,
        }
    }

    pub fn submenu(id: &str, label: &str, items: Vec<ContextMenuItem>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::Submenu { items },
            enabled: true,
            icon: None,
        }
    }

    pub fn separator() -> Self {
        Self {
            id: "".to_string(),
            label: "".to_string(),
            kind: ContextMenuItemKind::Separator,
            enabled: false,
            icon: None,
        }
    }

    pub fn info(id: &str, label: &str, value: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind: ContextMenuItemKind::Info { label: label.to_string(), value: value.to_string() },
            enabled: false,
            icon: None,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn with_icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.to_string());
        self
    }
}

/// Context menu builder for different targets - matches spec §12.1
pub struct ContextMenuBuilder;

impl ContextMenuBuilder {
    /// Build options popover for terminal node (spec §12.1)
    pub fn for_terminal_node(_node_id: &str) -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::info("title", "Title", "Terminal"),
            ContextMenuItem::separator(),
            // Profile submenu
            ContextMenuItem::submenu("profile", "Profile", vec![
                ContextMenuItem::command("profile.default", "Default", "profile.default"),
                ContextMenuItem::command("profile.big-text", "Big Text", "profile.big-text"),
                ContextMenuItem::command("profile.ssh", "SSH", "profile.ssh"),
                ContextMenuItem::command("profile.logs", "Logs", "profile.logs"),
                ContextMenuItem::command("profile.presentation", "Presentation", "profile.presentation"),
                ContextMenuItem::command("profile.high-contrast", "High Contrast", "profile.high-contrast"),
            ]),
            // Opacity slider
            ContextMenuItem::slider("opacity", "Opacity", "node.getOpacity", "node.setOpacity", 0.5, 1.0, 0.05),
            // Style/tint selector
            ContextMenuItem::select("tint", "Style", "node.getTint", "node.setTint", vec![
                ("ink".to_string(), "Ink".to_string()),
                ("moss".to_string(), "Moss".to_string()),
                ("indigo".to_string(), "Indigo".to_string()),
                ("maroon".to_string(), "Maroon".to_string()),
            ]),
            // Font scale slider
            ContextMenuItem::slider("font_scale", "Font Scale", "node.getFontScale", "node.setFontScale", 0.7, 2.5, 0.1),
            // Input mode
            ContextMenuItem::select("input_mode", "Input Mode", "node.getInputMode", "node.setInputMode", vec![
                ("terminal".to_string(), "Terminal".to_string()),
                ("workspace".to_string(), "Workspace".to_string()),
            ]),
            ContextMenuItem::separator(),
            // Background blur toggle
            ContextMenuItem::toggle("bg_blur", "Background Blur", "node.getBgBlur", "node.setBgBlur", "On", "Off"),
            // Border color picker
            ContextMenuItem::color_picker("border_color", "Border Color", "node.getBorderColor", "node.setBorderColor"),
            // Always on top toggle
            ContextMenuItem::toggle("always_on_top", "Always on Top", "node.getAlwaysOnTop", "node.setAlwaysOnTop", "On", "Off"),
            ContextMenuItem::separator(),
            // Terminal-specific
            ContextMenuItem::slider("scrollback", "Scrollback (lines)", "terminal.getScrollback", "terminal.setScrollback", 1000.0, 100000.0, 1000.0),
            ContextMenuItem::toggle("reflow", "Reflow Text", "terminal.getReflow", "terminal.setReflow", "On", "Off"),
            ContextMenuItem::info("mouse_mode", "Mouse Mode", "Auto"),
            ContextMenuItem::toggle("copy_on_select", "Copy on Select", "terminal.getCopyOnSelect", "terminal.setCopyOnSelect", "On", "Off"),
            ContextMenuItem::separator(),
            ContextMenuItem::command("save_default", "Save as Default", "node.saveAsDefault"),
            ContextMenuItem::command("close", "Close", "terminal.close"),
        ]
    }

    /// Build options popover for projection node
    pub fn for_projection_node(_node_id: &str) -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::info("title", "Title", "Projection"),
            ContextMenuItem::separator(),
            // Live/Snapshot toggle
            ContextMenuItem::toggle("live_mode", "Live View", "projection.getLive", "projection.setLive", "Live", "Snapshot"),
            // Follow source
            ContextMenuItem::toggle("follow", "Follow Source", "projection.getFollow", "projection.setFollow", "On", "Off"),
            // Edit filter
            ContextMenuItem::command("edit_filter", "Edit Filter", "projection.editFilter"),
            // Detach
            ContextMenuItem::command("detach", "Detach", "projection.detach"),
            ContextMenuItem::separator(),
            // Reading mode
            ContextMenuItem::command("reading", "Reading Mode", "reading.enter"),
            // Opacity/style/font scale (shared with terminals)
            ContextMenuItem::slider("opacity", "Opacity", "node.getOpacity", "node.setOpacity", 0.5, 1.0, 0.05),
            ContextMenuItem::select("tint", "Style", "node.getTint", "node.setTint", vec![
                ("ink".to_string(), "Ink".to_string()),
                ("moss".to_string(), "Moss".to_string()),
                ("indigo".to_string(), "Indigo".to_string()),
                ("maroon".to_string(), "Maroon".to_string()),
            ]),
            ContextMenuItem::slider("font_scale", "Font Scale", "node.getFontScale", "node.setFontScale", 0.7, 2.5, 0.1),
            ContextMenuItem::separator(),
            ContextMenuItem::command("close", "Close", "projection.close"),
        ]
    }

    /// Build options popover for widget node
    pub fn for_widget_node(_node_id: &str) -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::info("title", "Title", "Widget"),
            ContextMenuItem::separator(),
            ContextMenuItem::command("plugin_settings", "Plugin Settings", "widget.pluginSettings"),
            ContextMenuItem::slider("update_interval", "Update Interval (ms)", "widget.getUpdateInterval", "widget.setUpdateInterval", 100.0, 60000.0, 100.0),
            ContextMenuItem::separator(),
            ContextMenuItem::submenu("actions", "Widget Actions", vec![]), // Populated by plugin
            ContextMenuItem::command("close", "Close", "widget.close"),
        ]
    }

    /// Build context menu for terminal selection (text selected)
    pub fn for_terminal_selection() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::command("copy", "Copy", "clipboard.copy"),
            ContextMenuItem::command("pin_live", "Pin as Live View", "view.pinLive"),
            ContextMenuItem::command("pin_snapshot", "Pin as Snapshot", "view.pinSnapshot"),
            ContextMenuItem::command("zoom_selection", "Zoom to Selection", "camera.zoomToSelection"),
            ContextMenuItem::separator(),
            ContextMenuItem::command("read_selection", "Read Selection", "reading.enterSelection"),
            ContextMenuItem::command("filter_selection", "Filter Selection", "view.filterSelection"),
            ContextMenuItem::command("search_selection", "Search Selection", "search.selection"),
        ]
    }

    /// Build context menu for empty workspace
    pub fn for_workspace() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::command("new_terminal", "New Terminal", "terminal.new"),
            ContextMenuItem::command("new_varied", "New Varied Terminal", "terminal.varied"),
            ContextMenuItem::command("paste", "Paste", "clipboard.paste"),
            ContextMenuItem::separator(),
            ContextMenuItem::command("tile_h", "Tile Horizontally", "layout.tileH"),
            ContextMenuItem::command("tile_v", "Tile Vertically", "layout.tileV"),
            ContextMenuItem::command("tile_grid", "Tile Grid", "layout.tileGrid"),
            ContextMenuItem::command("cascade", "Cascade", "layout.cascade"),
            ContextMenuItem::command("orbit", "Orbit", "layout.orbit"),
            ContextMenuItem::command("focus_ring", "Focus Ring", "layout.focus"),
            ContextMenuItem::separator(),
            ContextMenuItem::command("save_layout", "Save Layout", "layout.save"),
            ContextMenuItem::command("restore_layout", "Restore Layout", "layout.restore"),
            ContextMenuItem::separator(),
            ContextMenuItem::command("toggle_effects", "Toggle Effects", "effects.toggle"),
            ContextMenuItem::command("plugin_console", "Plugin Console", "plugin.console"),
            ContextMenuItem::command("help", "Help", "help.open"),
        ]
    }
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_mode() {
        assert_eq!(InteractionMode::Workspace.as_str(), "workspace");
        assert_eq!(InteractionMode::Terminal.as_str(), "terminal");
        assert_eq!(InteractionMode::Reading.as_str(), "reading");
        assert_eq!(InteractionMode::Dashboard.as_str(), "dashboard");
    }

    #[test]
    fn test_modifiers_default() {
        let m = Modifiers::default();
        assert!(!m.ctrl && !m.shift && !m.alt && !m.meta);
    }

    #[test]
    fn test_context_menu_builder() {
        let menu = ContextMenuBuilder::for_terminal_selection();
        assert!(!menu.is_empty());
        assert_eq!(menu[0].id, "copy");
        // Separator is at index 4 (after copy, pin_live, pin_snapshot, zoom_selection)
        assert!(matches!(menu[4].kind, ContextMenuItemKind::Separator));
    }
}
