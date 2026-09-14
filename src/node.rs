//! Node - a positioned object in the workspace.
//!
//! Nodes have components: Transform, Style, Focus, Input, Surface, Behavior, Permissions.

use super::*;

/// Input behavior for a node, describing how it handles input events.
#[derive(Debug, Clone)]
pub struct InputBehavior {
    /// The interaction mode name (workspace, terminal, reading, dashboard)
    pub mode: String,
    /// Whether the node captures keyboard focus
    pub focus_key: bool,
    /// Whether the node captures pointer events
    pub focus_pointer: bool,
    /// Whether the node forwards events to an embedded terminal
    pub terminal_mouse: bool,
    /// Custom key bindings for this node
    pub keybindings: Vec<String>,
}

impl InputBehavior {
    pub fn new(mode: &str) -> Self {
        Self {
            mode: mode.to_string(),
            focus_key: false,
            focus_pointer: false,
            terminal_mouse: false,
            keybindings: Vec::new(),
        }
    }

    pub fn with_key_focus(mut self) -> Self {
        self.focus_key = true;
        self
    }

    pub fn with_pointer_focus(mut self) -> Self {
        self.focus_pointer = true;
        self
    }

    pub fn with_terminal_mouse(mut self) -> Self {
        self.terminal_mouse = true;
        self
    }

    pub fn with_keybinding(mut self, binding: &str) -> Self {
        self.keybindings.push(binding.to_string());
        self
    }
}

impl Default for InputBehavior {
    fn default() -> Self {
        Self::new("workspace")
    }
}

/// Plugin behavior for a node, describing lifecycle hooks from plugins.
#[derive(Debug, Clone, Default)]
pub struct PluginBehavior {
    /// The plugin ID that owns this node, if any
    pub plugin_id: Option<String>,
    /// The widget ID this node uses, if any
    pub widget_id: Option<String>,
    /// Whether the node is managed by a plugin
    pub plugin_managed: bool,
    /// Custom state JSON attached by the plugin
    pub plugin_state: Option<serde_json::Value>,
}

impl PluginBehavior {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn from_plugin(plugin_id: &str) -> Self {
        Self {
            plugin_id: Some(plugin_id.to_string()),
            plugin_managed: true,
            ..Default::default()
        }
    }

    pub fn with_widget(plugin_id: &str, widget_id: &str) -> Self {
        Self {
            plugin_id: Some(plugin_id.to_string()),
            widget_id: Some(widget_id.to_string()),
            plugin_managed: true,
            ..Default::default()
        }
    }
}

/// Components that a node can have
#[derive(Debug, Clone)]
pub struct Node {
    /// The unique ID of this node
    pub id: NodeId,
    /// The transform component (position, scale, rotation)
    pub transform: Transform,
    /// Node size in world pixels (width, height)
    pub size: (i32, i32),
    /// The style component (colors, fonts)
    pub style: Theme,
    /// The input behavior component
    pub input: InputBehavior,
    /// The surface ID this node renders
    pub surface_id: Option<SurfaceId>,
    /// The projection component (how to project the surface)
    pub projection: Option<ProjectionSurface>,
    /// Plugin behavior component
    pub plugin: PluginBehavior,
    /// Permissions for this node
    pub permissions: Vec<Permission>,
    /// Group ID if this node belongs to a group
    pub group_id: Option<NodeId>,
}

impl Node {
    /// Create a new node with default values
    pub fn new(id: NodeId, x: i32, y: i32) -> Self {
        Self {
            id,
            transform: Transform::new(x, y),
            size: (480, 320),
            style: Theme::default(),
            input: InputBehavior::default(),
            surface_id: None,
            projection: None,
            plugin: PluginBehavior::none(),
            permissions: vec![],
            group_id: None,
        }
    }

    /// Set the surface for this node
    pub fn set_surface(&mut self, surface_id: SurfaceId) {
        self.surface_id = Some(surface_id);
    }

    /// Get the surface ID
    pub fn surface_id(&self) -> Option<SurfaceId> {
        self.surface_id
    }

    /// Set the input behavior
    pub fn set_input(&mut self, input: InputBehavior) {
        self.input = input;
    }

    /// Get the input behavior
    pub fn input(&self) -> &InputBehavior {
        &self.input
    }

    /// Set the plugin behavior
    pub fn set_plugin(&mut self, plugin: PluginBehavior) {
        self.plugin = plugin;
    }

    /// Get the plugin behavior
    pub fn plugin(&self) -> &PluginBehavior {
        &self.plugin
    }

    /// Set the group ID
    pub fn set_group(&mut self, group_id: NodeId) {
        self.group_id = Some(group_id);
    }

    /// Remove from group
    pub fn ungroup(&mut self) {
        self.group_id = None;
    }

    /// Add a permission to this node
    pub fn add_permission(&mut self, permission: Permission) {
        self.permissions.push(permission);
    }
}
