//! Workspace - the infinite zoomable canvas containing nodes and a camera.
//!
//! The workspace contains a scene graph and a camera lens.

use super::*;
use std::collections::HashMap;

/// Scene graph node reference
#[derive(Debug, Clone)]
pub struct SceneNodeRef {
    pub node_id: NodeId,
    pub parent: Option<NodeId>,
}

/// Scene graph managing all nodes in the workspace
#[derive(Debug, Clone)]
pub struct SceneGraph {
    nodes: HashMap<NodeId, Node>,
    children: HashMap<NodeId, Vec<NodeId>>,
    parents: HashMap<NodeId, Option<NodeId>>,
    groups: HashMap<NodeId, Vec<NodeId>>,
    z_order: Vec<NodeId>,
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            children: HashMap::new(),
            parents: HashMap::new(),
            groups: HashMap::new(),
            z_order: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: Node) {
        let id = node.id;
        self.nodes.insert(id, node);
        self.children.entry(id).or_default();
        self.parents.entry(id).or_default();
        self.z_order.push(id);
    }

    pub fn remove_node(&mut self, node_id: NodeId) -> Option<Node> {
        self.z_order.retain(|id| *id != node_id);
        self.children.remove(&node_id);
        self.parents.remove(&node_id);
        self.nodes.remove(&node_id)
    }

    pub fn get_node(&self, node_id: NodeId) -> Option<&Node> {
        self.nodes.get(&node_id)
    }

    pub fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&node_id)
    }

    pub fn all_nodes(&self) -> Vec<&Node> {
        self.nodes.values().collect()
    }

    pub fn node_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    pub fn add_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        if self.nodes.contains_key(&parent_id) && self.nodes.contains_key(&child_id) {
            self.children.entry(parent_id).or_default().push(child_id);
            self.parents.insert(child_id, Some(parent_id));
        }
    }

    pub fn remove_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        if let Some(children) = self.children.get_mut(&parent_id) {
            children.retain(|id| *id != child_id);
        }
        self.parents.insert(child_id, None);
    }

    pub fn get_children(&self, node_id: NodeId) -> Vec<NodeId> {
        self.children.get(&node_id).cloned().unwrap_or_default()
    }

    pub fn get_parent(&self, node_id: NodeId) -> Option<NodeId> {
        self.parents.get(&node_id).copied().flatten()
    }

    pub fn create_group(&mut self, group_id: NodeId, members: Vec<NodeId>) {
        for member_id in &members {
            if let Some(node) = self.nodes.get_mut(member_id) {
                node.group_id = Some(group_id);
            }
        }
        self.groups.insert(group_id, members);
    }

    pub fn add_to_group(&mut self, group_id: NodeId, node_id: NodeId) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.group_id = Some(group_id);
            self.groups.entry(group_id).or_default();
            if let Some(members) = self.groups.get_mut(&group_id) {
                if !members.contains(&node_id) {
                    members.push(node_id);
                }
            }
        }
    }

    pub fn remove_from_group(&mut self, node_id: NodeId) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            if let Some(group_id) = node.group_id {
                if let Some(members) = self.groups.get_mut(&group_id) {
                    members.retain(|id| *id != node_id);
                }
                node.group_id = None;
            }
        }
    }

    pub fn get_group_members(&self, group_id: NodeId) -> Vec<NodeId> {
        self.groups.get(&group_id).cloned().unwrap_or_default()
    }

    pub fn get_groups(&self) -> Vec<(NodeId, Vec<NodeId>)> {
        self.groups
            .iter()
            .map(|(id, members)| (*id, members.clone()))
            .collect()
    }

    pub fn bring_to_front(&mut self, node_id: NodeId) {
        self.z_order.retain(|id| *id != node_id);
        self.z_order.push(node_id);
    }

    pub fn send_to_back(&mut self, node_id: NodeId) {
        self.z_order.retain(|id| *id != node_id);
        self.z_order.insert(0, node_id);
    }

    pub fn z_order(&self) -> &[NodeId] {
        &self.z_order
    }

    pub fn contains(&self, node_id: NodeId) -> bool {
        self.nodes.contains_key(&node_id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// The workspace is the infinite zoomable canvas containing nodes and a camera lens.
pub struct Workspace {
    pub camera: CameraLens,
    pub scene: SceneGraph,
    pub active: bool,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            camera: CameraLens::new(Camera::new()),
            scene: SceneGraph::new(),
            active: true,
        }
    }

    pub fn add_node(&mut self, node: Node) {
        self.scene.add_node(node);
    }

    pub fn remove_node(&mut self, node_id: NodeId) -> Option<Node> {
        self.scene.remove_node(node_id)
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub fn node_transform(&self, node_id: NodeId) -> Option<Transform> {
        self.scene.get_node(node_id).map(|node| node.transform)
    }

    pub fn set_zoom(&mut self, zoom: f64) {
        self.camera.set_zoom(zoom);
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.scene.z_order
    }

    pub fn node_ids(&self) -> Vec<NodeId> {
        self.scene.node_ids()
    }

    pub fn get_node(&self, node_id: NodeId) -> Option<&Node> {
        self.scene.get_node(node_id)
    }

    pub fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut Node> {
        self.scene.get_node_mut(node_id)
    }

    pub fn all_nodes(&self) -> Vec<&Node> {
        self.scene.all_nodes()
    }

    pub fn create_group(&mut self, group_id: NodeId, members: Vec<NodeId>) {
        self.scene.create_group(group_id, members);
    }

    pub fn add_node_to_group(&mut self, group_id: NodeId, node_id: NodeId) {
        self.scene.add_to_group(group_id, node_id);
    }

    pub fn remove_node_from_group(&mut self, node_id: NodeId) {
        self.scene.remove_from_group(node_id);
    }

    pub fn get_group_members(&self, group_id: NodeId) -> Vec<NodeId> {
        self.scene.get_group_members(group_id)
    }

    pub fn get_groups(&self) -> Vec<(NodeId, Vec<NodeId>)> {
        self.scene.get_groups()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Export workspace state as JSON for persistence
    pub fn export_state(&self) -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "camera": {
                "x": self.camera.x,
                "y": self.camera.y,
                "zoom": self.camera.zoom,
                "bookmarks": self.camera.bookmarks,
            },
            "nodes": self.scene.node_ids().len(),
            "groups": self.scene.get_groups().len(),
        })
    }

    /// Import workspace state from JSON
    pub fn import_state(
        &mut self,
        state: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(camera_val) = state.get("camera") {
            if let (Some(x), Some(y), Some(zoom)) = (
                camera_val.get("x").and_then(|v| v.as_f64()),
                camera_val.get("y").and_then(|v| v.as_f64()),
                camera_val.get("zoom").and_then(|v| v.as_f64()),
            ) {
                self.camera.x = x;
                self.camera.y = y;
                self.camera.zoom = zoom;
                self.camera.target_x = x;
                self.camera.target_y = y;
                self.camera.target_zoom = zoom;
            }
            if let Some(bookmarks) = camera_val.get("bookmarks").and_then(|v| v.as_array()) {
                for bm in bookmarks {
                    if let (Some(name), Some(bx), Some(by), Some(bz)) = (
                        bm.get("name").and_then(|v| v.as_str()),
                        bm.get("x").and_then(|v| v.as_f64()),
                        bm.get("y").and_then(|v| v.as_f64()),
                        bm.get("zoom").and_then(|v| v.as_f64()),
                    ) {
                        self.camera.bookmarks.push(crate::camera::CameraBookmark {
                            name: name.to_string(),
                            x: bx,
                            y: by,
                            zoom: bz,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}
impl SceneGraph {
    pub fn all_nodes_mut(&mut self) -> Vec<&mut Node> {
        self.nodes.values_mut().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new();
        assert!(ws.is_active());
        assert!(ws.scene.is_empty());
    }

    #[test]
    fn test_scene_graph_add_node() {
        let mut sg = SceneGraph::new();
        let node = Node::new(NodeId(1), 100, 200);
        sg.add_node(node);
        assert!(sg.contains(NodeId(1)));
        assert_eq!(sg.len(), 1);
        assert!(!sg.is_empty());
    }

    #[test]
    fn test_scene_graph_groups() {
        let mut sg = SceneGraph::new();
        sg.add_node(Node::new(NodeId(1), 0, 0));
        sg.add_node(Node::new(NodeId(2), 100, 100));
        sg.add_node(Node::new(NodeId(3), 200, 200));
        sg.create_group(NodeId(100), vec![NodeId(1), NodeId(2)]);
        assert_eq!(sg.get_group_members(NodeId(100)).len(), 2);
        assert_eq!(sg.get_groups().len(), 1);
    }

    #[test]
    fn test_scene_graph_z_order() {
        let mut sg = SceneGraph::new();
        sg.add_node(Node::new(NodeId(1), 0, 0));
        sg.add_node(Node::new(NodeId(2), 100, 100));
        sg.add_node(Node::new(NodeId(3), 200, 200));
        sg.bring_to_front(NodeId(1));
        let order = sg.z_order();
        assert_eq!(order[order.len() - 1], NodeId(1));
    }

    #[test]
    fn test_workspace_groups() {
        let mut ws = Workspace::new();
        ws.add_node(Node::new(NodeId(1), 0, 0));
        ws.add_node(Node::new(NodeId(2), 100, 100));
        ws.create_group(NodeId(100), vec![NodeId(1), NodeId(2)]);
        assert_eq!(ws.get_group_members(NodeId(100)).len(), 2);
    }

    #[test]
    fn test_workspace_export_state() {
        let mut ws = Workspace::new();
        ws.add_node(Node::new(NodeId(1), 0, 0));
        let state = ws.export_state();
        assert_eq!(state["version"], 1);
        assert_eq!(state["nodes"], 1);
    }

    #[test]
    fn test_workspace_import_state() {
        let mut ws = Workspace::new();
        ws.camera_mut().set_position(100.0, 200.0);
        ws.camera_mut().set_zoom(2.0);
        let state = ws.export_state();
        let mut ws2 = Workspace::new();
        assert!(ws2.import_state(&state).is_ok());
        assert_eq!(ws2.camera().x, 100.0);
        assert_eq!(ws2.camera().y, 200.0);
        assert_eq!(ws2.camera().zoom, 2.0);
    }
}
