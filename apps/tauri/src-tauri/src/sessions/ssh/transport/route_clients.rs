use std::process::{
    Child as RouteClientChild, Command as RouteClientCommand, Stdio as RouteClientStdio,
};

#[derive(Clone, Copy, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SshRouteClient {
    V2ray,
    ClashVerge,
    Amnezia,
    EasyTier,
}

#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SshRouteClientConfig {
    executable: PathBuf,
    config: Option<PathBuf>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SshRouteClientEndpoint {
    #[serde(rename = "type")]
    kind: &'static str,
    host: std::net::IpAddr,
    port: u16,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SshRouteClientStatus {
    kind: SshRouteClient,
    running: bool,
    last_exit: Option<i32>,
    endpoints: Vec<SshRouteClientEndpoint>,
}

#[derive(Default)]
struct SshRouteClientProcess {
    child: Option<RouteClientChild>,
    last_exit: Option<i32>,
}

static SSH_ROUTE_CLIENTS: LazyLock<StdMutex<HashMap<SshRouteClient, SshRouteClientProcess>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

impl SshRouteClient {
    fn name(self) -> &'static str {
        match self {
            Self::V2ray => "v2ray",
            Self::ClashVerge => "clash-verge",
            Self::Amnezia => "amnezia",
            Self::EasyTier => "easytier",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::V2ray => "V2Ray",
            Self::ClashVerge => "Clash Verge",
            Self::Amnezia => "AmneziaVPN",
            Self::EasyTier => "EasyTier",
        }
    }

    fn is_desktop_gui(self) -> bool {
        matches!(self, Self::ClashVerge | Self::Amnezia)
    }

    fn executable_names(self) -> &'static [&'static str] {
        match self {
            Self::V2ray => &["v2ray"],
            Self::ClashVerge => &["clash-verge", "clash-verge-rev"],
            Self::Amnezia => &["amneziavpn", "amnezia-vpn"],
            Self::EasyTier => &["easytier-core"],
        }
    }
}

fn route_client_local_file(path: &Path) -> Result<PathBuf, crate::AppError> {
    use crate::AppError;
    if !path.is_absolute() || path.to_string_lossy().starts_with(r"\\") {
        return Err(AppError::Command(
            "Choose an absolute local file path".into(),
        ));
    }
    let path = path
        .canonicalize()
        .map_err(|_| AppError::Command("Client file could not be opened".into()))?;
    if !path.is_file() || path.to_string_lossy().starts_with(r"\\?\UNC\") {
        return Err(AppError::Command(
            "Choose an absolute local file path".into(),
        ));
    }
    Ok(path)
}

fn make_ssh_route_client_command(
    kind: SshRouteClient,
    definition: &SshRouteClientConfig,
) -> Result<RouteClientCommand, crate::AppError> {
    use crate::AppError;
    let executable = route_client_local_file(&definition.executable)?;
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !kind.executable_names().iter().any(|expected| {
        if cfg!(windows) {
            name.strip_suffix(".exe") == Some(expected)
        } else {
            name == *expected
        }
    }) {
        return Err(AppError::Command(
            "The executable name does not match the chosen client".into(),
        ));
    }
    let mut process = RouteClientCommand::new(&executable);
    if let Some(parent) = executable.parent() {
        process.current_dir(parent);
    }
    if !kind.is_desktop_gui() {
        let config = definition.config.as_ref().ok_or_else(|| {
            AppError::Command("Choose a configuration file for this client".into())
        })?;
        let config = route_client_local_file(config)?;
        match kind {
            SshRouteClient::V2ray => {
                process.args(["run", "-c"]).arg(config);
            }
            SshRouteClient::EasyTier => {
                process.arg("--config-file").arg(config);
            }
            SshRouteClient::ClashVerge | SshRouteClient::Amnezia => unreachable!(),
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
    }
    process
        .stdin(RouteClientStdio::null())
        .stdout(RouteClientStdio::null())
        .stderr(RouteClientStdio::null());
    Ok(process)
}

fn inspect_route_client(process: &mut SshRouteClientProcess) {
    if let Some(status) = process
        .child
        .as_mut()
        .and_then(|child| child.try_wait().ok())
        .flatten()
    {
        process.last_exit = status.code();
        process.child = None;
    }
}

fn v2ray_route_endpoints(config: &Path) -> Vec<SshRouteClientEndpoint> {
    let Ok(config) = route_client_local_file(config) else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(config) else {
        return Vec::new();
    };
    let Ok(config) = serde_json::from_slice::<Value>(&bytes) else {
        return Vec::new();
    };
    config["inbounds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|inbound| {
            let kind = match inbound["protocol"].as_str()? {
                "socks" => "socks5",
                "http" => "http",
                _ => return None,
            };
            let host = inbound["listen"]
                .as_str()?
                .parse::<std::net::IpAddr>()
                .ok()?;
            let port = inbound["port"].as_u64()?.try_into().ok()?;
            // Automatic import supports explicitly loopback-bound, credential-free inbounds.
            let settings = &inbound["settings"];
            let authentication = settings["auth"].as_str().unwrap_or("noauth");
            let accounts = settings["accounts"].as_array();
            if !host.is_loopback()
                || port == 0
                || authentication != "noauth"
                || accounts.is_some_and(|accounts| !accounts.is_empty())
            {
                return None;
            }
            Some(SshRouteClientEndpoint { kind, host, port })
        })
        .take(16)
        .collect()
}

