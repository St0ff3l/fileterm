impl LocalTerminalRuntimeGate {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(true),
            emit_lock: Mutex::new(()),
        }
    }

    pub async fn deactivate(&self) {
        // Flip the fast-path first so a reader that wakes while shutdown is
        // waiting cannot enqueue another chunk. The short lock wait lets an
        // already-publishing chunk finish in the normal case, but a stalled
        // renderer channel must never make close/reconnect hang forever.
        self.active.store(false, Ordering::Release);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(250), self.emit_lock.lock())
            .await;
    }
}

impl Default for LocalTerminalRuntimeGate {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferRunHandle {
    pub async fn wait_until_settled(mut self) {
        if *self.settled.borrow() {
            return;
        }
        while self.settled.changed().await.is_ok() {
            if *self.settled.borrow() {
                return;
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TransferProgressSample {
    pub bytes: u64,
    pub sampled_at: std::time::Instant,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTab {
    pub id: String,
    pub profile_id: String,
    pub session_type: String,
    pub title: String,
    pub layout: String, // "terminal-file" | "file-only" | "terminal-only"
    pub status: WorkspaceTabStatus,
    /// External CLI/MCP sessions stay attached to the App worker but are not
    /// shown in the top-level tab bar until the user attaches them.
    #[serde(default)]
    pub is_background: bool,
    /// External caller that created the session. Ordinary GUI sessions omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<WorkspaceSessionSource>,
    /// 分屏树根节点；普通 tab 为 None。只有分屏的根 tab 持有。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_root: Option<PaneNode>,
    /// 分屏 leaf 所属的顶层 workspace tab。leaf 仍保留独立 session，但绝不作为顶栏 tab。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_root_tab_id: Option<String>,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceSessionSource {
    Cli,
    #[default]
    Mcp,
}

/// 分屏方向：row = 左右分（垂直分屏），column = 上下分（水平分屏）
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Row,
    Column,
}

/// 分屏树节点。leaf 引用一个真实 tab id；split 递归持有子节点。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PaneNode {
    Leaf {
        // The renderer's core contract uses `tabId`. Keep accepting the
        // previous Rust-shaped spelling so a restored in-memory snapshot does
        // not make the layout unreadable during an upgrade.
        #[serde(rename = "tabId", alias = "tab_id")]
        tab_id: String,
    },
    Split {
        direction: SplitDirection,
        children: Vec<PaneNode>,
        /// 每个子节点占比，长度与 children 一致，和为 1。
        weights: Vec<f32>,
    },
}

impl PaneNode {
    /// 递归收集所有 leaf 的 tab_id。
    pub fn leaf_tab_ids(&self) -> Vec<String> {
        match self {
            PaneNode::Leaf { tab_id } => vec![tab_id.clone()],
            PaneNode::Split { children, .. } => {
                children.iter().flat_map(|c| c.leaf_tab_ids()).collect()
            }
        }
    }

    /// 递归查找并替换指定 leaf 节点为新子树。返回是否替换成功。
    /// 用于分屏：把当前 leaf 替换为 split(leaf, new_leaf)。
    pub fn replace_leaf(&mut self, target_tab_id: &str, replacement: PaneNode) -> bool {
        match self {
            PaneNode::Leaf { tab_id } if tab_id == target_tab_id => {
                *self = replacement;
                true
            }
            PaneNode::Leaf { .. } => false,
            PaneNode::Split { children, .. } => {
                for child in children.iter_mut() {
                    if child.replace_leaf(target_tab_id, replacement.clone()) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// 递归移除指定 leaf 节点。移除后规整：
    ///
    /// - split 只剩一个子节点时，用唯一子节点替换该 split
    ///
    /// 返回 true 表示子树中发生了移除。
    pub fn remove_leaf(&mut self, target_tab_id: &str) -> bool {
        match self {
            PaneNode::Leaf { tab_id } if tab_id == target_tab_id => true,
            PaneNode::Leaf { .. } => false,
            PaneNode::Split {
                children, weights, ..
            } => {
                // 先检查直接 children 里有没有匹配的 leaf
                let direct_idx = children.iter().position(|c| match c {
                    PaneNode::Leaf { tab_id } => tab_id == target_tab_id,
                    PaneNode::Split { .. } => false,
                });

                if let Some(idx) = direct_idx {
                    children.remove(idx);
                    if weights.len() > idx {
                        weights.remove(idx);
                    }
                } else {
                    // 递归到子 split
                    let mut found_in_child = false;
                    for child in children.iter_mut() {
                        if child.remove_leaf(target_tab_id) {
                            found_in_child = true;
                            break;
                        }
                    }
                    if !found_in_child {
                        return false;
                    }
                }

                normalize_weights(weights);
                // 规整：split 只剩一个子节点时用唯一子节点替换自身
                if children.len() == 1 {
                    let only = children.pop().unwrap();
                    *self = only;
                }
                true
            }
        }
    }

    /// 更新指定 split 节点的 weights。`pane_path` 使用从根节点到该 split 的 child index。
    pub fn set_split_weights_at_path(&mut self, pane_path: &[usize], next_weights: &[f32]) -> bool {
        let PaneNode::Split {
            children, weights, ..
        } = self
        else {
            return false;
        };

        if pane_path.is_empty() {
            if weights.len() != next_weights.len() {
                return false;
            }
            weights.clone_from_slice(next_weights);
            normalize_weights(weights);
            return true;
        }

        let Some(child) = children.get_mut(pane_path[0]) else {
            return false;
        };
        child.set_split_weights_at_path(&pane_path[1..], next_weights)
    }
}

/// 归一化 weights 数组：确保和为 1，且长度 >= 2 时每项 > 0。
fn normalize_weights(weights: &mut [f32]) {
    if weights.is_empty() {
        return;
    }
    let sum: f32 = weights.iter().sum();
    if sum <= 0.0 || !sum.is_finite() {
        let n = weights.len() as f32;
        for w in weights.iter_mut() {
            *w = 1.0 / n;
        }
        return;
    }
    for w in weights.iter_mut() {
        *w /= sum;
    }
}

/// Rust-side mirror of `packages/core::TabStatus`. Keeping this as an enum
/// prevents backend-only strings such as `disconnected` from leaking into the
/// renderer and silently breaking menus/status views.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceTabStatus {
    Idle,
    Connecting,
    Connected,
    Error,
    Closed,
}

impl WorkspaceTabStatus {
    pub fn is_connected(self) -> bool {
        self == Self::Connected
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCapabilities {
    pub terminal: bool,
    pub files: bool,
    pub resource_monitoring: bool,
    pub shell_integration: bool,
    pub file_access: bool,
    pub tunnels: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDiskSpace {
    pub available_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileCapabilities {
    pub protocol: String,
    pub protocol_version: Option<String>,
    pub extensions: Vec<String>,
    pub checksum_algorithms: Vec<String>,
    pub disk_space: Option<RemoteDiskSpace>,
    pub server_copy: bool,
    pub symlink: bool,
    pub hardlink: bool,
}

impl ConnectionCapabilities {
    /// Return whether an SSH profile explicitly targets an interactive network
    /// device instead of a POSIX/Windows server. Missing and unknown values
    /// deliberately fall back to the legacy server behavior.
    pub fn is_network_device_profile(profile: &serde_json::Value) -> bool {
        profile.get("type").and_then(serde_json::Value::as_str) == Some("ssh")
            && profile
                .get("deviceMode")
                .and_then(serde_json::Value::as_str)
                == Some("network-device")
    }

    pub fn for_profile(profile: &serde_json::Value) -> Self {
        if Self::is_network_device_profile(profile) {
            return Self {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: true,
            };
        }

        Self::for_session_type(
            profile
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("ssh"),
        )
    }

    pub fn for_session_type(session_type: &str) -> Self {
        match session_type {
            "ssh" => Self {
                terminal: true,
                files: true,
                resource_monitoring: true,
                shell_integration: true,
                file_access: true,
                tunnels: true,
            },
            "ftp" => Self {
                terminal: false,
                files: true,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            },
            "local" => Self {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            },
            _ => Self {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            },
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub profile_id: String,
    /// Monotonic terminal-target identity. This must not change for ordinary
    /// terminal output; it changes only when the interactive target changes.
    pub ai_session_revision: String,
    /// Effective SSH session mode after banner resolution. `auto` is never
    /// exposed here: a connected session is either a normal server or a
    /// network device, while a connecting legacy snapshot keeps this empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_mode: Option<String>,
    pub access_host: String,
    pub summary: String,
    pub terminal_transcript: String,
    pub remote_path: String,
    pub shell_cwd: Option<String>,
    pub follow_shell_cwd: bool,
    pub remote_files_loading: bool,
    pub remote_files: Vec<serde_json::Value>,
    pub sftp_unavailable_reason: Option<String>,
    pub file_access_mode: String, // "user" | "root"
    pub sudo_user: Option<String>,
    pub has_reusable_sudo_auth: bool,
    /// 登录用户（首次 OSC 1337 RemoteUser= 观察到的用户，或 profile.username）。
    /// 用于判断 shell 用户是否变化以自动切 root 视角。
    pub login_user: Option<String>,
    /// 当前 shell 观察到的用户（OSC 1337 RemoteUser=）。与 sudo_user 分开：
    /// sudo_user 是用户显式配置的 sudo 目标，shell_user 是终端实际运行用户。
    pub shell_user: Option<String>,
    pub connected: bool,
    pub system_metrics: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_monitoring_unavailable_reason: Option<String>,
    pub capabilities: ConnectionCapabilities,
    pub remote_capabilities: Option<RemoteFileCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_mode: Option<String>,
}
