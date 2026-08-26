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

pub fn pnpm() -> &'static str {
    if cfg!(windows) {
        "pnpm.cmd"
    } else {
        "pnpm"
    }
}

pub fn node() -> &'static str {
    "node"
}

/// Hides the console window on Windows (CREATE_NO_WINDOW) so spawning
/// npm.cmd / pnpm.cmd / node.exe never flashes a terminal next to the
/// launcher GUI. No-op on other platforms.
pub fn hide_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

pub fn version_bin(version_dir: &std::path::Path) -> PathBuf {
    version_dir
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("lib")
        .join("bin.js")
}

/// Checks that a version's bin.js is present and readable. On Windows, pnpm
/// hard-links store files into the version tree and a transient filesystem
/// state (antivirus scan, indexer, post-install flush) can make `exists()`
/// return false once; retry briefly before declaring the install broken.
pub fn version_bin_ready(version_dir: &std::path::Path) -> bool {
    let bin = version_bin(version_dir);
    for _ in 0..5 {
        if bin.exists() {
            if let Ok(meta) = std::fs::metadata(&bin) {
                if meta.len() > 0 {
                    return true;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    bin.exists()
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
    env.push((
        "DSH_HOME".to_string(),
        home.path.to_string_lossy().to_string(),
    ));
    env.push(("DSH_LAUNCHER_INSTANCE".to_string(), inst.name.clone()));
    for (k, v) in &inst.env_overrides {
        if k == "DSH_HOME" {
            continue; // reserved
        }
        env.push((k.clone(), v.clone()));
    }
    Ok(env)
}

/// Whether a profile is a DSH web application: its package.json
/// `dsh.profile.bundles` includes `@deepseek-ai/dsh-web-app`. Such profiles
/// understand `--host/--port` and get a webview; they must bind a random
/// free port so several instances don't collide.
fn is_web_profile(home_path: &std::path::Path, profile: &str) -> bool {
    let pkg = home_path
        .join("profiles")
        .join(profile)
        .join("package.json");
    let Ok(raw) = std::fs::read_to_string(&pkg) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    doc.pointer("/dsh/profile/bundles")
        .and_then(|b| b.as_array())
        .map(|arr| {
            arr.iter()
                .any(|b| b.as_str() == Some("@deepseek-ai/dsh-web-app"))
        })
        .unwrap_or(false)
}

/// Whether the installed `dsh-web-app` bundle accepts `--no-open` (added in
/// 0.1.0-rc.8). Feature-detect the flag in the bundle's startup script: the
/// flag is a string literal in `lib/startup.js`, so presence is an exact
/// signal that survives pre-release version-number formats. The bundle lives
/// under pnpm's store; we scan `node_modules/.pnpm/**/@deepseek-ai/dsh-web-app/lib/startup.js`.
fn web_app_supports_no_open(version_dir: &std::path::Path) -> bool {
    // The pnpm hoisted "public" store keeps one canonical copy with a stable
    // path; the hashed `.pnpm/<name>@<ver>_<hash>` layout would need a scan.
    let hoisted = version_dir
        .join("node_modules")
        .join(".pnpm")
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh-web-app")
        .join("lib")
        .join("startup.js");
    let Ok(raw) = std::fs::read_to_string(&hoisted) else {
        return false;
    };
    raw.contains("--no-open") || raw.contains("no-open")
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
    if !version_bin_ready(&version.dir) {
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
    hide_console(&mut cmd);
    cmd.arg(&bin).arg("--profile").arg(profile);
    // Web-app profiles (their bundle list includes @deepseek-ai/dsh-web-app)
    // get a random free port; other profiles are managed purely as processes
    // (no URL/webview). We detect the web bundle rather than relying on the
    // profile being literally named "web" so user-named web profiles work.
    let home_path = cfg
        .homes
        .iter()
        .find(|h| h.id == inst.home_id)
        .map(|h| h.path.clone());
    let is_web = home_path
        .as_deref()
        .map(|hp| is_web_profile(hp, profile))
        .unwrap_or(profile == "web");
    if is_web {
        cmd.arg("--host").arg("127.0.0.1").arg("--port").arg("0");
        // `--no-open` was added to dsh-web-app in 0.1.0-rc.8: the launcher
        // embeds the UI in its own webview, so the app must not open the
        // system browser. Feature-detect the flag in the installed bundle's
        // startup.js rather than comparing pre-release versions.
        if web_app_supports_no_open(&version.dir) {
            cmd.arg("--no-open");
        }
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

    let shared_child: Arc<Mutex<Option<tokio::process::Child>>> = Arc::new(Mutex::new(Some(child)));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_web_profile_detects_web_app_bundle() {
        let dir = std::env::temp_dir().join(format!("dsh-proc-test-{}", uuid::Uuid::new_v4()));
        let profile_dir = dir.join("profiles").join("test");
        std::fs::create_dir_all(&profile_dir).unwrap();
        // No package.json -> not a web profile.
        assert!(!is_web_profile(&dir, "test"));
        // Web app bundle -> web profile.
        std::fs::write(
            profile_dir.join("package.json"),
            r#"{"name":"dsh-profile-test","private":true,"dependencies":{},"dsh":{"profile":{"bundles":["@deepseek-ai/dsh-base","@deepseek-ai/dsh-web-app"]}}}"#,
        )
        .unwrap();
        assert!(is_web_profile(&dir, "test"));
        // Non-web profile (no dsh-web-app bundle) -> false.
        let other = dir.join("profiles").join("bot");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(
            other.join("package.json"),
            r#"{"name":"dsh-profile-bot","private":true,"dependencies":{},"dsh":{"profile":{"bundles":["@deepseek-ai/dsh-base"]}}}"#,
        )
        .unwrap();
        assert!(!is_web_profile(&dir, "bot"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn web_app_supports_no_open_feature_detects_flag() {
        let dir = std::env::temp_dir().join(format!("dsh-proc-test-{}", uuid::Uuid::new_v4()));
        let startup_dir = dir
            .join("node_modules")
            .join(".pnpm")
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh-web-app")
            .join("lib");
        // No startup.js yet -> false.
        assert!(!web_app_supports_no_open(&dir));
        std::fs::create_dir_all(&startup_dir).unwrap();
        // Old bundle without the flag (<= 0.1.0-rc.7) -> false.
        std::fs::write(
            startup_dir.join("startup.js"),
            "const p = new Command().option('--host <host>').option('--port <port>')",
        )
        .unwrap();
        assert!(!web_app_supports_no_open(&dir));
        // New bundle with the flag (>= 0.1.0-rc.8) -> true.
        std::fs::write(
            startup_dir.join("startup.js"),
            "const p = new Command().option('--no-open', 'do not open the Web UI in the default browser')",
        )
        .unwrap();
        assert!(web_app_supports_no_open(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }
}
