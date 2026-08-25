use crate::config::{new_id, DshInstance, DshVersion};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

pub const TASK_PROGRESS_EVENT: &str = "task://progress";
pub const TASK_LOG_EVENT: &str = "task://log";

const MAX_LOG_LINES: usize = 1000;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Running,
    Done,
    Error,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskInfo {
    pub id: String,
    pub kind: String, // "create-instance"
    pub label: String,
    pub version: String,
    pub state: TaskState,
    pub percent: u32,
    pub created_at: i64,
    pub message: Option<String>,
    pub instance_id: Option<String>,
    pub instance_name: Option<String>,
    /// Reserved dedicated HOME path while the task is running; the actual
    /// HOME record is only created when the instance is created, so a
    /// cancelled/failed task never leaves an orphan HOME. Not serialized.
    #[serde(skip)]
    pub reserved_home_path: Option<std::path::PathBuf>,
    pub logs: Vec<String>,
    #[serde(skip)]
    pub child: Option<Arc<Mutex<Option<tokio::process::Child>>>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskProgress {
    pub id: String,
    pub state: TaskState,
    pub percent: u32,
    pub message: Option<String>,
    pub instance_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskLog {
    pub id: String,
    pub line: String,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Enqueues a background task that installs the given DSH version (if not
/// installed yet) and then creates the instance. Returns the task id.
#[tauri::command(rename_all = "snake_case")]
pub async fn start_create_instance_task(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    version: String,
    home_id: Option<String>,
    dedicated: bool,
) -> Result<String, String> {
    let name = name.trim().to_string();
    let version = version.trim().to_string();
    if name.is_empty() {
        return Err("实例名称不能为空".to_string());
    }
    if version.is_empty() {
        return Err("版本号不能为空".to_string());
    }

    // Dedicated HOME: reserve the path now (placeholder) but do NOT create the
    // HOME record yet — it is created only once the instance is actually made,
    // so a failed/cancelled task leaves no orphan HOME behind.
    let reserved_home_path: Option<std::path::PathBuf> = if dedicated {
        let path = state
            .data_dir
            .join("homes")
            .join(crate::config::sanitize_name(&name));
        Some(path)
    } else {
        None
    };

    // Validate early so a doomed task is never enqueued.
    {
        let cfg = state.config.lock().unwrap();
        if cfg.instances.iter().any(|i| i.name == name) {
            return Err("同名实例已存在".to_string());
        }
        // For a non-dedicated task the chosen HOME must exist already.
        if !dedicated {
            if let Some(hid) = &home_id {
                if !cfg.homes.iter().any(|h| h.id == *hid) {
                    return Err("DSH_HOME 不存在".to_string());
                }
            }
        }
    }
    // Reject a running/pending task that will create the same instance name
    // once it finishes (prevents duplicate name submissions).
    // Also reject two running tasks reserving the same dedicated HOME path.
    {
        let tasks = state.tasks.lock().await;
        for task in tasks.values() {
            if task.state == TaskState::Running {
                if task.instance_name.as_deref() == Some(name.as_str()) {
                    return Err("同名实例的下载任务已在进行中".to_string());
                }
                if let (Some(a), Some(b)) = (&task.reserved_home_path, &reserved_home_path) {
                    if crate::config::paths_equal(a, b) {
                        return Err("该专属 DSH_HOME 已被其他下载任务占用".to_string());
                    }
                }
            }
        }
    }

    let task = TaskInfo {
        id: new_id("t"),
        kind: "create-instance".to_string(),
        label: format!("下载 DSH {version} 并创建实例「{name}」"),
        version: version.clone(),
        state: TaskState::Running,
        percent: 0,
        created_at: now_millis(),
        message: None,
        instance_id: None,
        instance_name: Some(name.clone()),
        reserved_home_path,
        logs: Vec::new(),
        child: None,
    };
    let task_id = task.id.clone();
    state.tasks.lock().await.insert(task_id.clone(), task);
    emit_progress(&app, &task_id, TaskState::Running, 0, None, None);

    let worker_app = app.clone();
    let worker_task_id = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let state = worker_app.state::<AppState>();
        run_create_instance_task(
            &worker_app,
            &state,
            &worker_task_id,
            &name,
            &version,
            &home_id,
        )
        .await;
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<TaskInfo>, String> {
    let tasks = state.tasks.lock().await;
    let mut out: Vec<TaskInfo> = tasks.values().cloned().collect();
    out.sort_by_key(|t| std::cmp::Reverse(t.created_at));
    Ok(out)
}

#[tauri::command]
pub async fn remove_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut tasks = state.tasks.lock().await;
    let Some(task) = tasks.get(&id) else {
        return Err("任务不存在".to_string());
    };
    if task.state == TaskState::Running {
        return Err("任务仍在运行，请先取消".to_string());
    }
    tasks.remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn cancel_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let child = {
        let tasks = state.tasks.lock().await;
        tasks.get(&id).and_then(|t| t.child.clone())
    };
    {
        let mut tasks = state.tasks.lock().await;
        if let Some(task) = tasks.get_mut(&id) {
            task.state = TaskState::Cancelled;
            task.message = Some("已取消".to_string());
        }
    }
    if let Some(child) = child {
        let taken = child.lock().await.take();
        if let Some(mut c) = taken {
            let _ = c.kill().await;
        }
    }
    emit_progress(
        &app,
        &id,
        TaskState::Cancelled,
        0,
        Some("已取消".to_string()),
        None,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

async fn run_create_instance_task(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    name: &str,
    version: &str,
    home_id: &Option<String>,
) {
    // The dedicated HOME path is read from the task's reservation; only then
    // is the actual HOME record created (inside do_create_instance).
    let reserved = {
        let tasks = state.tasks.lock().await;
        tasks
            .get(task_id)
            .and_then(|t| t.reserved_home_path.clone())
    };
    let result = do_create_instance(
        app,
        state,
        task_id,
        name,
        version,
        home_id,
        reserved.as_deref(),
    )
    .await;

    let mut tasks = state.tasks.lock().await;
    if let Some(task) = tasks.get_mut(task_id) {
        if task.state == TaskState::Cancelled {
            return;
        }
        match result {
            Ok(instance_id) => {
                task.state = TaskState::Done;
                task.percent = 100;
                task.instance_id = Some(instance_id.clone());
                // The dedicated HOME now exists for real; release the placeholder.
                task.reserved_home_path = None;
                emit_progress(app, task_id, TaskState::Done, 100, None, Some(instance_id));
            }
            Err(msg) => {
                task.state = TaskState::Error;
                task.message = Some(msg.clone());
                push_log_locked(task, &format!("error: {msg}"));
                emit_progress(
                    app,
                    task_id,
                    TaskState::Error,
                    task.percent,
                    Some(msg),
                    None,
                );
            }
        }
    }
}

async fn do_create_instance(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    name: &str,
    version: &str,
    home_id: &Option<String>,
    reserved_home_path: Option<&std::path::Path>,
) -> Result<String, String> {
    // 1. Install the version if missing.
    let version_record = {
        let cfg = state.config.lock().unwrap();
        cfg.versions.iter().find(|v| v.version == version).cloned()
    };
    let version_record = match version_record {
        Some(v) => v,
        None => install_version_streamed(app, state, task_id, version).await?,
    };

    // 2. Resolve the actual DSH_HOME: for a dedicated task, create the HOME
    //    record now (path-based reuse keeps it idempotent); otherwise the
    //    caller-provided HOME id must already exist.
    let resolved_home_id = match home_id {
        Some(hid) => hid.clone(),
        None => {
            let path = reserved_home_path
                .ok_or_else(|| "缺少专属 DSH_HOME 路径".to_string())?
                .to_string_lossy()
                .to_string();
            crate::commands::create_home_record(state, name, &path)?.id
        }
    };
    let home_path = {
        let cfg = state.config.lock().unwrap();
        cfg.homes
            .iter()
            .find(|h| h.id == resolved_home_id)
            .ok_or_else(|| "DSH_HOME 不存在".to_string())?
            .path
            .clone()
    };

    // 2.5. Ensure the default web profile exists and a `__temp__` template
    // copy is created, so later profiles can be derived from it.
    ensure_web_profile_template(app, state, task_id, &home_path, &version_record).await?;

    // 3. Create the instance record.
    let inst = {
        let mut cfg = state.config.lock().unwrap();
        if cfg.instances.iter().any(|i| i.name == name) {
            return Err("同名实例已存在".to_string());
        }
        if !cfg.homes.iter().any(|h| h.id == resolved_home_id) {
            return Err("DSH_HOME 不存在".to_string());
        }
        let inst = DshInstance {
            id: new_id("i"),
            name: name.to_string(),
            version_id: version_record.id.clone(),
            home_id: resolved_home_id,
            env_overrides: Default::default(),
            default_profile: None,
            last_profile: None,
        };
        cfg.instances.push(inst.clone());
        crate::commands::save_state(state, &cfg)?;
        inst
    };
    Ok(inst.id)
}

/// Ensures the default `web` profile exists in the given DSH_HOME and that a
/// `__temp__` copy (the template later profiles are derived from) is present.
/// If the template is missing, it boots the installed DSH with
/// `--profile web --port <random>`, waits for the web URL (meaning the profile
/// was materialized), terminates it, then copies `profiles/web` to
/// `profiles/__temp__`. The profile for a fresh HOME is created the first time
/// a DSH process runs with that HOME, so this is a one-time cost per HOME.
async fn ensure_web_profile_template(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    home_path: &std::path::Path,
    version: &DshVersion,
) -> Result<(), String> {
    let profiles = home_path.join("profiles");
    let temp_dir = profiles.join("__temp__");
    if temp_dir.exists() {
        return Ok(());
    }

    let bin = crate::process::version_bin(&version.dir);
    if !bin.exists() {
        return Err(format!(
            "版本 {} 安装不完整（缺少 {}）",
            version.version,
            bin.display()
        ));
    }

    let port = 20000 + rand_port_offset();
    let msg = format!("正在初始化 web profile（端口 {port}）…");
    push_task_log(app, state, task_id, &msg).await;

    let mut child = tokio::process::Command::new(crate::process::node())
        .arg(&bin)
        .arg("--profile")
        .arg("web")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .env("DSH_HOME", home_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 DSH 生成 profile 失败: {e}"))?;

    // Wait for the web URL to appear (profile has been created), then stop it.
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(300));
    let mut attempts = 0;
    let mut ready = false;
    if let Some(out) = child.stdout.take() {
        let mut reader = BufReader::new(out).lines();
        loop {
            tokio::select! {
                line = reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            let l = l.trim().to_string();
                            if !l.is_empty() {
                                push_task_log(app, state, task_id, &l).await;
                            }
                            if l.contains("dsh web: http") {
                                ready = true;
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                _ = timer.tick() => {
                    attempts += 1;
                    if attempts > 200 { break; } // ~60s safety cap
                }
            }
        }
    }

    // Take ownership of the child handle to kill it (we already removed stdout).
    let _ = child.stderr.take();
    child.kill().await.ok();

    if !ready {
        return Err("生成 web profile 超时或失败".to_string());
    }

    // Copy profiles/web → profiles/__temp__.
    let web_dir = profiles.join("web");
    if !web_dir.exists() {
        return Err("web profile 目录未生成".to_string());
    }
    copy_dir(&web_dir, &temp_dir).map_err(|e| format!("复制 __temp__ profile 失败: {e}"))?;
    push_task_log(app, state, task_id, "web profile 模板 __temp__ 已创建").await;
    Ok(())
}

/// Simple deterministic-ish port offset so multiple homes don't collide often.
fn rand_port_offset() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 30000) as u16
}

async fn push_task_log(app: &AppHandle, state: &State<'_, AppState>, task_id: &str, line: &str) {
    let mut tasks = state.tasks.lock().await;
    if let Some(task) = tasks.get_mut(task_id) {
        // Cap the retained log (mirrors stream_pipe's MAX_LOG_LINES).
        if task.logs.len() >= MAX_LOG_LINES {
            task.logs.remove(0);
        }
        task.logs.push(line.to_string());
    }
    emit_log(app, task_id, line);
}

/// Recursively copies a directory tree (files only, directories preserved).
fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&from, &to)?;
        } else if ty.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Runs `pnpm install --loglevel=http` for the given version, streaming every
/// output line into the task log (and as events). The pnpm content store is
/// placed under the app data dir (`.pnpm-store`) so versions are installed
/// into the launcher's own storage. Returns the new version record on success.
async fn install_version_streamed(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    version: &str,
) -> Result<DshVersion, String> {
    let dir = state.data_dir.join("versions").join(version);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建版本目录失败: {e}"))?;
    let store_dir = state.data_dir.join(".pnpm-store");

    // pnpm (>=10) ignores dependency build scripts by default, which would
    // skip native modules like node-pty / koffi. A workspace manifest inside
    // the install dir opts back into running all build scripts.
    let ws_manifest = dir.join("pnpm-workspace.yaml");
    let ws_content = "onlyBuiltDependencies:\n  - '*'\n";
    if !ws_manifest.exists() {
        std::fs::write(&ws_manifest, ws_content)
            .map_err(|e| format!("写入 pnpm-workspace.yaml 失败: {e}"))?;
    }

    // Make sure a pnpm executable is available before installing: use the
    // system one if present, otherwise bootstrap the latest pnpm into the
    // launcher's data dir via npm.
    let pnpm_prog = ensure_pnpm(app, state, task_id).await?;

    let mut cmd = tokio::process::Command::new(&pnpm_prog);
    cmd.args(["install", "--prefix"])
        .arg(&dir)
        .arg("--store-dir")
        .arg(&store_dir)
        .args(["--loglevel=http"])
        .arg(format!("@deepseek-ai/dsh@{version}"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("pnpm 启动失败: {e}（请确认已安装 Node.js 与 pnpm）"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let shared_child: Arc<Mutex<Option<tokio::process::Child>>> = Arc::new(Mutex::new(Some(child)));

    // Expose the child for cancellation.
    {
        let mut tasks = state.tasks.lock().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.child = Some(shared_child.clone());
        }
    }

    // Stream both pipes into the task log.
    for pipe in [stdout.map(StreamPipe::Out), stderr.map(StreamPipe::Err)]
        .into_iter()
        .flatten()
    {
        let app2 = app.clone();
        let tid = task_id.to_string();
        tauri::async_runtime::spawn(async move {
            stream_pipe(app2, tid, pipe).await;
        });
    }

    // Heartbeat: after the metadata phase npm goes quiet while installing /
    // compiling native modules, so the line-counted percent would freeze.
    // Keep nudging the percent upward (90 → 99) while the process is alive.
    {
        let app2 = app.clone();
        let tid = task_id.to_string();
        let hb_child = shared_child.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                let still_running = {
                    let mut guard = hb_child.lock().await;
                    match guard.as_mut() {
                        Some(c) => matches!(c.try_wait(), Ok(None)),
                        None => false,
                    }
                };
                if !still_running {
                    break;
                }
                let state = app2.state::<AppState>();
                let mut tasks = state.tasks.lock().await;
                let Some(task) = tasks.get_mut(&tid) else {
                    break;
                };
                if task.state != TaskState::Running {
                    break;
                }
                if task.percent < 90 {
                    task.percent = (task.percent + 2).min(90);
                } else if task.percent < 99 {
                    task.percent += 1;
                } else {
                    break; // capped; wait for the final 100 on success
                }
                let pct = task.percent;
                drop(tasks);
                emit_progress(&app2, &tid, TaskState::Running, pct, None, None);
            }
        });
    }

    // Wait for npm to finish by polling try_wait (the heartbeat and
    // cancellation also access the shared child).
    let status = loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let mut guard = shared_child.lock().await;
        let Some(child) = guard.as_mut() else {
            return Err("任务已取消".to_string());
        };
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => continue,
            Err(e) => return Err(format!("npm 等待失败: {e}")),
        }
    };

    if !status.success() {
        let last = {
            let tasks = state.tasks.lock().await;
            tasks
                .get(task_id)
                .and_then(|t| t.logs.last().cloned())
                .unwrap_or_default()
        };
        return Err(format!("npm 安装失败（{}）", last));
    }

    let record = DshVersion {
        id: new_id("v"),
        version: version.to_string(),
        dir,
    };
    let mut cfg = state.config.lock().unwrap();
    if let Some(existing) = cfg.versions.iter().find(|v| v.version == *version) {
        return Ok(existing.clone());
    }
    cfg.versions.push(record.clone());
    crate::commands::save_state(state, &cfg)?;
    Ok(record)
}

