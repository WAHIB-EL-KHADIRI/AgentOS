use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Permission {
    ReadFileSystem,
    WriteFileSystem,
    Network,
    UseSecret(String),
    ExecuteTool(String),
    ManageAgents,
    Custom(String),
}

impl Permission {
    pub fn description(&self) -> String {
        match self {
            Permission::ReadFileSystem => "Read files from the filesystem".into(),
            Permission::WriteFileSystem => "Write files to the filesystem".into(),
            Permission::Network => "Make network requests".into(),
            Permission::UseSecret(key) => format!("Use secret '{key}'"),
            Permission::ExecuteTool(tool) => format!("Execute tool '{tool}'"),
            Permission::ManageAgents => "Manage other agents".into(),
            Permission::Custom(name) => format!("Custom permission '{name}'"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionSet {
    permissions: BTreeSet<Permission>,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, permission: Permission) {
        self.permissions.insert(permission);
    }

    pub fn revoke(&mut self, permission: &Permission) {
        self.permissions.remove(permission);
    }

    pub fn contains(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Permission> {
        self.permissions.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.permissions.len()
    }

    pub fn check(&self, permission: &Permission) -> Result<(), String> {
        if self.contains(permission) {
            Ok(())
        } else {
            Err(format!("permission denied: {permission:?}"))
        }
    }

    pub fn all_permissions() -> Self {
        let mut set = Self::new();
        set.grant(Permission::ReadFileSystem);
        set.grant(Permission::WriteFileSystem);
        set.grant(Permission::Network);
        set.grant(Permission::ManageAgents);
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_grant_and_check() {
        let mut set = PermissionSet::new();
        set.grant(Permission::ReadFileSystem);
        assert!(set.contains(&Permission::ReadFileSystem));
        assert!(!set.contains(&Permission::Network));
    }

    #[test]
    fn test_permission_check_ok() {
        let mut set = PermissionSet::new();
        set.grant(Permission::Network);
        assert!(set.check(&Permission::Network).is_ok());
    }

    #[test]
    fn test_permission_check_denied() {
        let set = PermissionSet::new();
        assert!(set.check(&Permission::Network).is_err());
    }

    #[test]
    fn test_permission_revoke() {
        let mut set = PermissionSet::new();
        set.grant(Permission::ReadFileSystem);
        set.revoke(&Permission::ReadFileSystem);
        assert!(!set.contains(&Permission::ReadFileSystem));
    }

    #[test]
    fn test_all_permissions() {
        let all = PermissionSet::all_permissions();
        assert!(all.contains(&Permission::ReadFileSystem));
        assert!(all.contains(&Permission::WriteFileSystem));
        assert!(all.contains(&Permission::Network));
        assert!(all.contains(&Permission::ManageAgents));
    }

    #[test]
    fn test_permission_description() {
        let desc = Permission::ReadFileSystem.description();
        assert!(!desc.is_empty());
        assert!(desc.contains("Read files"));
    }
}
