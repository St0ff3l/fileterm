const COMMAND_BATCH_HISTORY: &str = "command-batches.json";
const COMMAND_BATCH_HISTORY_LIMIT: usize = 30;
const COMMAND_BATCH_TARGET_LIMIT: usize = 16;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandBatchTarget {
    profile_id: String,
    tab_id: String,
    session_revision: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum CommandBatchStatus {
    Unknown,
    Success,
    Failed,
    TimedOut,
    InputRequired,
    Error,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandBatchResult {
    profile_id: String,
    name: String,
    status: CommandBatchStatus,
    exit_code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    output_truncated: bool,
}

impl CommandBatchResult {
    fn apply(&mut self, result: Result<crate::services::action_review::RemoteExecResult, AppError>) {
        match result {
            Ok(result) => {
                self.status = if result.timed_out {
                    CommandBatchStatus::TimedOut
                } else {
                    match result.exit_code {
                        Some(0) => CommandBatchStatus::Success,
                        Some(_) => CommandBatchStatus::Failed,
                        None if result.input_required => CommandBatchStatus::InputRequired,
                        None => CommandBatchStatus::Unknown,
                    }
                };
                self.exit_code = result.exit_code;
                self.output_truncated = result.output_truncated;
                self.output = Some(result.output);
            }
            Err(error) => {
                self.status = CommandBatchStatus::Error;
                self.error = Some(error.to_string());
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandBatchRun {
    id: String,
    template_name: String,
    started_at: i64,
    finished_at: Option<i64>,
    results: Vec<CommandBatchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    history_error: Option<String>,
}

fn command_batch_history_entry(run: &CommandBatchRun) -> Result<Value, AppError> {
    let mut value = serde_json::to_value(run)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    if let Some(results) = value["results"].as_array_mut() {
        for result in results.iter_mut().filter_map(Value::as_object_mut) {
            result.remove("output");
            result.remove("error");
        }
    }
    Ok(value)
}

fn render_batch_command(template: &str, args: &BTreeMap<usize, String>) -> Result<String, AppError> {
    use crate::services::action_review::MAX_REMOTE_EXEC_COMMAND_BYTES as LIMIT;
    let size_error = || AppError::Command(format!("The command exceeds the {LIMIT}-byte limit"));
    if template.len() > LIMIT {
        return Err(size_error());
    }
    let mut inserted = 0_usize;
    let command = render_command_template_with(template, |index| {
        let value = args.get(&index)?;
        inserted = inserted.saturating_add(value.len());
        (inserted <= LIMIT).then_some(value)
    });
    if inserted > LIMIT || command.len() > LIMIT {
        return Err(size_error());
    }
    if command.trim().is_empty() {
        return Err(AppError::Command("The command is empty".to_string()));
    }
    Ok(command)
}

#[tauri::command]
pub fn app_list_command_batches(app: AppHandle) -> Result<Vec<Value>, AppError> {
    let mut history = read_json_array(&app, COMMAND_BATCH_HISTORY)?;
    history.truncate(COMMAND_BATCH_HISTORY_LIMIT);
    Ok(history)
}

#[tauri::command]
pub async fn app_run_command_batch(
    app: AppHandle,
    window: WebviewWindow,
    command_id: String,
    expected_template: String,
    args: BTreeMap<usize, String>,
    targets: Vec<CommandBatchTarget>,
) -> Result<CommandBatchRun, AppError> {
    use crate::services::action_review::{execute_background_remote_command, RemoteExecRequest};
    use crate::services::workspace::WorkspaceState;
    use crate::storage::write_json_array;

    if window.label() != "main" {
        return Err(AppError::Command("Command batches require the main window".to_string()));
    }
    if targets.is_empty() || targets.len() > COMMAND_BATCH_TARGET_LIMIT {
        return Err(AppError::Command(format!(
            "Select between 1 and {COMMAND_BATCH_TARGET_LIMIT} SSH servers"
        )));
    }
    let mut profiles = std::collections::HashSet::with_capacity(targets.len());
    if targets.iter().any(|target| !profiles.insert(&target.profile_id)) {
        return Err(AppError::Command("A server is selected more than once".to_string()));
    }
    let state = app.state::<WorkspaceState>();
    let _batch = state.command_batch.try_lock().map_err(|_| {
        AppError::Command("Another command batch is still running".to_string())
    })?;

    let commands = read_json_array(&app, "commands.json")?;
    let saved = commands
        .iter()
        .find(|item| item["id"].as_str() == Some(command_id.as_str()))
        .ok_or_else(|| AppError::Command("The command template was deleted".to_string()))?;
    if saved["command"].as_str() != Some(expected_template.as_str()) {
        return Err(AppError::Command("The command template changed. Review it again.".to_string()));
    }
    let command = render_batch_command(&expected_template, &args)?;

    let mut requests = Vec::with_capacity(targets.len());
    let mut results = Vec::with_capacity(targets.len());
    {
        let tabs = state.tabs.read().await;
        for target in targets {
            let tab = tabs.iter().find(|tab| {
                tab.id == target.tab_id
                    && tab.profile_id == target.profile_id
                    && tab.session_type == "ssh"
                    && (!tab.is_background || tab.source.is_none())
            });
            let name = tab.map(|tab| tab.title.clone()).unwrap_or_else(|| target.profile_id.clone());
            let mut result = CommandBatchResult {
                profile_id: target.profile_id,
                name,
                status: CommandBatchStatus::Unknown,
                exit_code: None,
                output: None,
                error: None,
                output_truncated: false,
            };
            if tab.is_some() {
                requests.push((results.len(), RemoteExecRequest {
                    tab_id: target.tab_id,
                    command: command.clone(),
                    cwd: None,
                    timeout_ms: None,
                    expected_session_revision: Some(target.session_revision),
                    sudo_password: None,
                    su_password: None,
                    save_sudo_password: false,
                    save_su_password: false,
                    allow_local_privileged_prompt: false,
                    privileged_prompt_notice: None,
                }));
            } else {
                result.apply(Err(AppError::Command("The selected SSH session is no longer available".to_string())));
            }
            results.push(result);
        }
    }
    let mut run = CommandBatchRun {
        id: uuid::Uuid::new_v4().to_string(),
        template_name: saved["name"].as_str().unwrap_or_default().to_string(),
        started_at: chrono::Utc::now().timestamp_millis(),
        finished_at: None,
        results,
        history_error: None,
    };
    // Record the attempt before dispatch so an interrupted run is visible without retrying it.
    let mut history = app_list_command_batches(app.clone())?;
    history.insert(0, command_batch_history_entry(&run)?);
    history.truncate(COMMAND_BATCH_HISTORY_LIMIT);
    write_json_array(&app, COMMAND_BATCH_HISTORY, &history)?;

    let mut running = tokio::task::JoinSet::new();
    for (index, request) in requests {
        let app = app.clone();
        running.spawn(async move { (index, execute_background_remote_command(&app, request).await) });
    }
    while let Some(result) = running.join_next().await {
        if let Ok((index, result)) = result {
            run.results[index].apply(result);
        }
    }
    run.finished_at = Some(chrono::Utc::now().timestamp_millis());
    let saved = command_batch_history_entry(&run).and_then(|entry| {
        history[0] = entry;
        write_json_array(&app, COMMAND_BATCH_HISTORY, &history)
    });
    if let Err(error) = saved {
        run.history_error = Some(error.to_string());
    }
    Ok(run)
}

#[cfg(test)]
mod command_batch_tests {
    use super::*;
    use crate::services::action_review::RemoteExecResult;

    #[test]
    fn sparse_template_parameters_keep_the_library_numbering_and_limit_expansion() {
        let args = BTreeMap::from([(3, "value".to_string())]);
        assert_eq!(
            render_batch_command("printf '%s' [p#3][p#1][p#0]", &args).unwrap(),
            "printf '%s' value"
        );
        let oversized = BTreeMap::from([(3, "x".repeat(40_000))]);
        assert!(render_batch_command("[p#3][p#3]", &oversized).is_err());
        assert!(render_batch_command("[p#99]", &args).is_err());
    }

    #[test]
    fn mixed_results_keep_real_exit_codes_and_history_omits_live_details() {
        let test_output = |exit_code, timed_out, input_required| RemoteExecResult {
            output: "private remote output".to_string(),
            exit_code,
            timed_out,
            output_truncated: false,
            raw_terminal: false,
            input_required,
            input_kind: None,
        };
        let results = [
            Ok(test_output(Some(0), false, false)),
            Ok(test_output(Some(7), false, false)),
            Ok(test_output(None, true, false)),
            Ok(test_output(None, false, true)),
            Err(AppError::Command("private failure details".to_string())),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, outcome)| {
            let mut result = CommandBatchResult {
                profile_id: format!("profile-{index}"),
                name: format!("Host {index}"),
                status: CommandBatchStatus::Unknown,
                exit_code: None,
                output: None,
                error: None,
                output_truncated: false,
            };
            result.apply(outcome);
            result
        })
        .collect();
        let run = CommandBatchRun {
            id: "batch".to_string(),
            template_name: "Check".to_string(),
            started_at: 1,
            finished_at: Some(2),
            results,
            history_error: None,
        };
        let stored = command_batch_history_entry(&run).unwrap();
        let stored = stored["results"].as_array().unwrap();
        for (index, status) in ["success", "failed", "timedOut", "inputRequired", "error"]
            .iter()
            .enumerate()
        {
            assert_eq!(stored[index]["status"], *status);
            assert!(stored[index].get("output").is_none());
            assert!(stored[index].get("error").is_none());
        }
        assert_eq!(stored[0]["exitCode"], 0);
        assert_eq!(stored[1]["exitCode"], 7);
        assert!(stored[2]["exitCode"].is_null());
    }
}
