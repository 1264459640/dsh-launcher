use crate::config::{
    new_id, sanitize_name, DshHome, DshInstance, DshVersion, LauncherSettings,
    NewInstanceInput, RemoteVersion, SettingsPatch,
};
use crate::{process, AppState};
use std::collections::BTreeMap;
use tauri::{AppHandle, State};

// ---------------------------------------------------------------------------
// DSH_HOME
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_homes(state: State<'_, AppState>) -> Result<Vec<DshHome>, String> {
    Ok(state.config.lock().unwrap().homes.clone())
}

#[tauri::command]
pub fn create_home(
    state: State<'_, AppState>,
    name: String,
    path: String,
) -> Result<DshHome, String> {
    create_home_record(&state, &name, &path)
}

/// Shared helper: validates + creates a DSH_HOME record.
pub(crate) fn create_home_record(
    state: &State<'_, AppState>,
    name: &str,
    path: &str,
) -> Result<DshHome, String> {
    let name = name.trim();
    let path = path.trim();
    if name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    if path.is_empty() {
        return Err("路径不能为空".to_string());
    }
    let path = std::path::PathBuf::from(path);
    std::fs::create_dir_all(&path).map_err(|e| format!("创建目录失败: {e}"))?;
    let home = DshHome {
        id: new_id("h"),
        name: name.to_string(),
        path,
    };
    let mut cfg = state.config.lock().unwrap();
    cfg.homes.push(home.clone());
    save_state(state, &cfg)?;
    Ok(home)
}

#[tauri::command]
pub fn default_dedicated_home_path(
    state: State<'_, AppState>,
    name: String,
) -> Result<String, String> {
    Ok(state
        .data_dir
        .join("homes")
        .join(sanitize_name(&name))
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub fn remove_home(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap();
    if cfg.instances.iter().any(|i| i.home_id == id) {
        return Err("该 DSH_HOME 仍被实例引用，无法删除".to_string());
    }
    cfg.homes.retain(|h| h.id != id);
    save_state(&state, &cfg)
}

// ---------------------------------------------------------------------------
// DSH versions
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_versions(state: State<'_, AppState>) -> Result<Vec<DshVersion>, String> {
    Ok(state.config.lock().unwrap().versions.clone())
}

/// Queries the npm registry for available @deepseek-ai/dsh versions with
/// their publish dates.
#[tauri::command]
pub async fn fetch_available_versions() -> Result<Vec<RemoteVersion>, String> {
    let versions_json = run_npm_view("@deepseek-ai/dsh", "versions").await?;
    let versions: Vec<String> = serde_json::from_str(&versions_json)
        .map_err(|e| format!("解析版本列表失败: {e}"))?;

    let time_json = run_npm_view("@deepseek-ai/dsh", "time").await?;
    // npm >= 9 returns `time` as `[{ "created": ..., "<version>": "<date>" }]`
    // (array wrapping one object); older npm returned the object directly.
    let time_map: BTreeMap<String, serde_json::Value> = match serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&time_json) {
        Ok(map) => map,
        Err(_) => {
            let arr: Vec<BTreeMap<String, serde_json::Value>> = serde_json::from_str(&time_json)
                .map_err(|e| format!("解析发布时间失败: {e}"))?;
            arr.into_iter().next().unwrap_or_default()
        }
    };

    let mut out = Vec::with_capacity(versions.len());
    for v in versions {
        let released_at = time_map
            .get(&v)
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        out.push(RemoteVersion {
            version: v,
            released_at,
        });
    }
    Ok(out)
}

async fn run_npm_view(pkg: &str, field: &str) -> Result<String, String> {
    let output = tokio::process::Command::new(process::npm())
        .args(["view", pkg, field, "--json"])
        .output()
        .await
        .map_err(|e| format!("npm 执行失败: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "npm view 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub fn remove_version(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap();
    if cfg.instances.iter().any(|i| i.version_id == id) {
        return Err("该版本仍被实例引用，无法删除".to_string());
    }
    let Some(version) = cfg.versions.iter().find(|v| v.id == id).cloned() else {
        return Err("版本不存在".to_string());
    };
    cfg.versions.retain(|v| v.id != id);
    save_state(&state, &cfg)?;
    // Best-effort removal of the install directory.
    let _ = std::fs::remove_dir_all(&version.dir);
    Ok(())
}

// ---------------------------------------------------------------------------
// Instances
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_instances(state: State<'_, AppState>) -> Result<Vec<DshInstance>, String> {
    Ok(state.config.lock().unwrap().instances.clone())
}

#[tauri::command]
pub fn create_instance(
    state: State<'_, AppState>,
    input: NewInstanceInput,
) -> Result<DshInstance, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("实例名称不能为空".to_string());
    }
    let mut cfg = state.config.lock().unwrap();
    if cfg.instances.iter().any(|i| i.name == name) {
        return Err("同名实例已存在".to_string());
    }
    if !cfg.versions.iter().any(|v| v.id == input.version_id) {
        return Err("DSH 版本不存在".to_string());
    }
    if !cfg.homes.iter().any(|h| h.id == input.home_id) {
        return Err("DSH_HOME 不存在".to_string());
    }
    let inst = DshInstance {
        id: new_id("i"),
        name,
        version_id: input.version_id,
        home_id: input.home_id,
        env_overrides: input.env_overrides,
        default_profile: input.default_profile,
        last_profile: None,
    };
    cfg.instances.push(inst.clone());
    save_state(&state, &cfg)?;
    Ok(inst)
}

#[tauri::command]
pub fn update_instance(
    state: State<'_, AppState>,
    input: DshInstance,
) -> Result<DshInstance, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("实例名称不能为空".to_string());
    }
    let mut cfg = state.config.lock().unwrap();
    if cfg.instances.iter().any(|i| i.name == name && i.id != input.id) {
        return Err("同名实例已存在".to_string());
    }
    if !cfg.versions.iter().any(|v| v.id == input.version_id) {
        return Err("DSH 版本不存在".to_string());
    }
    if !cfg.homes.iter().any(|h| h.id == input.home_id) {
        return Err("DSH_HOME 不存在".to_string());
    }
    let mut updated = input;
    updated.name = name;
    let Some(pos) = cfg.instances.iter().position(|i| i.id == updated.id) else {
        return Err("实例不存在".to_string());
    };
    cfg.instances[pos] = updated.clone();
    save_state(&state, &cfg)?;
    Ok(updated)
}

