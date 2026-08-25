use serde::Serialize;
use tauri::State;

use crate::AppState;

#[derive(Clone, Debug, Serialize)]
pub struct ToolStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatus {
    pub node: ToolStatus,
    pub pnpm: ToolStatus,
}

async fn probe(program: &str) -> ToolStatus {
    let mut cmd = tokio::process::Command::new(program);
    crate::process::hide_console(&mut cmd);
    let output = cmd.arg("--version").output().await;

    match output {
        Ok(out) if out.status.success() => ToolStatus {
            installed: true,
            version: Some(String::from_utf8_lossy(&out.stdout).trim().to_string()),
            path: None,
        },
        _ => ToolStatus {
            installed: false,
            version: None,
            path: None,
        },
    }
}

#[tauri::command]
pub async fn get_runtime_status(_state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let node = probe("node").await;
    let pnpm = probe("pnpm").await;
    Ok(RuntimeStatus { node, pnpm })
}
