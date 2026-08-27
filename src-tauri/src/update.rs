//! Launcher self-update check against the GitHub releases API.
//!
//! Channel follows the running build: a `-dev.<run>` build looks at every
//! published release (including prereleases, and a shipped stable outranks a
//! dev build), while a stable build only considers non-prerelease releases.

use serde::{Deserialize, Serialize};

const RELEASES_URL: &str =
    "https://api.github.com/repos/dsh-plugins/dsh-launcher/releases?per_page=50";
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct LauncherUpdateInfo {
    pub current: String,
    /// "dev" when the running build is a -dev.N prerelease, else "stable".
    pub channel: String,
    pub up_to_date: bool,
    pub latest: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ReleaseEntry {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    published_at: Option<String>,
}

/// Parses a release tag ("v0.2.0" / "v0.2.0-dev.12") as semver.
fn parse_tag(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// Picks the newest published release strictly newer than `current`.
/// Dev channel sees prereleases and stables; stable channel only stables.
fn pick_latest<'a>(
    current: &semver::Version,
    dev_channel: bool,
    releases: &'a [ReleaseEntry],
) -> Option<&'a ReleaseEntry> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter(|r| dev_channel || !r.prerelease)
        .filter_map(|r| parse_tag(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, _)| v > current)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, r)| r)
}

#[tauri::command]
pub async fn check_launcher_update() -> Result<LauncherUpdateInfo, String> {
    let current_raw = env!("CARGO_PKG_VERSION");
    let current = semver::Version::parse(current_raw)
        .map_err(|e| format!("当前版本号无效 {current_raw}: {e}"))?;
    let dev_channel = current_raw.contains("-dev.");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("dsh-launcher")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    crate::log_debug!("检查启动器更新：{RELEASES_URL}");
    let resp = client
        .get(RELEASES_URL)
        .send()
        .await
        .map_err(|e| format!("请求 GitHub Releases 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub Releases 返回 HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;
    if bytes.len() > MAX_BYTES {
        return Err("GitHub Releases 响应过大".to_string());
    }
    let releases: Vec<ReleaseEntry> =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析 releases 失败: {e}"))?;

    let latest = pick_latest(&current, dev_channel, &releases);
    match latest {
        Some(r) => crate::log_info!(
            "发现新版本 {}（当前 {current_raw}，{dev} 渠道）",
            r.tag_name,
            dev = if dev_channel { "dev" } else { "stable" }
        ),
        None => crate::log_info!("启动器已是最新（{current_raw}）"),
    }
    Ok(LauncherUpdateInfo {
        current: current_raw.to_string(),
        channel: if dev_channel { "dev" } else { "stable" }.to_string(),
        up_to_date: latest.is_none(),
        latest: latest.map(|r| r.tag_name.trim_start_matches('v').to_string()),
        url: latest.map(|r| r.html_url.clone()),
        published_at: latest.and_then(|r| r.published_at.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, draft: bool) -> ReleaseEntry {
        ReleaseEntry {
            tag_name: tag.to_string(),
            html_url: format!("https://example.com/{tag}"),
            prerelease,
            draft,
            published_at: None,
        }
    }

    fn tag_of(found: Option<&ReleaseEntry>) -> Option<String> {
        found.map(|r| r.tag_name.clone())
    }

    #[test]
    fn stable_channel_ignores_prereleases() {
        let current = semver::Version::parse("0.1.9").unwrap();
        let releases = vec![
            release("v0.2.0-dev.7", true, false),
            release("v0.1.9", false, false),
        ];
        // Only prereleases are newer: a stable build stays put.
        assert_eq!(tag_of(pick_latest(&current, false, &releases)), None);
    }

    #[test]
    fn stable_channel_finds_newer_stable() {
        let current = semver::Version::parse("0.1.9").unwrap();
        let releases = vec![
            release("v0.2.0-dev.7", true, false),
            release("v0.2.0", false, false),
        ];
        assert_eq!(
            tag_of(pick_latest(&current, false, &releases)),
            Some("v0.2.0".to_string())
        );
    }

    #[test]
    fn dev_channel_finds_newer_dev_build() {
        let current = semver::Version::parse("0.2.0-dev.3").unwrap();
        let releases = vec![
            release("v0.2.0-dev.2", true, false),
            release("v0.2.0-dev.5", true, false),
        ];
        assert_eq!(
            tag_of(pick_latest(&current, true, &releases)),
            Some("v0.2.0-dev.5".to_string())
        );
    }

    #[test]
    fn dev_channel_prefers_shipped_stable_over_dev_build() {
        let current = semver::Version::parse("0.2.0-dev.9").unwrap();
        let releases = vec![release("v0.2.0", false, false)];
        assert_eq!(
            tag_of(pick_latest(&current, true, &releases)),
            Some("v0.2.0".to_string())
        );
    }

    #[test]
    fn drafts_and_same_version_are_excluded() {
        let current = semver::Version::parse("0.2.0-dev.9").unwrap();
        let releases = vec![
            release("v0.2.0-dev.10", true, true), // draft
            release("v0.2.0-dev.9", true, false), // same as current
            release("garbage-tag", true, false),  // unparsable
        ];
        assert_eq!(tag_of(pick_latest(&current, true, &releases)), None);
    }
}
