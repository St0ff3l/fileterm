/// Stable category independent of a per-session scope/tab ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogCategory {
    App,
    Workspace,
    Connection,
    Terminal,
    Monitoring,
    Performance,
    Files,
    Transfer,
    Storage,
    Security,
    Updates,
    Integration,
    Logging,
    Other,
}

impl LogCategory {
    pub fn from_name(name: &str) -> Self {
        Self::from_component(&name.trim().to_ascii_lowercase()).unwrap_or(Self::Other)
    }

    pub fn from_scope(scope: &str) -> Self {
        if matches!(scope, "renderer:performance" | "app:windows-performance") {
            return Self::Performance;
        }
        if scope == "renderer:monitoring" {
            return Self::Monitoring;
        }
        if scope == "renderer:workspace" {
            return Self::Workspace;
        }

        if let Some(category) = scope.split(':').next().and_then(Self::from_component) {
            return category;
        }

        // Native tracing targets use Rust module paths (crate::domain::module)
        // while compatibility scopes use the `domain:session` form above.
        scope
            .rsplit("::")
            .find_map(Self::from_component)
            .unwrap_or(Self::Other)
    }

    fn from_component(component: &str) -> Option<Self> {
        match component {
            "app" => Some(Self::App),
            "window" | "fonts" | "panic" => Some(Self::App),
            "workspace" | "renderer" => Some(Self::Workspace),
            "russh" | "russh_sftp" | "connection" | "ssh" | "ssh-probe" | "ssh-interaction"
            | "ftp" | "telnet" | "serial" | "connection-test" | "worker" => Some(Self::Connection),
            "terminal" | "local" => Some(Self::Terminal),
            "performance" => Some(Self::Performance),
            "metrics" | "monitoring" => Some(Self::Monitoring),
            "sftp" | "files" | "local-files" => Some(Self::Files),
            "transfer" => Some(Self::Transfer),
            "storage" | "profile" => Some(Self::Storage),
            "security" => Some(Self::Security),
            "update" | "updates" => Some(Self::Updates),
            "integration" | "mcp" | "ai" | "webdav" | "s3" | "s3-backup" => Some(Self::Integration),
            "logging" => Some(Self::Logging),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Workspace => "workspace",
            Self::Connection => "connection",
            Self::Terminal => "terminal",
            Self::Monitoring => "monitoring",
            Self::Performance => "performance",
            Self::Files => "files",
            Self::Transfer => "transfer",
            Self::Storage => "storage",
            Self::Security => "security",
            Self::Updates => "updates",
            Self::Integration => "integration",
            Self::Logging => "logging",
            Self::Other => "other",
        }
    }
}

#[cfg(test)]
mod backup_scope_tests {
    use super::LogCategory;

    #[test]
    fn backup_scopes_share_the_integration_log_filter() {
        assert_eq!(LogCategory::from_scope("webdav"), LogCategory::Integration);
        assert_eq!(
            LogCategory::from_scope("s3-backup"),
            LogCategory::Integration
        );
    }
}