#[tauri::command]
pub async fn delete_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    // Stop it first if running.
    if state.running.lock().await.contains_key(&id) {
        let _ = process::stop_instance_process(&app, &state, &id).await;
    }
    let mut cfg = state.config.lock().unwrap();
    cfg.instances.retain(|i| i.id != id);
    if cfg.settings.last_instance_id.as_deref() == Some(id.as_str()) {
        cfg.settings.last_instance_id = None;
    }
    save_state(&state, &cfg)
}

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>, home_id: String) -> Result<Vec<String>, String> {
    let cfg = state.config.lock().unwrap();
    let home = cfg
        .homes
        .iter()
        .find(|h| h.id == home_id)
        .ok_or_else(|| "DSH_HOME 不存在".to_string())?;
    let profiles_dir = home.path.join("profiles");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "node_modules" {
                continue;
            }
            if entry.path().is_dir() {
                out.push(name);
            }
        }
    }
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Instance runtime
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    profile: String,
) -> Result<(), String> {
    process::start_instance_process(&app, &state, &id, &profile).await?;
    // Remember the last used profile.
    let mut cfg = state.config.lock().unwrap();
    if let Some(inst) = cfg.instances.iter_mut().find(|i| i.id == id) {
        inst.last_profile = Some(profile);
    }
    save_state(&state, &cfg)
}

#[tauri::command]
pub async fn stop_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    process::stop_instance_process(&app, &state, &id).await
}

#[tauri::command]
pub async fn list_instance_status(state: State<'_, AppState>) -> Result<Vec<crate::config::InstanceStatus>, String> {
    Ok(process::list_statuses(&state).await)
}

#[tauri::command]
pub async fn open_instance_window(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let entry = state.running.lock().await.get(&id).map(|r| r.url.clone());
    let Some(url) = entry.flatten() else {
        return Err("实例未在运行或尚未就绪".to_string());
    };
    let name = state
        .config
        .lock()
        .unwrap()
        .instances
        .iter()
        .find(|i| i.id == id)
        .map(|i| i.name.clone())
        .unwrap_or_else(|| id.clone());
    crate::windows::open_instance_window(&app, &id, &name, &url)
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<LauncherSettings, String> {
    Ok(state.config.lock().unwrap().settings.clone())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: SettingsPatch,
) -> Result<LauncherSettings, String> {
    let mut cfg = state.config.lock().unwrap();
    if let Some(v) = settings.locale {
        cfg.settings.locale = v;
    }
    if let Some(v) = settings.minimize_to_tray {
        cfg.settings.minimize_to_tray = v;
    }
    if let Some(v) = settings.autostart {
        let prev = cfg.settings.autostart;
        cfg.settings.autostart = v;
        if v != prev {
            use tauri_plugin_autostart::ManagerExt;
            let mgr = app.autolaunch();
            let result = if v {
                mgr.enable()
            } else {
                mgr.disable()
            };
            if let Err(e) = result {
                // Revert the stored flag so the UI stays truthful.
                cfg.settings.autostart = prev;
                return Err(format!("设置开机自启失败: {e}"));
            }
        }
    }
    if let Some(v) = settings.last_instance_id {
        cfg.settings.last_instance_id = Some(v);
    }
    let out = cfg.settings.clone();
    save_state(&state, &cfg)?;
    Ok(out)
}

// ---------------------------------------------------------------------------

pub(crate) fn save_state(state: &State<'_, AppState>, cfg: &crate::config::Config) -> Result<(), String> {
    crate::config::save_config(&state.config_path, cfg)
}
