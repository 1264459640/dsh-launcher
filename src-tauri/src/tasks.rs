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
#[tauri::command]
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

    // Resolve the DSH_HOME upfront so the task only needs the final id.
    let resolved_home_id = {
        if dedicated {
            let path = state
                .data_dir
                .join("homes")
                .join(crate::config::sanitize_name(&name))
                .to_string_lossy()
                .to_string();
            let home = crate::commands::create_home_record(&state, &name, &path)?;
            home.id
        } else {
            home_id.ok_or_else(|| "请选择 DSH_HOME".to_string())?
        }
    };

    // Validate early so a doomed task is never enqueued.
    {
        let cfg = state.config.lock().unwrap();
        if cfg.instances.iter().any(|i| i.name == name) {
            return Err("同名实例已存在".to_string());
        }
        if !cfg.homes.iter().any(|h| h.id == resolved_home_id) {
            return Err("DSH_HOME 不存在".to_string());
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
        run_create_instance_task(&worker_app, &state, &worker_task_id, &name, &version, &resolved_home_id).await;
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<TaskInfo>, String> {
    let tasks = state.tasks.lock().await;
    let mut out: Vec<TaskInfo> = tasks.values().cloned().collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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
    emit_progress(&app, &id, TaskState::Cancelled, 0, Some("已取消".to_string()), None);
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
    home_id: &str,
) {
    let result = do_create_instance(app, state, task_id, name, version, home_id).await;

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
                emit_progress(app, task_id, TaskState::Done, 100, None, Some(instance_id));
            }
            Err(msg) => {
                task.state = TaskState::Error;
                task.message = Some(msg.clone());
                push_log_locked(task, &format!("error: {msg}"));
                emit_progress(app, task_id, TaskState::Error, task.percent, Some(msg), None);
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
    home_id: &str,
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

    // 2. Create the instance record.
    let inst = {
        let mut cfg = state.config.lock().unwrap();
        if cfg.instances.iter().any(|i| i.name == name) {
            return Err("同名实例已存在".to_string());
        }
        let inst = DshInstance {
            id: new_id("i"),
            name: name.to_string(),
            version_id: version_record.id.clone(),
            home_id: home_id.to_string(),
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

/// Runs `npm install --loglevel=http` for the given version, streaming every
/// output line into the task log (and as events). Returns the new version
/// record on success.
async fn install_version_streamed(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    version: &str,
) -> Result<DshVersion, String> {
    let dir = state.data_dir.join("versions").join(version);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建版本目录失败: {e}"))?;

    let mut child = tokio::process::Command::new(crate::process::npm())
        .args(["install", "--prefix"])
        .arg(&dir)
        .args(["--no-audit", "--no-fund", "--loglevel=http"])
        .arg(format!("@deepseek-ai/dsh@{version}"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("npm 启动失败: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let shared_child: Arc<Mutex<Option<tokio::process::Child>>> =
        Arc::new(Mutex::new(Some(child)));

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

    let taken = shared_child.lock().await.take();
    let Some(mut child) = taken else {
        return Err("任务已取消".to_string());
    };
    let status = child
        .wait()
        .await
        .map_err(|e| format!("npm 等待失败: {e}"))?;

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
                    let pct = (task.percent + 1).min(95);
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
