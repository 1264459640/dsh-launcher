use crate::config::{Config, InstanceState, InstanceStatus};
use crate::AppState;
use regex::Regex;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

pub const STATUS_EVENT: &str = "instance://status";

/// A live instance process.
pub struct RunningInstance {
    /// Child handle shared with the waiter task. `None` once the process was
    /// taken for waiting/killing.
    pub child: Arc<Mutex<Option<tokio::process::Child>>>,
    pub profile: String,
    pub url: Option<String>,
}

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"dsh web: (https?://[^\s]+)").unwrap())
}

pub fn npm() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

pub fn node() -> &'static str {
    "node"
}

pub fn version_bin(version_dir: &PathBuf) -> PathBuf {
    version_dir
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("lib")
        .join("bin.js")
}

/// Builds the effective environment for an instance: DSH_HOME (from the
/// instance's home), the launcher marker, then the user's overrides
/// (DSH_HOME is reserved and never overridden).
pub fn build_env(cfg: &Config, instance_id: &str) -> Result<Vec<(String, String)>, String> {
    let inst = cfg
        .instances
        .iter()
        .find(|i| i.id == instance_id)
        .ok_or_else(|| "实例不存在".to_string())?;
    let home = cfg
        .homes
        .iter()
        .find(|h| h.id == inst.home_id)
        .ok_or_else(|| "DSH_HOME 不存在".to_string())?;

    let mut env: Vec<(String, String)> = Vec::new();
    env.push(("DSH_HOME".to_string(), home.path.to_string_lossy().to_string()));
    env.push(("DSH_LAUNCHER_INSTANCE".to_string(), inst.name.clone()));
    for (k, v) in &inst.env_overrides {
        if k == "DSH_HOME" {
            continue; // reserved
        }
        env.push((k.clone(), v.clone()));
    }
    Ok(env)
}

/// Spawns a DSH CLI process for the instance/profile and starts the watchers
/// that parse the web URL, write logs and emit status events.
pub async fn start_instance_process(
    app: &AppHandle,
    state: &State<'_, AppState>,
    instance_id: &str,
    profile: &str,
) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    let inst = cfg
        .instances
        .iter()
        .find(|i| i.id == instance_id)
        .cloned()
        .ok_or_else(|| "实例不存在".to_string())?;
    let version = cfg
        .versions
        .iter()
        .find(|v| v.id == inst.version_id)
        .ok_or_else(|| "实例引用的 DSH 版本未安装".to_string())?;

    let bin = version_bin(&version.dir);
    if !bin.exists() {
        return Err(format!(
            "版本 {} 安装不完整（缺少 {}），请重新安装",
            version.version,
            bin.display()
        ));
    }

    // Guard: already running/starting.
    {
        let running = state.running.lock().await;
        if running.contains_key(instance_id) {
            return Err("实例已在运行".to_string());
        }
    }

    let mut cmd = Command::new(node());
    cmd.arg(&bin).arg("--profile").arg(profile);
    // Only the web app understands --host/--port; other profiles are managed
    // purely as processes (no URL/webview).
    if profile == "web" {
        cmd.arg("--host").arg("127.0.0.1").arg("--port").arg("0");
    }

    let env = build_env(&cfg, instance_id)?;
    for (k, v) in env {
        cmd.env(k, v);
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("启动进程失败: {e}"))?;

    // Take the pipes before wrapping the child (the waiter takes ownership of
    // the child itself).
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let log_dir = state.data_dir.join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    let log_path = log_dir.join(format!("{instance_id}.log"));
    let log_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap_or_else(|_| {
                // Fallback: discard output if logging fails.
                OpenOptions::new()
                    .write(true)
                    .open(std::path::Path::new("NUL"))
                    .unwrap_or_else(|_| panic!("cannot open log file"))
            }),
    ));

    let shared_child: Arc<Mutex<Option<tokio::process::Child>>> =
        Arc::new(Mutex::new(Some(child)));

    emit_status(
        app,
        &InstanceStatus {
            id: instance_id.to_string(),
            state: InstanceState::Starting,
            url: None,
            profile: Some(profile.to_string()),
            exit_code: None,
        },
    );

    // Register the running entry (stop_instance reaches it through the map).
    state.running.lock().await.insert(
        instance_id.to_string(),
        RunningInstance {
            child: shared_child.clone(),
            profile: profile.to_string(),
            url: None,
        },
    );
    crate::tray::rebuild_tray_menu(app).await;

    // Waiter: awaits process exit, then cleans up and notifies.
    {
        let waiter_child = shared_child.clone();
        let waiter_app = app.clone();
        let waiter_id = instance_id.to_string();
        let waiter_profile = profile.to_string();
        tauri::async_runtime::spawn(async move {
            let state = waiter_app.state::<AppState>();
            let taken = waiter_child.lock().await.take();
            let code = if let Some(mut c) = taken {
                c.wait().await.ok().and_then(|s| s.code())
            } else {
                None
            };
            state.running.lock().await.remove(&waiter_id);
            emit_status(
                &waiter_app,
                &InstanceStatus {
                    id: waiter_id.clone(),
                    state: InstanceState::Exited,
                    url: None,
                    profile: Some(waiter_profile),
                    exit_code: code,
                },
            );
            crate::tray::rebuild_tray_menu(&waiter_app).await;
            crate::windows::close_instance_window(&waiter_app, &waiter_id);
        });
    }

    // stdout watcher: parse `dsh web: <url>` and emit "running".
    if let Some(out) = stdout {
        let reader_app = app.clone();
        let reader_id = instance_id.to_string();
        let reader_profile = profile.to_string();
        let reader_log = log_file.clone();
        tauri::async_runtime::spawn(async move {
            let state = reader_app.state::<AppState>();
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log_line(&reader_log, &line).await;
                if let Some(cap) = url_re().captures(&line) {
                    let url = cap[1].to_string();
                    {
                        let mut running = state.running.lock().await;
                        if let Some(entry) = running.get_mut(&reader_id) {
                            entry.url = Some(url.clone());
                        }
                    }
                    emit_status(
                        &reader_app,
                        &InstanceStatus {
                            id: reader_id.clone(),
                            state: InstanceState::Running,
                            url: Some(url),
                            profile: Some(reader_profile.clone()),
                            exit_code: None,
                        },
                    );
                }
            }
        });
    }

    // stderr watcher: forward to the log (diagnostics).
    if let Some(err) = stderr {
        let reader_log = log_file.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log_line(&reader_log, &line).await;
            }
        });
    }

    Ok(())
}

