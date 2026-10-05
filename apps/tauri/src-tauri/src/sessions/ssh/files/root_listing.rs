/// List a directory using GNU `find -printf` when available, with a shell-glob
/// fallback for NAS images whose BusyBox `find` does not implement `-printf`.
async fn exec_list_dir_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<Vec<Value>, String> {
    let cmd = root_list_shell_command(path);
    let output =
        exec_shell_file_command(handle, &cmd, access_method, sudo_user, sudo_password).await?;
    Ok(parse_root_file_list(&output, path))
}

const PORTABLE_ROOT_LIST_SCRIPT: &str = r#"
dir=__FILETERM_DIR__
if [ ! -d "$dir" ]; then
  printf '无法访问目录: %s\n' "$dir" >&2
  exit 1
fi
if [ ! -r "$dir" ] || [ ! -x "$dir" ]; then
  printf '没有读取目录权限: %s\n' "$dir" >&2
  exit 1
fi
if stat -c '%s' / >/dev/null 2>&1; then
  stat_style=linux
elif stat -f '%z' / >/dev/null 2>&1; then
  stat_style=bsd
else
  printf '服务器缺少支持格式化输出的 stat 命令\n' >&2
  exit 1
fi
for entry in "$dir"/* "$dir"/.[!.]* "$dir"/..?*; do
  [ -e "$entry" ] || [ -L "$entry" ] || continue
  if [ -L "$entry" ]; then
    entry_type=l
    if [ -d "$entry" ]; then link_type=d; else link_type=f; fi
  elif [ -d "$entry" ]; then
    entry_type=d
    link_type=d
  else
    entry_type=f
    link_type=f
  fi
  if [ "$stat_style" = linux ]; then
    metadata=$(stat -c '%s|%Y|%U:%G|%a' "$entry") || exit 1
  else
    metadata=$(stat -f '%z|%m|%Su:%Sg|%OLp' "$entry") || exit 1
  fi
  [ -n "$metadata" ] || {
    printf '无法读取文件元数据: %s\n' "$entry" >&2
    exit 1
  }
  size=${metadata%%|*}
  metadata=${metadata#*|}
  mtime=${metadata%%|*}
  metadata=${metadata#*|}
  owner_group=${metadata%%|*}
  mode=${metadata#*|}
  name=${entry##*/}
  printf '%s|%s|%s|%s|%s|%s|%s\n' "$entry_type" "$link_type" "$size" "$mtime" "$owner_group" "$mode" "$name"
done
"#;

fn root_list_shell_command(path: &str) -> String {
    // Probe the extension independently of the requested path. If the probe
    // used that path, a permission error could be mistaken for an unsupported
    // `-printf` option and the fallback might silently show an empty folder.
    let quoted_path = shell_quote(path);
    let portable_script = PORTABLE_ROOT_LIST_SCRIPT.replace("__FILETERM_DIR__", &quoted_path);
    format!(
        "if find / -maxdepth 0 -printf '' >/dev/null 2>&1; then\n  find -H {quoted_path} -maxdepth 1 -mindepth 1 -printf '%y|%Y|%s|%T@|%u:%g|%m|%f\\n'\nelse{portable_script}\nfi",
    )
}

fn parse_root_file_list(output: &str, path: &str) -> Vec<Value> {
    let path_norm = path.trim_end_matches('/');
    let mut items = Vec::new();
    if let Some(parent_item) = parent_remote_item(path) {
        items.push(parent_item);
    }
    for line in output.lines() {
        let line = line.trim_end_matches('\n');
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(7, '|').collect();
        if parts.len() < 7 {
            continue;
        }
        let type_char = parts[0].chars().next().unwrap_or('f');
        let is_dir = type_char == 'd';
        let is_link = type_char == 'l';
        let link_target_is_dir = is_link && parts[1].starts_with('d');
        let effective_is_dir = is_dir || link_target_is_dir;
        let size_value = parts[2].parse::<u64>().unwrap_or(0);
        let size_str = if effective_is_dir {
            "-".to_string()
        } else {
            format_bytes(size_value)
        };
        let mtime: i64 = parts[3]
            .split('.')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let owner_group = parts[4].to_string();
        let perm_octal = u32::from_str_radix(parts[5], 8).unwrap_or(0o644);
        let name = parts[6].to_string();
        if name == "." || name == ".." {
            continue;
        }

        let file_type = effective_remote_file_type(is_dir, is_link, link_target_is_dir);
        let permission = format_perm(perm_octal, is_dir, is_link);
        let full_path = if path_norm.is_empty() || path_norm == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", path_norm, name)
        };
        let modified = format_unix_ts(mtime);

        items.push(serde_json::json!({
            "name": name,
            "path": full_path,
            "type": file_type,
            "isSymlink": is_link,
            "size": size_str,
            "modified": modified,
            "permission": permission,
            "ownerGroup": owner_group,
        }));
    }
    items.sort_by(|a, b| {
        let af = a["type"].as_str() == Some("folder");
        let bf = b["type"].as_str() == Some("folder");
        bf.cmp(&af).then_with(|| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        })
    });
    items
}

#[cfg(test)]
include!("root_listing_tests.rs");
