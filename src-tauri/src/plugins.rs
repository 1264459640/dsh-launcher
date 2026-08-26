// Plugin marketplace: fetches the plugin catalog from the market API and
// exposes per-channel versions (stable = releases/latest, beta =
// pre-releases/next, alpha = latest commit) plus install/enable plumbing.

use crate::config::{new_id, DshInstance};
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

/// Market API endpoint (dsh-plugins.github.io publishes to the custom domain).
const MARKET_URL: &str = "https://dsh-plug.in/api/plugins.json";
const NPM_REGISTRY: &str = "https://registry.npmjs.org";

/// Public OAuth App client id used to boost unauthenticated GitHub API quota
/// from 60 to 5000 requests/hour (an anonymous client-id parameter, no
/// authorization or token storage required). App: "DSH Launcher".
const GITHUB_CLIENT_ID: &str = "Ov23li6vtlVd83282YL6";

/// Build a GitHub API URL with the anonymous client-id quota boost.
fn github_api_url(path: &str) -> String {
    let sep = if path.contains('?') { '&' } else { '?' };
    format!("https://api.github.com{path}{sep}client_id={GITHUB_CLIENT_ID}")
}

// ---------------------------------------------------------------------------
// Market catalog
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketPluginDescription {
    pub language: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketPluginUrls {
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub issues: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketPluginRelationship {
    /// The market JSON uses `type`; we expose it to the frontend as `kind`.
    #[serde(alias = "type")]
    pub kind: String,
    pub id: String,
    pub versions: String,
}

/// description can be a plain string or a localized list; normalise both.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarketDescription {
    Plain(String),
    Localized(Vec<MarketPluginDescription>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketPlugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<MarketDescription>,
    #[serde(default)]
    pub support_versions: Option<serde_json::Value>,
    #[serde(default)]
    pub urls: Option<MarketPluginUrls>,
    #[serde(default)]
    pub relationship: Option<Vec<MarketPluginRelationship>>,
}

// ---------------------------------------------------------------------------
// Version channels
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PluginChannel {
    /// Releases / npm latest.
    Stable,
    /// Pre-releases / npm next.
    Beta,
    /// Latest commit on the default branch.
    Alpha,
}

#[derive(Clone, Debug, Serialize)]
pub struct PluginVersionInfo {
    /// The raw version identifier passed to the installer: a semver version
    /// for stable/beta, a commit hash for alpha.
    pub version: String,
    pub channel: PluginChannel,
    /// Short human label (e.g. the commit date or release tag).
    pub label: Option<String>,
    /// Whether this is the channel's default (latest) entry.
    pub is_default: bool,
}

/// A page of versions. `has_more` is true when pagination can continue
/// (used by the alpha / commit channel).
#[derive(Clone, Debug, Serialize)]
pub struct PluginVersionPage {
    pub versions: Vec<PluginVersionInfo>,
    pub has_more: bool,
}

// ---------------------------------------------------------------------------
// Installed plugin (per instance/profile)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct InstalledPlugin {
    /// Package name / id (e.g. "@dsh-plugin/dsh-auxiliary").
    pub id: String,
    /// Installed version spec as recorded in the profile manifest.
    pub version: Option<String>,
    /// Whether the plugin is currently enabled (not disabled in cordis.patch.yml).
    pub enabled: bool,
    /// The cordis plugin id used in cordis.patch.yml (disables/insert rows).
    pub cordis_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginInput {
    pub plugin_id: String,
    pub version: String,
    pub channel: PluginChannel,
    pub instance_id: String,
    pub profile: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPluginsEnabledInput {
    pub instance_id: String,
    pub profile: String,
    pub plugin_ids: Vec<String>,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("dsh-launcher")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

/// Fetch and parse a JSON document with a size cap.
async fn fetch_json(url: &str, cap: usize) -> Result<serde_json::Value, String> {
    let client = http_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求失败 {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("请求失败 {url}: HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败 {url}: {e}"))?;
    if bytes.len() > cap {
        return Err(format!("响应过大 {url}"));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("解析 JSON 失败 {url}: {e}"))
}

// ---------------------------------------------------------------------------
// Commands: catalog
// ---------------------------------------------------------------------------

/// Fetches the marketplace plugin catalog. `query` filters by id/name/
/// description (case-insensitive substring) when provided.
#[tauri::command(rename_all = "snake_case")]
pub async fn fetch_plugin_market(query: Option<String>) -> Result<Vec<MarketPlugin>, String> {
    let value = fetch_json(MARKET_URL, 4 * 1024 * 1024).await?;
    let plugins: Vec<MarketPlugin> =
        serde_json::from_value(value).map_err(|e| format!("解析插件市场数据失败: {e}"))?;

    let q = query
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let Some(q) = q else {
        return Ok(plugins);
    };

    let filtered = plugins
        .into_iter()
        .filter(|p| {
            if p.id.to_lowercase().contains(&q) || p.name.to_lowercase().contains(&q) {
                return true;
            }
            match &p.description {
                Some(MarketDescription::Plain(s)) => s.to_lowercase().contains(&q),
                Some(MarketDescription::Localized(list)) => {
                    list.iter().any(|d| d.content.to_lowercase().contains(&q))
                }
                None => false,
            }
        })
        .collect();
    Ok(filtered)
}

// ---------------------------------------------------------------------------
// Commands: versions per channel
// ---------------------------------------------------------------------------

/// Resolve the GitHub "owner/repo" for a plugin id. npm ids look up the
/// package's repository URL when urls.repository is absent; github: ids are
/// parsed directly.
fn github_repo_of(plugin: &MarketPlugin) -> Option<String> {
    if let Some(repo) = plugin
        .urls
        .as_ref()
        .and_then(|u| u.repository.as_ref().or(u.homepage.as_ref()))
    {
        if let Some(pos) = repo.find("github.com/") {
            let tail = &repo[pos + "github.com/".len()..];
            let tail = tail.trim_end_matches(".git").trim_end_matches('/');
            let mut parts = tail.split('/');
            if let (Some(owner), Some(name)) = (parts.next(), parts.next()) {
                if !owner.is_empty() && !name.is_empty() {
                    return Some(format!("{owner}/{name}"));
                }
            }
        }
    }
    if let Some(rest) = plugin.id.strip_prefix("github:") {
        return Some(
            rest.trim_end_matches(".git")
                .trim_end_matches('/')
                .to_string(),
        );
    }
    None
}

/// Fetches versions for a plugin across the requested channel.
/// - stable/beta read the npm registry dist-tags (latest / next) and fall
///   back to the version list ordered by publish time (all at once).
/// - alpha pages through the GitHub commit history (30 per page); `page` is
///   1-based and defaults to 1. `has_more` tells the UI to lazy-load more.
#[tauri::command(rename_all = "snake_case")]
pub async fn fetch_plugin_versions(
    plugin_id: String,
    channel: PluginChannel,
    page: Option<u32>,
) -> Result<PluginVersionPage, String> {
    match channel {
        PluginChannel::Stable | PluginChannel::Beta => {
            let versions = npm_versions(&plugin_id, &channel).await?;
            Ok(PluginVersionPage {
                versions,
                has_more: false,
            })
        }
        PluginChannel::Alpha => alpha_commit(&plugin_id, page.unwrap_or(1)).await,
    }
}

async fn npm_versions(
    plugin_id: &str,
    channel: &PluginChannel,
) -> Result<Vec<PluginVersionInfo>, String> {
    // Scoped npm packages must be URL-encoded (@dsh-plugin/x -> @dsh-plugin%2fx).
    let encoded = plugin_id.replace('/', "%2f");
    let url = format!("{NPM_REGISTRY}/{encoded}");
    let doc = fetch_json(&url, 16 * 1024 * 1024).await?;

    let dist_tags = doc
        .get("dist-tags")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let tag_name = match channel {
        PluginChannel::Stable => "latest",
        PluginChannel::Beta => "next",
        PluginChannel::Alpha => unreachable!(),
    };

    // The channel's default (dist-tag) version, if present.
    let default_version = dist_tags
        .get(tag_name)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Collect all published versions with their release time, newest first.
    let time = doc.get("time").cloned().unwrap_or(serde_json::json!({}));
    let mut versions: Vec<(String, String)> = Vec::new();
    if let Some(obj) = time.as_object() {
        for (ver, ts) in obj {
            if ver == "created" || ver == "modified" {
                continue;
            }
            let ts = ts.as_str().unwrap_or("").to_string();
            versions.push((ver.clone(), ts));
        }
    }
    versions.sort_by(|a, b| b.1.cmp(&a.1)); // newest first by ISO time

    // Filter per channel: stable = no pre-release tag, beta = pre-release tag.
    let is_prerelease = |v: &str| v.contains('-');
    let mut out: Vec<PluginVersionInfo> = Vec::new();
    for (ver, ts) in versions {
        let include = match channel {
            PluginChannel::Stable => !is_prerelease(&ver),
            PluginChannel::Beta => is_prerelease(&ver),
            PluginChannel::Alpha => unreachable!(),
        };
        if !include {
            continue;
        }
        let is_default = default_version.as_deref() == Some(ver.as_str());
        out.push(PluginVersionInfo {
            version: ver,
            channel: channel.clone(),
            label: if ts.is_empty() { None } else { Some(ts) },
            is_default,
        });
    }

    // Make sure the dist-tag default is present even if it didn't pass the
    // filter (e.g. a `latest` that is itself a pre-release).
    if let Some(def) = default_version {
        if !out.iter().any(|v| v.version == def) {
            out.insert(
                0,
                PluginVersionInfo {
                    version: def,
                    channel: channel.clone(),
                    label: Some("dist-tag".to_string()),
                    is_default: true,
                },
            );
        }
    }
    Ok(out)
}

/// Fetches one page of the commit history (alpha channel). GitHub commits
/// API returns up to `per_page` items; `has_more` is true when a full page
/// came back. `is_default` marks the first commit of page 1.
async fn alpha_commit(plugin_id: &str, page: u32) -> Result<PluginVersionPage, String> {
    // Alpha needs the GitHub repo; it is derived from the market entry.
    let catalog = fetch_plugin_market(None).await?;
    let plugin = catalog
        .iter()
        .find(|p| p.id == plugin_id)
        .ok_or_else(|| format!("插件 {plugin_id} 不在市场中"))?;
    let repo = github_repo_of(plugin)
        .ok_or_else(|| format!("插件 {plugin_id} 没有可用的 GitHub 仓库地址"))?;

    const PER_PAGE: u32 = 30;
    let url = github_api_url(&format!(
        "/repos/{repo}/commits?per_page={PER_PAGE}&page={page}"
    ));
    let doc = fetch_json(&url, 4 * 1024 * 1024).await?;
    let mut out: Vec<PluginVersionInfo> = Vec::new();
    if let Some(arr) = doc.as_array() {
        for (i, commit) in arr.iter().enumerate() {
            let sha = commit
                .get("sha")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if sha.is_empty() {
                continue;
            }
            let message = commit
                .pointer("/commit/message")
                .and_then(|v| v.as_str())
                .map(|s| s.lines().next().unwrap_or("").to_string());
            let date = commit
                .pointer("/commit/committer/date")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let label = match (message, date) {
                (Some(m), Some(d)) => Some(format!("{d} · {m}")),
                (Some(m), None) => Some(m),
                (None, Some(d)) => Some(d),
                _ => None,
            };
            out.push(PluginVersionInfo {
                version: sha,
                channel: PluginChannel::Alpha,
                label,
                is_default: page == 1 && i == 0,
            });
        }
    }
    let has_more = out.len() as u32 == PER_PAGE;
    Ok(PluginVersionPage {
        versions: out,
        has_more,
    })
}

// ---------------------------------------------------------------------------
// Profile manifest helpers (read/write package.json + cordis.patch.yml)
// ---------------------------------------------------------------------------

/// Path of a profile dir under a DSH_HOME.
fn profile_dir(home_path: &std::path::Path, profile: &str) -> std::path::PathBuf {
    home_path.join("profiles").join(profile)
}

/// Read the profile package.json (dsh.profile.bundles + dependencies).
fn read_profile_manifest(dir: &std::path::Path) -> Result<serde_json::Value, String> {
    let path = dir.join("package.json");
    if !path.exists() {
        return Ok(serde_json::json!({
            "private": true,
            "dependencies": {},
            "dsh": { "profile": { "bundles": [] } },
        }));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取 package.json 失败: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析 package.json 失败: {e}"))
}

fn write_profile_manifest(dir: &std::path::Path, value: &serde_json::Value) -> Result<(), String> {
    let path = dir.join("package.json");
    let raw = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = dir.join("package.json.tmp");
    std::fs::write(&tmp, raw).map_err(|e| format!("写入 package.json 失败: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("保存 package.json 失败: {e}"))?;
    Ok(())
}

/// cordis id for a package: bundles register under their unscoped short name
/// (dsh-auxiliary) unless the package declares otherwise. We default to the
/// last path segment without the scope.
pub fn cordis_id_of(package: &str) -> String {
    let last = package.rsplit('/').next().unwrap_or(package);
    last.to_string()
}

// ---------------------------------------------------------------------------
// Commands: installed plugin listing (per instance + profile)
// ---------------------------------------------------------------------------

/// Lists plugins installed into an instance's profile, excluding core
/// @deepseek-ai/* packages. Reads the profile manifest (dependencies +
/// bundles) and cordis.patch.yml (disabled rows).
#[tauri::command(rename_all = "snake_case")]
pub async fn list_installed_plugins(
    state: State<'_, AppState>,
    instance_id: String,
    profile: String,
) -> Result<Vec<InstalledPlugin>, String> {
    let (home_path, _version) = resolve_instance(&state, &instance_id)?;
    let dir = profile_dir(&home_path, &profile);
    let manifest = read_profile_manifest(&dir)?;

    let mut ids: Vec<String> = Vec::new();
    let mut versions: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    if let Some(deps) = manifest.get("dependencies").and_then(|d| d.as_object()) {
        for (name, spec) in deps {
            if name.starts_with("@deepseek-ai/") {
                continue;
            }
            ids.push(name.clone());
            versions.insert(name.clone(), spec.as_str().unwrap_or("").to_string());
        }
    }
    if let Some(bundles) = manifest
        .pointer("/dsh/profile/bundles")
        .and_then(|b| b.as_array())
    {
        for b in bundles {
            if let Some(name) = b.as_str() {
                if name.starts_with("@deepseek-ai/") || ids.iter().any(|i| i == name) {
                    continue;
                }
                ids.push(name.to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();

    // Disabled set from cordis.patch.yml (`- id: <cordis-id>` + `disabled: true`).
    let disabled = read_disabled_ids(&dir);

    let out = ids
        .into_iter()
        .map(|id| {
            let cordis_id = cordis_id_of(&id);
            let enabled = !disabled.contains(&cordis_id) && !disabled.contains(&id);
            InstalledPlugin {
                version: versions.get(&id).cloned(),
                enabled,
                cordis_id: Some(cordis_id),
                id,
            }
        })
        .collect();
    Ok(out)
}

/// Parse disabled cordis ids from a profile's cordis.patch.yml. We do a
/// lightweight line scan (avoid pulling a YAML parser dependency for this).
fn read_disabled_ids(dir: &std::path::Path) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let path = dir.join("cordis.patch.yml");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return set;
    };
    let mut current_id: Option<String> = None;
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("- id:") {
            current_id = Some(t.trim_start_matches("- id:").trim().to_string());
        } else if t.starts_with("id:") && !line.starts_with(' ') && !line.starts_with('\t') {
            current_id = Some(t.trim_start_matches("id:").trim().to_string());
        } else if t == "disabled: true" {
            if let Some(id) = current_id.take() {
                set.insert(id);
            }
        } else if t.starts_with("- ") && !t.starts_with("- id:") {
            current_id = None;
        }
    }
    set
}

/// Resolve an instance to (home_path, version_dir).
fn resolve_instance(
    state: &State<'_, AppState>,
    instance_id: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let cfg = state.config.lock().unwrap();
    let inst: &DshInstance = cfg
        .instances
        .iter()
        .find(|i| i.id == instance_id)
        .ok_or_else(|| "实例不存在".to_string())?;
    let home = cfg
        .homes
        .iter()
        .find(|h| h.id == inst.home_id)
        .ok_or_else(|| "DSH_HOME 不存在".to_string())?;
    let version = cfg
        .versions
        .iter()
        .find(|v| v.id == inst.version_id)
        .ok_or_else(|| "版本不存在".to_string())?;
    Ok((home.path.clone(), version.dir.clone()))
}

// ---------------------------------------------------------------------------
// Commands: enable / disable (cordis.patch.yml disabled rows)
// ---------------------------------------------------------------------------

/// Sets plugins enabled/disabled in a profile's cordis.patch.yml by adding or
/// removing `disabled: true` rows. Batch-capable via plugin_ids.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_plugins_enabled(
    state: State<'_, AppState>,
    input: SetPluginsEnabledInput,
) -> Result<(), String> {
    let (home_path, _) = resolve_instance(&state, &input.instance_id)?;
    let dir = profile_dir(&home_path, &input.profile);
    let patch_path = dir.join("cordis.patch.yml");

    let mut raw = if patch_path.exists() {
        std::fs::read_to_string(&patch_path)
            .map_err(|e| format!("读取 cordis.patch.yml 失败: {e}"))?
    } else {
        String::new()
    };

    for package in &input.plugin_ids {
        let cordis_id = cordis_id_of(package);
        raw = set_disabled_row(&raw, &cordis_id, input.enabled);
    }

    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 profile 目录失败: {e}"))?;
    std::fs::write(&patch_path, raw).map_err(|e| format!("写入 cordis.patch.yml 失败: {e}"))?;
    Ok(())
}

/// Add or remove a `disabled: true` row for a cordis id in cordis.patch.yml.
fn set_disabled_row(raw: &str, cordis_id: &str, enabled: bool) -> String {
    // Remove any existing rows for this id (both plain and commented forms).
    let mut out: Vec<String> = Vec::new();
    let mut skip_block = false;
    for line in raw.lines() {
        let t = line.trim();
        // A top-level `[]` placeholder is dropped when we have any real entry
        // to write; it is kept only while the document stays empty.
        if t == "[]" {
            continue;
        }
        let is_target_id = t == format!("- id: {cordis_id}") || t == format!("id: {cordis_id}");
        if is_target_id {
            // Start of a block for this id; look ahead: if it is a pure
            // `disabled: true` block we drop it entirely.
            skip_block = true;
            continue;
        }
        if skip_block {
            // Inside the block: only `disabled:` and blank lines belong to it.
            if t == "disabled: true" || t == "disabled: false" || t.is_empty() {
                skip_block = false; // end of this small block
                continue;
            }
            // Block has other content (config etc.) — keep it, stop skipping.
            skip_block = false;
            out.push(line.to_string());
            continue;
        }
        out.push(line.to_string());
    }

    let mut cleaned: Vec<String> = out;
    // Trim trailing blank lines.
    while cleaned.last().map(|l| l.trim().is_empty()) == Some(true) {
        cleaned.pop();
    }

    if !enabled {
        // Append a fresh disable row (block sequence, never after `[]`).
        cleaned.push(String::new());
        cleaned.push(format!("- id: {cordis_id}"));
        cleaned.push("  disabled: true".to_string());
    }

    let mut result = cleaned.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    // If the document became empty again (everything removed), restore the
    // `[]` placeholder so the file stays a valid top-level array.
    let body: String = result
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");
    if body.trim().is_empty() {
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str("[]\n");
    }
    result
}

// ---------------------------------------------------------------------------
// Commands: install task
// ---------------------------------------------------------------------------

/// Enqueues an install task: pnpm add <pkg>@<version> into the profile dir,
/// register the bundle in package.json (dsh.profile.bundles) and cordis.patch
/// insert row for non-bundle plugins. Reuses the shared pnpm store and the
/// onlyBuiltDependencies build-script opt-in.
#[tauri::command(rename_all = "snake_case")]
pub async fn start_install_plugin_task(
    app: AppHandle,
    state: State<'_, AppState>,
    input: InstallPluginInput,
) -> Result<String, String> {
    // Validate instance + profile early.
    let (home_path, _version) = resolve_instance(&state, &input.instance_id)?;
    let dir = profile_dir(&home_path, &input.profile);
    if !dir.exists() {
        return Err(format!("Profile「{}」不存在", input.profile));
    }

    let task = crate::tasks::TaskInfo {
        id: new_id("t"),
        kind: "install-plugin".to_string(),
        label: format!(
            "安装插件 {}@{} 到「{}」的 profile「{}」",
            input.plugin_id, input.version, input.instance_id, input.profile
        ),
        version: input.version.clone(),
        state: crate::tasks::TaskState::Running,
        percent: 0,
        created_at: crate::tasks::now_millis_pub(),
        message: None,
        instance_id: Some(input.instance_id.clone()),
        instance_name: Some(input.plugin_id.clone()),
        reserved_home_path: None,
        logs: Vec::new(),
        child: None,
    };
    let task_id = task.id.clone();
    state.tasks.lock().await.insert(task_id.clone(), task);
    crate::tasks::emit_progress_pub(
        &app,
        &task_id,
        crate::tasks::TaskState::Running,
        0,
        None,
        None,
    );

    let worker_app = app.clone();
    let worker_task_id = task_id.clone();
    let input = input.clone();
    tauri::async_runtime::spawn(async move {
        let state = worker_app.state::<AppState>();
        run_install_plugin_task(&worker_app, &state, &worker_task_id, input).await;
    });

    Ok(task_id)
}

async fn run_install_plugin_task(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    input: InstallPluginInput,
) {
    let result = do_install_plugin(app, state, task_id, &input).await;
    let mut tasks = state.tasks.lock().await;
    if let Some(task) = tasks.get_mut(task_id) {
        if task.state == crate::tasks::TaskState::Cancelled {
            return;
        }
        match result {
            Ok(()) => {
                task.state = crate::tasks::TaskState::Done;
                task.percent = 100;
                crate::tasks::emit_progress_pub(
                    app,
                    task_id,
                    crate::tasks::TaskState::Done,
                    100,
                    None,
                    Some(input.instance_id.clone()),
                );
            }
            Err(msg) => {
                task.state = crate::tasks::TaskState::Error;
                task.message = Some(msg.clone());
                crate::tasks::push_log_locked_pub(task, &format!("error: {msg}"));
                let pct = task.percent;
                drop(tasks);
                crate::tasks::emit_progress_pub(
                    app,
                    task_id,
                    crate::tasks::TaskState::Error,
                    pct,
                    Some(msg),
                    None,
                );
            }
        }
    }
}

async fn do_install_plugin(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    input: &InstallPluginInput,
) -> Result<(), String> {
    let (home_path, _version) = resolve_instance(state, &input.instance_id)?;
    let dir = profile_dir(&home_path, &input.profile);

    // Spec: npm packages use <pkg>@<version>; alpha (commit) installs from the
    // GitHub repo tarball at that commit.
    let spec = match input.channel {
        PluginChannel::Alpha => {
            let catalog = fetch_plugin_market(None).await?;
            let plugin = catalog
                .iter()
                .find(|p| p.id == input.plugin_id)
                .ok_or_else(|| format!("插件 {} 不在市场中", input.plugin_id))?;
            let repo = github_repo_of(plugin)
                .ok_or_else(|| format!("插件 {} 没有 GitHub 仓库", input.plugin_id))?;
            format!("github:{repo}#{}", input.version)
        }
        _ => format!("{}@{}", input.plugin_id, input.version),
    };

    crate::tasks::push_task_log_pub(
        app,
        state,
        task_id,
        &format!("安装 {spec} 到 {}", dir.display()),
    )
    .await;

    // 1. pnpm add into the profile dir (with the build-scripts opt-in and the
    //    shared store, mirroring install_version_streamed).
    install_into_profile(app, state, task_id, &dir, &spec).await?;

    // 2. Register the bundle in the profile manifest.
    register_bundle_in_manifest(&dir, &input.plugin_id, &input.version)?;
    crate::tasks::push_task_log_pub(app, state, task_id, "已在 package.json 注册 bundle").await;

    // 3. A bundle plugin (its package.json declares `dsh.bundle`) is mounted
    //    automatically from dsh.profile.bundles — writing an additional
    //    cordis.patch.yml insert row would mount it a second time and the
    //    loader would fail with `duplicate loader entry id`. Only a plain
    //    npm package without `dsh.bundle` needs the explicit insert row.
    if is_bundle_plugin(&dir, &input.plugin_id) {
        crate::tasks::push_task_log_pub(
            app,
            state,
            task_id,
            "检测到 bundle 插件，已通过 bundles 注册（跳过 cordis insert 行）",
        )
        .await;
    } else {
        ensure_cordis_insert(&dir, &input.plugin_id)?;
        crate::tasks::push_task_log_pub(app, state, task_id, "已写入 cordis.patch.yml insert 行")
            .await;
    }

    Ok(())
}

/// Whether the installed plugin is a DSH bundle (its package.json has a
/// `dsh.bundle` section). Bundles are auto-mounted from dsh.profile.bundles.
fn is_bundle_plugin(profile_dir: &std::path::Path, plugin_id: &str) -> bool {
    let pkg_path = profile_dir
        .join("node_modules")
        .join(plugin_id)
        .join("package.json");
    let Ok(raw) = std::fs::read_to_string(&pkg_path) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    doc.pointer("/dsh/bundle").is_some()
}

/// pnpm add <spec> into a profile dir with the build-scripts opt-in.
async fn install_into_profile(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
    dir: &std::path::Path,
    spec: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建 profile 目录失败: {e}"))?;

    // Opt into dependency build scripts (pnpm>=10 disables them by default).
    let ws_manifest = dir.join("pnpm-workspace.yaml");
    if !ws_manifest.exists() {
        let content = "packages:\n  - .\nonlyBuiltDependencies:\n  - '*'\n";
        std::fs::write(&ws_manifest, content)
            .map_err(|e| format!("写入 pnpm-workspace.yaml 失败: {e}"))?;
    } else {
        // Ensure onlyBuiltDependencies is present without clobbering packages.
        let raw = std::fs::read_to_string(&ws_manifest)
            .map_err(|e| format!("读取 pnpm-workspace.yaml 失败: {e}"))?;
        if !raw.contains("onlyBuiltDependencies") {
            let merged = format!("{raw}\nonlyBuiltDependencies:\n  - '*'\n");
            std::fs::write(&ws_manifest, merged)
                .map_err(|e| format!("更新 pnpm-workspace.yaml 失败: {e}"))?;
        }
    }

    let store_dir = state.data_dir.join(".pnpm-store");
    let pnpm_prog = ensure_pnpm_for_plugins(app, state, task_id).await?;

    let mut cmd = tokio::process::Command::new(&pnpm_prog);
    crate::process::hide_console(&mut cmd);
    cmd.args(["add", spec])
        .arg("--prefix")
        .arg(dir)
        .arg("--store-dir")
        .arg(&store_dir)
        .args(["--loglevel=http"])
        .args([
            "--fetch-timeout",
            "300000",
            "--fetch-retries",
            "5",
            "--fetch-retry-maxtimeout",
            "120000",
            "--network-concurrency",
            "4",
        ]);
    if let Ok(registry) = std::env::var("DSH_NPM_REGISTRY") {
        let registry = registry.trim().to_string();
        if !registry.is_empty() {
            cmd.args(["--registry", &registry]);
        }
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    crate::tasks::run_streamed_command(app, state, task_id, cmd, "pnpm add").await
}

/// Ensure pnpm is available (delegates to the same logic as version installs).
async fn ensure_pnpm_for_plugins(
    app: &AppHandle,
    state: &State<'_, AppState>,
    task_id: &str,
) -> Result<std::path::PathBuf, String> {
    crate::tasks::ensure_pnpm_pub(app, state, task_id).await
}

/// Register the plugin in dsh.profile.bundles + dependencies.
fn register_bundle_in_manifest(
    dir: &std::path::Path,
    plugin_id: &str,
    version: &str,
) -> Result<(), String> {
    let mut manifest = read_profile_manifest(dir)?;

    // dependencies[plugin_id] = version
    if manifest.get("dependencies").is_none() {
        manifest["dependencies"] = serde_json::json!({});
    }
    manifest["dependencies"][plugin_id] = serde_json::Value::String(version.to_string());

    // dsh.profile.bundles += plugin_id
    if manifest.pointer("/dsh/profile/bundles").is_none() {
        manifest["dsh"] = serde_json::json!({ "profile": { "bundles": [] } });
    }
    let bundles = manifest
        .pointer_mut("/dsh/profile/bundles")
        .and_then(|b| b.as_array_mut())
        .ok_or_else(|| "package.json 缺少 dsh.profile.bundles".to_string())?;
    let exists = bundles.iter().any(|b| b.as_str() == Some(plugin_id));
    if !exists {
        bundles.push(serde_json::Value::String(plugin_id.to_string()));
    }

    write_profile_manifest(dir, &manifest)
}

/// Ensure cordis.patch.yml has an insert row for the plugin (non-bundle
/// plugins need an explicit mount row).
///
/// The file is a top-level YAML array. A fresh profile ships as comments +
/// a `[]` empty-array placeholder — we must replace that placeholder with a
/// block sequence (`- insert: ...`) instead of appending to it (appending
/// would produce two YAML documents and fail to parse).
fn ensure_cordis_insert(dir: &std::path::Path, plugin_id: &str) -> Result<(), String> {
    let cordis_id = cordis_id_of(plugin_id);
    let path = dir.join("cordis.patch.yml");
    let raw = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| format!("读取 cordis.patch.yml 失败: {e}"))?
    } else {
        String::new()
    };
    // Already mounted (insert row or a config block for the id)?
    if raw.contains(&format!("id: {cordis_id}")) {
        return Ok(());
    }

    let entry = format!("- insert:\n    - id: {cordis_id}\n      name: '{plugin_id}'\n");

    // Strip comment lines and blank lines to find the actual document body.
    let body: String = raw
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body_trimmed = body.trim();

    let out = if body_trimmed.is_empty() || body_trimmed == "[]" {
        // Empty document / empty-array placeholder: keep the comment header
        // and replace the `[]` with the new entry.
        let header: String = raw
            .lines()
            .take_while(|l| {
                let t = l.trim();
                t.is_empty() || t.starts_with('#')
            })
            .collect::<Vec<_>>()
            .join("\n");
        if header.trim().is_empty() {
            entry
        } else {
            format!("{}\n{}", header.trim_end(), entry)
        }
    } else {
        // Real entries exist: append a block entry.
        format!("{}\n{}", raw.trim_end(), entry)
    };

    std::fs::write(&path, out).map_err(|e| format!("写入 cordis.patch.yml 失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cordis_id_of_strips_scope_and_org() {
        assert_eq!(cordis_id_of("@dsh-plugin/dsh-auxiliary"), "dsh-auxiliary");
        assert_eq!(cordis_id_of("@dsh-external/dsh-sidechain"), "dsh-sidechain");
        assert_eq!(cordis_id_of("dsh-better-sidebar"), "dsh-better-sidebar");
        assert_eq!(cordis_id_of("@canglongcl/dsh-web-review"), "dsh-web-review");
    }

    #[test]
    fn set_disabled_row_adds_and_removes() {
        let raw = "# comment\n- id: other-plugin\n  config:\n    a: 1\n";
        // Add a disable row for dsh-auxiliary.
        let out = set_disabled_row(raw, "dsh-auxiliary", false);
        assert!(out.contains("- id: dsh-auxiliary"), "out: {out}");
        assert!(out.contains("  disabled: true"), "out: {out}");
        // The unrelated block must be preserved.
        assert!(out.contains("other-plugin"), "out: {out}");
        assert!(out.contains("config"), "out: {out}");
        assert!(out.contains("a: 1"), "out: {out}");

        // Remove it again -> back to the original content.
        let back = set_disabled_row(&out, "dsh-auxiliary", true);
        assert!(!back.contains("dsh-auxiliary"), "back: {back}");
        assert!(back.contains("other-plugin"), "back: {back}");
        assert!(back.contains("config"), "back: {back}");
    }

    #[test]
    fn set_disabled_row_replaces_existing() {
        let raw = "- id: dsh-auxiliary\n  disabled: true\n";
        let out = set_disabled_row(raw, "dsh-auxiliary", true);
        assert!(!out.contains("dsh-auxiliary"), "out: {out}");
        // Re-disable after removal.
        let out2 = set_disabled_row(&out, "dsh-auxiliary", false);
        assert!(out2.contains("- id: dsh-auxiliary"), "out2: {out2}");
        assert!(out2.contains("  disabled: true"), "out2: {out2}");
    }

    #[test]
    fn read_disabled_ids_parses_blocks() {
        let dir = std::env::temp_dir().join(format!("dsh-plugins-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("cordis.patch.yml"),
            "# header\n- id: ui-dsh-aionui-panel\n  disabled: true\n\n- id: live-stats\n  disabled: true\n\n- id: keep\n  config:\n    x: 1\n",
        )
        .unwrap();
        let set = read_disabled_ids(&dir);
        assert!(set.contains("ui-dsh-aionui-panel"));
        assert!(set.contains("live-stats"));
        assert!(!set.contains("keep"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn manifest_register_bundle() {
        let dir = std::env::temp_dir().join(format!("dsh-plugins-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // No manifest yet: write one via register.
        register_bundle_in_manifest(&dir, "@dsh-plugin/dsh-auxiliary", "^0.5.1").unwrap();
        let m = read_profile_manifest(&dir).unwrap();
        assert_eq!(m["dependencies"]["@dsh-plugin/dsh-auxiliary"], "^0.5.1");
        assert_eq!(
            m["dsh"]["profile"]["bundles"][0],
            "@dsh-plugin/dsh-auxiliary"
        );
        // Register again: no duplicate bundle entry.
        register_bundle_in_manifest(&dir, "@dsh-plugin/dsh-auxiliary", "^0.5.1").unwrap();
        let m2 = read_profile_manifest(&dir).unwrap();
        assert_eq!(m2["dsh"]["profile"]["bundles"].as_array().unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ensure_cordis_insert_only_once() {
        let dir = std::env::temp_dir().join(format!("dsh-plugins-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        ensure_cordis_insert(&dir, "@dsh-plugin/dsh-auxiliary").unwrap();
        ensure_cordis_insert(&dir, "@dsh-plugin/dsh-auxiliary").unwrap();
        let raw = std::fs::read_to_string(dir.join("cordis.patch.yml")).unwrap();
        assert_eq!(raw.matches("- insert:").count(), 1, "raw: {raw}");
        assert!(raw.contains("id: dsh-auxiliary"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_bundle_plugin_detects_dsh_bundle() {
        let dir = std::env::temp_dir().join(format!("dsh-plugins-test-{}", uuid::Uuid::new_v4()));
        let pkg_dir = dir
            .join("node_modules")
            .join("@dsh-plugin")
            .join("dsh-auxiliary");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        // No package.json yet -> not a bundle.
        assert!(!is_bundle_plugin(&dir, "@dsh-plugin/dsh-auxiliary"));
        // Bundle plugin: dsh.bundle present.
        std::fs::write(
            pkg_dir.join("package.json"),
            r#"{"name":"@dsh-plugin/dsh-auxiliary","version":"0.5.1","dsh":{"bundle":{"patch":"./cordis.patch.yml"}}}"#,
        )
        .unwrap();
        assert!(is_bundle_plugin(&dir, "@dsh-plugin/dsh-auxiliary"));
        // Plain package without dsh.bundle -> not a bundle.
        let plain_dir = dir.join("node_modules").join("@dsh-plugin").join("plain");
        std::fs::create_dir_all(&plain_dir).unwrap();
        std::fs::write(
            plain_dir.join("package.json"),
            r#"{"name":"@dsh-plugin/plain","version":"1.0.0"}"#,
        )
        .unwrap();
        assert!(!is_bundle_plugin(&dir, "@dsh-plugin/plain"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ensure_cordis_insert_replaces_empty_array_placeholder() {
        // The real-world bug: a fresh profile ships comments + `[]` and the
        // first insert row must REPLACE `[]`, not append after it (two YAML
        // documents would otherwise fail to parse).
        let dir = std::env::temp_dir().join(format!("dsh-plugins-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("cordis.patch.yml"),
            "# cordis.patch.yml\n# a top-level YAML array of load-order\n# overrides\n[]\n",
        )
        .unwrap();
        ensure_cordis_insert(&dir, "@dsh-plugin/dsh-auxiliary").unwrap();
        let raw = std::fs::read_to_string(dir.join("cordis.patch.yml")).unwrap();
        assert!(raw.contains("# cordis.patch.yml"), "header kept: {raw}");
        assert!(!raw.contains("[]"), "placeholder replaced: {raw}");
        assert!(raw.contains("- insert:"), "raw: {raw}");
        assert!(raw.contains("id: dsh-auxiliary"), "raw: {raw}");
        // The body must be a single valid block sequence.
        let body: String = raw
            .lines()
            .filter(|l| !l.trim().starts_with('#') && !l.trim().is_empty())
            .collect();
        assert!(body.starts_with("- insert:"), "single sequence: {body}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_disabled_row_replaces_empty_array_placeholder() {
        let raw = "# header\n# comment\n[]\n";
        let out = set_disabled_row(raw, "dsh-auxiliary", false);
        assert!(out.contains("# header"), "header kept: {out}");
        assert!(!out.contains("[]"), "placeholder dropped: {out}");
        assert!(out.contains("- id: dsh-auxiliary"), "out: {out}");
        assert!(out.contains("  disabled: true"), "out: {out}");
    }

    #[test]
    fn set_disabled_row_empty_again_restores_placeholder() {
        // Disabling then re-enabling the only entry should leave a valid
        // document again (comment header + `[]`), not an empty file.
        let raw = "# header\n[]\n";
        let off = set_disabled_row(raw, "dsh-auxiliary", false);
        assert!(!off.contains("[]"));
        let on = set_disabled_row(&off, "dsh-auxiliary", true);
        assert!(on.contains("[]"), "placeholder restored: {on}");
        assert!(!on.contains("dsh-auxiliary"), "entry removed: {on}");
    }

    #[test]
    fn relationship_type_alias_roundtrip() {
        // The market JSON uses `type`, the frontend expects `kind`.
        let raw = r#"{"type":"dependency","id":"@dsh-plugin/dsh-loader","versions":">=1.3.0"}"#;
        let rel: MarketPluginRelationship = serde_json::from_str(raw).unwrap();
        assert_eq!(rel.kind, "dependency");
        assert_eq!(rel.id, "@dsh-plugin/dsh-loader");
        // Serialized back out it must be `kind` (frontend contract).
        let out = serde_json::to_string(&rel).unwrap();
        assert!(out.contains("\"kind\":\"dependency\""), "out: {out}");
        assert!(!out.contains("\"type\":"), "out: {out}");
    }

    // Live network smoke tests (skipped by default; run with
    // `cargo test plugins::tests::live_ -- --ignored`).
    #[tokio::test]
    #[ignore]
    async fn live_fetch_market_and_versions() {
        let plugins = fetch_plugin_market(None).await.unwrap();
        assert!(!plugins.is_empty(), "market must return plugins");
        // The catalog must contain the loader plugin.
        assert!(
            plugins.iter().any(|p| p.id == "@dsh-plugin/dsh-loader"),
            "loader missing from market"
        );
        // Every relationship must round-trip to `kind` for the frontend.
        for p in &plugins {
            if let Some(rels) = &p.relationship {
                for r in rels {
                    let out = serde_json::to_string(r).unwrap();
                    assert!(
                        out.contains("\"kind\":"),
                        "relationship of {} must serialize kind: {out}",
                        p.id
                    );
                    assert!(
                        !out.contains("\"type\":"),
                        "relationship of {} must not leak `type`: {out}",
                        p.id
                    );
                }
            }
        }
        // npm-based stable versions for a known plugin.
        let stable = npm_versions("@dsh-plugin/dsh-auxiliary", &PluginChannel::Stable)
            .await
            .unwrap();
        assert!(!stable.is_empty());
        assert!(stable.iter().any(|v| v.is_default));
        let beta = npm_versions("@dsh-plugin/dsh-auxiliary", &PluginChannel::Beta)
            .await
            .unwrap();
        assert!(!beta.is_empty());
        // alpha: GitHub commit channel (client_id boosts the rate limit).
        let page1 = fetch_plugin_versions(
            "@dsh-plugin/dsh-auxiliary".to_string(),
            PluginChannel::Alpha,
            Some(1),
        )
        .await
        .unwrap();
        assert!(
            !page1.versions.is_empty(),
            "alpha commits must be fetchable"
        );
        assert!(page1.versions[0].is_default, "first commit is the default");
        // Pagination: page 2 must return a disjoint set when has_more.
        if page1.has_more {
            let page2 = fetch_plugin_versions(
                "@dsh-plugin/dsh-auxiliary".to_string(),
                PluginChannel::Alpha,
                Some(2),
            )
            .await
            .unwrap();
            assert!(!page2.versions.is_empty());
            assert!(
                page2
                    .versions
                    .iter()
                    .all(|v| !page1.versions.iter().any(|a| a.version == v.version)),
                "page 2 must not repeat page 1 commits"
            );
            assert!(!page2.versions[0].is_default, "only page 1 has the default");
        }
    }
}