/// Returns a usable pnpm executable. Prefers the system pnpm; falls back to
/// a pnpm bootstrapped into the launcher data dir (`tools/`). If neither
/// exists it installs the latest pnpm there via npm and returns its path.
async fn ensure_pnpm(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
) -> Result<std::path::PathBuf, String> {
    // 1. System pnpm available?
    let sys = tokio::process::Command::new(crate::process::pnpm())
        .arg("--version")
        .output()
        .await;
    if let Ok(out) = sys {
        if out.status.success() {
            return Ok(std::path::PathBuf::from(crate::process::pnpm()));
        }
    }

    // 2. Local pnpm already bootstrapped?
    let tools_dir = state.data_dir.join("tools");
    let local = local_pnpm_path(&tools_dir);
    if local.exists() {
        let probe = tokio::process::Command::new(&local)
            .arg("--version")
            .output()
            .await;
        if let Ok(out) = probe {
            if out.status.success() {
                return Ok(local);
            }
        }
    }

    // 3. Bootstrap the latest pnpm inside the data dir via npm.
    std::fs::create_dir_all(&tools_dir).map_err(|e| format!("创建工具目录失败: {e}"))?;
    let msg = "检测到未安装 pnpm，正在安装最新版…";
    {
        let mut tasks = state.tasks.lock().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.percent = 5;
            push_log_locked(task, msg);
        }
    }
    emit_progress(app, task_id, TaskState::Running, 5, None, None);
    emit_log(app, task_id, msg);

    let child = tokio::process::Command::new(crate::process::npm())
        .args(["install", "--global", "--prefix"])
        .arg(&tools_dir)
        .args(["pnpm@latest"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("pnpm 安装启动失败: {e}"))?;
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("pnpm 安装等待失败: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let last = err.lines().last().unwrap_or(&err).to_string();
        return Err(format!("pnpm 安装失败: {last}"));
    }

    let local = local_pnpm_path(&tools_dir);
    if !local.exists() {
        return Err(format!(
            "pnpm 安装完成但未找到可执行文件: {}",
            local.display()
        ));
    }
    Ok(local)
}

