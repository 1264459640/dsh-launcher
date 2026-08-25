use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Opens (or focuses) the webview window hosting the instance's DSH Web GUI.
pub fn open_instance_window(app: &AppHandle, instance_id: &str, name: &str, url: &str) -> Result<(), String> {
    let label = format!("instance-{instance_id}");
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_focus();
        return Ok(());
    }
    let parsed = WebviewUrl::External(url.parse().map_err(|e| format!("无效的 URL {url}: {e}"))?);
    WebviewWindowBuilder::new(app, label, parsed)
        .title(format!("{name} — DSH"))
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Closes the instance's webview window if it is open.
pub fn close_instance_window(app: &AppHandle, instance_id: &str) {
    let label = format!("instance-{instance_id}");
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.close();
    }
}
