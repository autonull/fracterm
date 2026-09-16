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

/// Context menu item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMenuItem {
    pub id: String,
    pub label: String,
    pub command: Option<String>,
    pub args: HashMap<String, CommandValue>,
    pub enabled: bool,
    pub separator: bool,
    pub submenu: Option<Vec<ContextMenuItem>>,
}

impl ContextMenuItem {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            command: None,
            args: HashMap::new(),
            enabled: true,
            separator: false,
            submenu: None,
        }
    }

    pub fn separator() -> Self {
        Self {
            id: "".to_string(),
            label: "".to_string(),
            command: None,
            args: HashMap::new(),
            enabled: false,
            separator: true,
            submenu: None,
        }
    }

    pub fn with_command(mut self, command: &str) -> Self {
        self.command = Some(command.to_string());
        self
    }

    pub fn with_args(mut self, args: HashMap<String, CommandValue>) -> Self {
        self.args = args;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn with_submenu(mut self, items: Vec<ContextMenuItem>) -> Self {
        self.submenu = Some(items);
        self
    }
}

/// Context menu builder for different targets
pub struct ContextMenuBuilder;

impl ContextMenuBuilder {
    /// Build context menu for terminal selection
    pub fn for_terminal_selection() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::new("copy", "Copy").with_command("clipboard.copy"),
            ContextMenuItem::new("pin_live", "Pin as Live View").with_command("view.pinLive"),
            ContextMenuItem::new("pin_snapshot", "Pin as Snapshot")
                .with_command("view.pinSnapshot"),
            ContextMenuItem::new("zoom_sel", "Zoom to Selection")
                .with_command("camera.zoomToSelection"),
            ContextMenuItem::separator(),
            ContextMenuItem::new("read_sel", "Read Selection")
                .with_command("reading.enterSelection"),
            ContextMenuItem::new("filter_sel", "Filter Selection")
                .with_command("view.filterSelection"),
            ContextMenuItem::new("search_sel", "Search Selection").with_command("search.selection"),
        ]
    }

    /// Build context menu for terminal node
    pub fn for_terminal_node() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::new("new_profile", "New Terminal from Profile")
                .with_command("terminal.newFromProfile"),
            ContextMenuItem::new("rename", "Rename").with_command("terminal.rename"),
            ContextMenuItem::new("opacity", "Opacity").with_command("terminal.opacity"),
            ContextMenuItem::new("style", "Style").with_command("terminal.style"),
            ContextMenuItem::new("input_mode", "Input Mode").with_command("terminal.inputMode"),
            ContextMenuItem::separator(),
            ContextMenuItem::new("save_layout", "Save to Layout").with_command("layout.save"),
            ContextMenuItem::new("close", "Close")
                .with_command("terminal.close")
                .disabled(),
        ]
    }

    /// Build context menu for projection node
    pub fn for_projection_node() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::new("follow", "Follow Source").with_command("projection.follow"),
            ContextMenuItem::new("edit_filter", "Edit Filter")
                .with_command("projection.editFilter"),
            ContextMenuItem::new("snapshot", "Snapshot").with_command("projection.snapshot"),
            ContextMenuItem::new("detach", "Detach").with_command("projection.detach"),
            ContextMenuItem::separator(),
            ContextMenuItem::new("reading", "Reading Mode").with_command("reading.enter"),
            ContextMenuItem::new("close", "Close").with_command("projection.close"),
        ]
    }

    /// Build context menu for empty workspace
    pub fn for_workspace() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::new("new_terminal", "New Terminal").with_command("terminal.new"),
            ContextMenuItem::new("paste", "Paste").with_command("clipboard.paste"),
            ContextMenuItem::separator(),
            ContextMenuItem::new("save_layout", "Save Layout").with_command("layout.save"),
            ContextMenuItem::new("restore_layout", "Restore Layout").with_command("layout.restore"),
            ContextMenuItem::separator(),
            ContextMenuItem::new("toggle_effects", "Toggle Effects").with_command("effects.toggle"),
            ContextMenuItem::new("plugin_console", "Plugin Console").with_command("plugin.console"),
            ContextMenuItem::new("help", "Help").with_command("help.open"),
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
        assert!(!menu[1].separator);
    }
}
