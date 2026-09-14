//! Permission - capability-based security system for plugins.

/// Scope for a permission
#[derive(Debug, Clone)]
pub enum PermissionScope {
    /// All instances of this permission
    All,
    /// Only the one creating this permission
    Created,
    /// Only specific permissions granted
    Granted(String),
    /// Limited to specific paths/patterns
    Scoped(Vec<String>),
    /// Limited to specific origins (for network)
    OriginScoped(Vec<String>),
    /// Limited to specific commands
    CommandScoped(Vec<String>),
}

/// Permission for accessing resources
#[derive(Debug, Clone)]
pub struct Permission {
    /// Permission type (e.g., "workspace.read", "terminal.create")
    pub permission_type: String,
    /// Scope of this permission
    pub scope: PermissionScope,
    /// Description
    pub description: String,
    /// Whether the permission is granted
    pub granted: bool,
    /// When granted
    pub granted_at: Option<i64>,
}

impl Permission {
    /// Create a new permission
    pub fn new(permission_type: &str, scope: PermissionScope, description: &str) -> Self {
        Self {
            permission_type: permission_type.to_string(),
            scope,
            description: description.to_string(),
            granted: false,
            granted_at: None,
        }
    }

    /// Grant this permission
    pub fn grant(&mut self) {
        self.granted = true;
        self.granted_at = Some(chrono::Utc::now().timestamp());
    }

    /// Revoke this permission
    pub fn revoke(&mut self) {
        self.granted = false;
        self.granted_at = None;
    }

    /// Check if permission is granted
    pub fn is_granted(&self) -> bool {
        self.granted
    }

    /// Check if this permission allows the action
    pub fn allows(&self, context: &PermissionContext) -> bool {
        if !self.is_granted() {
            return false;
        }
        match &self.scope {
            PermissionScope::All => true,
            PermissionScope::Created => context.check_created(self.permission_type.as_str()),
            PermissionScope::Granted(id) => context.check_granted(self.permission_type.as_str(), id),
            PermissionScope::Scoped(patterns) => context.check_scoped(self.permission_type.as_str(), patterns),
            PermissionScope::OriginScoped(origins) => context.check_origins(self.permission_type.as_str(), origins),
            PermissionScope::CommandScoped(commands) => context.check_commands(self.permission_type.as_str(), commands),
        }
    }
}

/// Context for permission checking
#[derive(Debug, Clone, Default)]
pub struct PermissionContext {
    /// Source plugin ID
    pub source: Option<String>,
    /// Target entity ID
    pub target: Option<String>,
    /// Command being executed
    pub command: Option<String>,
    /// Path for file operations
    pub path: Option<String>,
    /// Origin for network operations
    pub origin: Option<String>,
}

impl PermissionContext {
    /// Create a new context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the source plugin
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// Set the target
    pub fn with_target(mut self, target: &str) -> Self {
        self.target = Some(target.to_string());
        self
    }

    /// Set the command
    pub fn with_command(mut self, command: &str) -> Self {
        self.command = Some(command.to_string());
        self
    }

    /// Set the path
    pub fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Set the origin
    pub fn with_origin(mut self, origin: &str) -> Self {
        self.origin = Some(origin.to_string());
        self
    }

    /// Check if created by current context
    fn check_created(&self, perm_type: &str) -> bool {
        match perm_type {
            "terminal.read" | "terminal.write" | "workspace.read" | "workspace.write" => {
                self.target.is_some()
            }
            _ => false,
        }
    }

    /// Check if granted by current context
    fn check_granted(&self, perm_type: &str, _id: &str) -> bool {
        match perm_type {
            "storage" => self.source.is_some(),
            _ => false,
        }
    }

    /// Check if scoped to paths
    fn check_scoped(&self, _perm_type: &str, patterns: &[String]) -> bool {
        if self.path.is_none() {
            return false;
        }
        // Check if path matches any pattern
        for pattern in patterns {
            if pattern.contains("*") {
                // Simple wildcard matching
                continue;
            }
            if self.path.as_ref().unwrap().contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Check if scoped to origins
    fn check_origins(&self, _perm_type: &str, origins: &[String]) -> bool {
        if self.origin.is_none() {
            return false;
        }
        for origin in origins {
            if self.origin.as_ref().unwrap().starts_with(origin) {
                return true;
            }
        }
        false
    }

    /// Check if scoped to commands
    fn check_commands(&self, _perm_type: &str, commands: &[String]) -> bool {
        if self.command.is_none() {
            return false;
        }
        for cmd in commands {
            if self.command.as_ref().unwrap() == cmd {
                return true;
            }
        }
        false
    }
}