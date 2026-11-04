use std::path::PathBuf;

use super::*;

struct TempHome(PathBuf);

impl TempHome {
    fn new(config: &str) -> Self {
        let home =
            std::env::temp_dir().join(format!("fileterm-ssh-config-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh").join("config"), config).unwrap();
        Self(home)
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn environment(&self) -> SshConfigEnvironment {
        SshConfigEnvironment {
            home: self.0.clone(),
            local_user: "local".to_string(),
            local_hostname: "workstation.example".to_string(),
            user_config: self.0.join(".ssh").join("config"),
            system_config: None,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn resolve(home: &TempHome, alias: &str) -> ResolvedSshHost {
    resolve_host(alias, &SshConfigOverrides::default(), &home.environment()).unwrap()
}

const AGENT_STYLE_CONFIG: &str = r#"
Host *
    IdentitiesOnly yes
    IdentityFile ~/.keys/%n.pub
    ForwardAgent yes

Host git.example.com
    HostName 192.0.2.10
    User git
    IdentityFile ~/.keys/app-git.pub

Host lab-dev
    HostName 192.0.2.20
    User alice
    Port 48699

Host lab-inner
    HostName 10.0.0.5
    User bob
    ProxyJump lab-dev
"#;

#[test]
fn first_value_wins_and_identity_files_accumulate() {
    let home = TempHome::new(AGENT_STYLE_CONFIG);
    let host = resolve(&home, "git.example.com");
    assert_eq!(host.hostname, "192.0.2.10");
    assert_eq!(host.user, "git");
    assert_eq!(host.port, 22);
    assert!(host.identities_only);
    assert_eq!(
        host.identity_files,
        vec![
            home.path(".keys/git.example.com.pub"),
            home.path(".keys/app-git.pub")
        ]
    );
}

#[test]
fn port_user_and_alias_token_expand_for_simple_host() {
    let home = TempHome::new(AGENT_STYLE_CONFIG);
    let host = resolve(&home, "lab-dev");
    assert_eq!(host.hostname, "192.0.2.20");
    assert_eq!(host.port, 48699);
    assert_eq!(host.user, "alice");
    assert_eq!(host.identity_files, vec![home.path(".keys/lab-dev.pub")]);
    assert_eq!(host.agent, SshConfigAgent::Default);
}

#[test]
fn unknown_alias_falls_back_to_alias_as_hostname_and_local_user() {
    let home = TempHome::new(AGENT_STYLE_CONFIG);
    let host = resolve(&home, "Unlisted.Example");
    assert_eq!(host.hostname, "Unlisted.Example");
    assert_eq!(host.user, "local");
    assert_eq!(host.port, 22);
    assert_eq!(
        host.identity_files,
        vec![home.path(".keys/Unlisted.Example.pub")]
    );
}

#[test]
fn overrides_take_precedence_like_command_line() {
    let home = TempHome::new(AGENT_STYLE_CONFIG);
    let host = resolve_host(
        "lab-dev",
        &SshConfigOverrides {
            user: Some("root".to_string()),
            port: None,
            port_fallback: Some(2200),
        },
        &home.environment(),
    )
    .unwrap();
    assert_eq!(host.user, "root");
    // Config Port wins over the profile fallback port.
    assert_eq!(host.port, 48699);
    let unlisted = resolve_host(
        "other",
        &SshConfigOverrides {
            user: Some(" ".to_string()),
            port: None,
            port_fallback: Some(2200),
        },
        &home.environment(),
    )
    .unwrap();
    assert_eq!(unlisted.port, 2200);
    assert_eq!(unlisted.user, "local");
}

#[test]
fn proxy_jump_builds_route_with_jump_resolved_from_config() {
    let home = TempHome::new(AGENT_STYLE_CONFIG);
    let route = resolve_route(
        "lab-inner",
        &SshConfigOverrides::default(),
        &home.environment(),
    )
    .unwrap();
    let summary = route
        .iter()
        .map(|hop| {
            (
                hop.alias.as_str(),
                hop.hostname.as_str(),
                hop.port,
                hop.user.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        vec![
            ("lab-dev", "192.0.2.20", 48699, "alice"),
            ("lab-inner", "10.0.0.5", 22, "bob")
        ]
    );
}

#[test]
fn proxy_jump_list_and_inline_user_port() {
    let home = TempHome::new(
        "Host target\n  HostName 10.1.1.1\n  ProxyJump ops@first:2022,second\nHost second\n  HostName 10.2.2.2\n  ProxyJump ignored\n",
    );
    let route = resolve_route(
        "target",
        &SshConfigOverrides::default(),
        &home.environment(),
    )
    .unwrap();
    assert_eq!(route.len(), 3);
    assert_eq!(
        (
            route[0].hostname.as_str(),
            route[0].port,
            route[0].user.as_str()
        ),
        ("first", 2022, "ops")
    );
    // Later hops in a ProxyJump list do not follow their own ProxyJump.
    assert_eq!(route[1].hostname, "10.2.2.2");
    assert_eq!(route[2].hostname, "10.1.1.1");
}

#[test]
fn proxy_jump_cycle_is_rejected() {
    let home = TempHome::new("Host a\n  ProxyJump b\nHost b\n  ProxyJump a\n");
    let error =
        resolve_route("a", &SshConfigOverrides::default(), &home.environment()).unwrap_err();
    assert!(error.contains("cycle"), "{error}");
}

#[test]
fn proxy_command_without_jump_is_reported() {
    let home = TempHome::new("Host a\n  ProxyCommand nc %h %p\n");
    let error =
        resolve_route("a", &SshConfigOverrides::default(), &home.environment()).unwrap_err();
    assert!(error.contains("ProxyCommand"), "{error}");
}

#[test]
fn negated_patterns_equals_syntax_and_quotes() {
    let home = TempHome::new(
        "Host *.corp !bastion.corp\n  User=corp-user\nHost \"bastion.corp\"\n  IdentityAgent \"~/agent dir/sock\"\n",
    );
    assert_eq!(resolve(&home, "web.corp").user, "corp-user");
    let bastion = resolve(&home, "bastion.corp");
    assert_eq!(bastion.user, "local");
    assert_eq!(
        bastion.agent,
        SshConfigAgent::Socket(home.path("agent dir/sock").to_string_lossy().into_owned())
    );
}

#[test]
fn identity_agent_none_disables_agent() {
    let home = TempHome::new("Host a\n  IdentityAgent none\n");
    assert_eq!(resolve(&home, "a").agent, SshConfigAgent::Disabled);
}

#[test]
fn include_with_glob_is_processed_in_place() {
    let home = TempHome::new("Include conf.d/*.conf\nHost *\n  User fallback\n");
    home.write(
        ".ssh/conf.d/10-lab.conf",
        "Host lab\n  HostName 192.0.2.30\n  User included\n",
    );
    home.write(".ssh/conf.d/ignored.txt", "Host lab\n  User wrong\n");
    let host = resolve(&home, "lab");
    assert_eq!(host.hostname, "192.0.2.30");
    assert_eq!(host.user, "included");
    assert_eq!(resolve(&home, "elsewhere").user, "fallback");
}

#[test]
fn match_blocks_support_host_and_all() {
    let home = TempHome::new(
        "Host short\n  HostName short.example.net\nMatch host *.example.net\n  User matched\nMatch exec \"true\"\n  Port 2\nMatch all\n  Port 2201\n",
    );
    let host = resolve(&home, "short");
    assert_eq!(host.user, "matched");
    assert_eq!(host.port, 2201);
}

#[test]
fn known_hosts_defaults_and_overrides() {
    let home = TempHome::new(
        "Host custom\n  UserKnownHostsFile ~/kh1 %d/kh2\nHost off\n  UserKnownHostsFile none\n",
    );
    assert_eq!(
        resolve(&home, "custom").known_hosts_files,
        vec![home.path("kh1"), home.path("kh2")]
    );
    assert!(resolve(&home, "off").known_hosts_files.is_empty());
    assert_eq!(
        resolve(&home, "plain").known_hosts_files,
        vec![
            home.path(".ssh/known_hosts"),
            home.path(".ssh/known_hosts2")
        ]
    );
}

#[test]
fn list_hosts_skips_wildcards_and_duplicates() {
    let home = TempHome::new(&format!(
        "{AGENT_STYLE_CONFIG}\nHost lab-dev\n  User duplicate\n"
    ));
    let hosts = list_hosts(&home.environment());
    let aliases = hosts
        .iter()
        .map(|host| host.alias.as_str())
        .collect::<Vec<_>>();
    assert_eq!(aliases, vec!["git.example.com", "lab-dev", "lab-inner"]);
    let inner = hosts.iter().find(|host| host.alias == "lab-inner").unwrap();
    assert_eq!(inner.proxy_jump.as_deref(), Some("lab-dev"));
    assert_eq!(inner.user, "bob");
}

#[test]
fn missing_config_resolves_alias_directly() {
    let home = TempHome::new("");
    std::fs::remove_file(home.path(".ssh/config")).unwrap();
    let host = resolve(&home, "192.0.2.99");
    assert_eq!(host.hostname, "192.0.2.99");
    assert!(host.identity_files.is_empty());
    assert!(!host.identities_only);
    assert!(list_hosts(&home.environment()).is_empty());
}

#[test]
fn wildcard_matching() {
    assert!(parse::wildcard_match("*", "anything"));
    assert!(parse::wildcard_match("inc-*", "inc-dev"));
    assert!(parse::wildcard_match("h?st", "host"));
    assert!(!parse::wildcard_match("inc-*", "idc-aws"));
    assert!(parse::wildcard_match("*.corp", "a.b.corp"));
}

#[cfg(unix)]
#[test]
fn expand_user_path_handles_tilde() {
    let home = std::path::Path::new("/home/someone");
    assert_eq!(
        expand_user_path(" ~/.bitwarden-ssh-agent.sock ", home),
        PathBuf::from("/home/someone/.bitwarden-ssh-agent.sock")
    );
    assert_eq!(
        expand_user_path("/tmp/agent", home),
        PathBuf::from("/tmp/agent")
    );
}

#[test]
fn forward_agent_follows_identity_agent_and_first_value() {
    let home = TempHome::new(AGENT_STYLE_CONFIG);
    assert_eq!(
        resolve(&home, "lab-dev").forward_agent,
        Some(SshConfigAgent::Default)
    );

    let home = TempHome::new(
        "Host off\n  ForwardAgent no\nHost sock\n  IdentityAgent ~/agent.sock\n  ForwardAgent yes\nHost path\n  ForwardAgent ~/forward.sock\nHost noagent\n  IdentityAgent none\n  ForwardAgent yes\nHost *\n  ForwardAgent yes\n",
    );
    assert_eq!(resolve(&home, "off").forward_agent, None);
    assert_eq!(
        resolve(&home, "sock").forward_agent,
        Some(SshConfigAgent::Socket(
            home.path("agent.sock").to_string_lossy().into_owned()
        ))
    );
    assert_eq!(
        resolve(&home, "path").forward_agent,
        Some(SshConfigAgent::Socket(
            home.path("forward.sock").to_string_lossy().into_owned()
        ))
    );
    assert_eq!(resolve(&home, "noagent").forward_agent, None);
    assert_eq!(
        resolve(&home, "other").forward_agent,
        Some(SshConfigAgent::Default)
    );
}

#[test]
fn proxy_directives_share_first_value_precedence() {
    for (directives, expected_hops, rejected) in [
        ("ProxyCommand nc %h %p\n  ProxyJump bastion", 0, true),
        ("ProxyJump bastion\n  ProxyCommand nc %h %p", 2, false),
        ("ProxyCommand none\n  ProxyJump bastion", 1, false),
        ("ProxyJump none\n  ProxyCommand nc %h %p", 0, true),
    ] {
        let home = TempHome::new(&format!("Host target\n  {directives}\n"));
        let result = resolve_route(
            "target",
            &SshConfigOverrides::default(),
            &home.environment(),
        );
        if rejected {
            assert!(result.unwrap_err().contains("ProxyCommand"), "{directives}");
        } else {
            assert_eq!(result.unwrap().len(), expected_hops, "{directives}");
        }
    }
}

#[test]
fn identity_file_none_and_missing_explicit_keys_disable_default_files() {
    let home =
        TempHome::new("Host none\n  IdentityFile none\nHost explicit\n  IdentityFile ~/missing\n");
    let none = resolve(&home, "none");
    assert!(none.identity_files.is_empty());
    assert!(!none.use_default_identity_files);
    assert!(!resolve(&home, "explicit").use_default_identity_files);
    assert!(resolve(&home, "default").use_default_identity_files);
}

#[test]
fn strict_host_key_checking_uses_first_matching_setting() {
    let home = TempHome::new(
        "Host strict\n  StrictHostKeyChecking yes\nHost *\n  StrictHostKeyChecking ask\n",
    );
    assert!(resolve(&home, "strict").strict_host_key_checking);
    assert!(!resolve(&home, "other").strict_host_key_checking);
}