/// Stops a running instance: kills the child and emits the terminal event.
pub async fn stop_instance_process(
    app: &AppHandle,
    state: &State<'_, AppState>,
    instance_id: &str,
) -> Result<(), String> {
    let entry = state
        .running
        .lock()
        .await
        .get(instance_id)
        .map(|r| r.child.clone());
    let Some(child) = entry else {
        return Err("实例未在运行".to_string());
    };
    let taken = child.lock().await.take();
    if let Some(mut c) = taken {
        let _ = c.kill().await;
        let _ = c.wait().await;
    }
    state.running.lock().await.remove(instance_id);
    emit_status(
        app,
        &InstanceStatus {
            id: instance_id.to_string(),
            state: InstanceState::Stopped,
            url: None,
            profile: None,
            exit_code: Some(0),
        },
    );
    crate::tray::rebuild_tray_menu(app).await;
    crate::windows::close_instance_window(app, instance_id);
    Ok(())
}

/// Kills every running instance (called on launcher exit). Best-effort.
pub fn kill_all(state: &AppState) {
    let running = state.running.blocking_lock();
    for entry in running.values() {
        let taken = entry.child.blocking_lock().take();
        if let Some(mut c) = taken {
            let _ = c.start_kill();
        }
    }
}

pub async fn list_statuses(state: &State<'_, AppState>) -> Vec<InstanceStatus> {
    let running = state.running.lock().await;
    running
        .iter()
        .map(|(id, entry)| InstanceStatus {
            id: id.clone(),
            state: InstanceState::Running,
            url: entry.url.clone(),
            profile: Some(entry.profile.clone()),
            exit_code: None,
        })
        .collect()
}

async fn log_line(log: &Arc<Mutex<std::fs::File>>, line: &str) {
    let mut f = log.lock().await;
    let _ = writeln!(f, "{line}");
    let _ = f.flush();
}

fn emit_status(app: &AppHandle, status: &InstanceStatus) {
    let _ = app.emit(STATUS_EVENT, status);
}
