//! Launcher proxy support (settings → network clients and instance env).
//!
//! The `proxy_enabled` settings toggle routes the launcher's own HTTP
//! requests (market, registry, downloads, news, update checks) through the
//! configured proxy; `proxy_apply_dsh` additionally injects the proxy into
//! launched dsh instances as HTTP(S)_PROXY / NO_PROXY environment variables,
//! overriding the instance's own overrides on the next start.

use crate::config::LauncherSettings;
use std::sync::RwLock;

#[derive(Clone, Debug)]
struct ProxyConf {
    /// `scheme://host:port` passed to `reqwest::Proxy::all`.
    server: String,
    /// Comma-separated NO_PROXY list.
    no_proxy: String,
}

fn conf_from_settings(s: &LauncherSettings) -> Option<ProxyConf> {
    let url = s.proxy_url.trim().trim_end_matches('/');
    if !s.proxy_enabled || url.is_empty() {
        return None;
    }
    Some(ProxyConf {
        server: format!("{url}:{}", s.proxy_port),
        no_proxy: s.no_proxy.trim().to_string(),
    })
}

/// Snapshot of the active proxy for HTTP client construction sites that have
/// no access to the config mutex. Synced at startup and on every settings
/// update.
static CURRENT: RwLock<Option<ProxyConf>> = RwLock::new(None);

/// Refreshes the global snapshot from (possibly new) settings.
pub fn sync_from_settings(settings: &LauncherSettings) {
    *CURRENT.write().unwrap() = conf_from_settings(settings);
}

/// Applies the configured proxy (if enabled) to a reqwest client builder.
/// Invalid proxy configuration is ignored rather than breaking the request.
pub fn apply(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    let guard = CURRENT.read().unwrap();
    let Some(conf) = guard.as_ref() else {
        return builder;
    };
    match reqwest::Proxy::all(&conf.server) {
        Ok(proxy) => builder.proxy(proxy.no_proxy(reqwest::NoProxy::from_string(&conf.no_proxy))),
        Err(e) => {
            crate::log_warn!("代理地址无效，忽略代理设置 {}: {e}", conf.server);
            builder
        }
    }
}

/// Proxy environment variable names managed by [`override_env`].
const PROXY_KEYS: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
];

/// Injects the proxy into an instance environment, replacing any proxy
/// variables the instance already defines (both cases). Callers check
/// `proxy_enabled && proxy_apply_dsh` first.
pub fn override_env(env: &mut Vec<(String, String)>, settings: &LauncherSettings) {
    let Some(conf) = conf_from_settings(settings) else {
        return;
    };
    env.retain(|(k, _)| !PROXY_KEYS.iter().any(|p| p.eq_ignore_ascii_case(k)));
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
        env.push((key.to_string(), conf.server.clone()));
    }
    env.push(("NO_PROXY".to_string(), conf.no_proxy.clone()));
    env.push(("no_proxy".to_string(), conf.no_proxy));
}