/// Path of the pnpm executable inside a tools dir (Windows uses .cmd).
fn local_pnpm_path(tools_dir: &std::path::Path) -> std::path::PathBuf {
    if cfg!(windows) {
        tools_dir.join("pnpm.cmd")
    } else {
        tools_dir.join("pnpm")
    }
}

// ---------------------------------------------------------------------------

enum StreamPipe {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

impl tokio::io::AsyncRead for StreamPipe {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            StreamPipe::Out(p) => std::pin::Pin::new(p).poll_read(cx, buf),
            StreamPipe::Err(p) => std::pin::Pin::new(p).poll_read(cx, buf),
        }
    }
}

async fn stream_pipe(app: AppHandle, task_id: String, pipe: StreamPipe) {
    let state = app.state::<AppState>();
    let mut lines = BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        if line.is_empty() {
            continue;
        }
        let percent = {
            let mut tasks = state.tasks.lock().await;
            match tasks.get_mut(&task_id) {
                Some(task) if task.state == TaskState::Running => {
                    push_log_locked(task, &line);
                    let pct = (task.percent + 1).min(90);
                    task.percent = pct;
                    // Throttle: emit progress roughly every 20 log lines.
                    if task.logs.len() % 20 == 0 {
                        Some(pct)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };
        emit_log(&app, &task_id, &line);
        if let Some(pct) = percent {
            emit_progress(&app, &task_id, TaskState::Running, pct, None, None);
        }
    }
}

fn push_log_locked(task: &mut TaskInfo, line: &str) {
    if task.logs.len() >= MAX_LOG_LINES {
        task.logs.remove(0);
    }
    task.logs.push(line.to_string());
}

fn emit_progress(
    app: &AppHandle,
    id: &str,
    state: TaskState,
    percent: u32,
    message: Option<String>,
    instance_id: Option<String>,
) {
    let _ = app.emit(
        TASK_PROGRESS_EVENT,
        TaskProgress {
            id: id.to_string(),
            state,
            percent,
            message,
            instance_id,
        },
    );
}

fn emit_log(app: &AppHandle, id: &str, line: &str) {
    let _ = app.emit(
        TASK_LOG_EVENT,
        TaskLog {
            id: id.to_string(),
            line: line.to_string(),
        },
    );
}