fn ssh_route_client_status(policy: &SshRoutePolicy) -> Vec<SshRouteClientStatus> {
    let mut processes = SSH_ROUTE_CLIENTS.lock().expect("SSH route clients lock");
    [
        SshRouteClient::V2ray,
        SshRouteClient::ClashVerge,
        SshRouteClient::Amnezia,
        SshRouteClient::EasyTier,
    ]
    .into_iter()
    .map(|kind| {
        let process = processes.entry(kind).or_default();
        inspect_route_client(process);
        let endpoints = if kind == SshRouteClient::V2ray {
            policy
                .clients
                .get(&kind)
                .and_then(|config| config.config.as_deref())
                .map(v2ray_route_endpoints)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        SshRouteClientStatus {
            kind,
            running: process.child.is_some(),
            last_exit: process.last_exit,
            endpoints,
        }
    })
    .collect()
}

pub(crate) fn control_ssh_route_client(
    app: &AppHandle,
    kind: SshRouteClient,
    start: bool,
) -> Result<SshRoutePolicySnapshot, crate::AppError> {
    use crate::AppError;
    let path = crate::storage::workspace_file(app, SSH_ROUTE_POLICY_FILE)?;
    let config = read_route_policy_file(&path)?;
    {
        let mut processes = SSH_ROUTE_CLIENTS.lock().expect("SSH route clients lock");
        let process = processes.entry(kind).or_default();
        inspect_route_client(process);
        if start && process.child.is_none() {
            let definition = config.clients.get(&kind).ok_or_else(|| {
                AppError::Command("Save the client paths before starting it".into())
            })?;
            let mut command = make_ssh_route_client_command(kind, definition)?;
            process.child = Some(
                command
                    .spawn()
                    .map_err(|error| AppError::Command(format!("Client start failed: {error}")))?,
            );
            process.last_exit = None;
        } else if !start {
            if kind.is_desktop_gui() {
                return Err(AppError::Command(format!(
                    "Close {} from its own interface",
                    kind.display_name()
                )));
            }
            if let Some(child) = process.child.as_mut() {
                child
                    .kill()
                    .map_err(|error| AppError::Command(format!("Client stop failed: {error}")))?;
                process.last_exit = child.wait().ok().and_then(|status| status.code());
                process.child = None;
            }
        }
    }
    crate::services::logging::info(
        app,
        "ssh-route-client",
        format!(
            "client={} action={}",
            kind.name(),
            if start { "start" } else { "stop" }
        ),
    );
    ssh_route_policy_snapshot(app)
}

pub(crate) fn stop_owned_ssh_route_clients() {
    let mut processes = SSH_ROUTE_CLIENTS.lock().expect("SSH route clients lock");
    for (kind, process) in processes.iter_mut() {
        if !kind.is_desktop_gui() {
            if let Some(mut child) = process.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
