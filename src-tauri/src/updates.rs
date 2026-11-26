use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Serialize)]
pub struct UpdateStatus {
    pub phase: String,
    pub version: Option<String>,
    pub error: Option<String>,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            version: None,
            error: None,
        }
    }
}

pub type State = Mutex<UpdateStatus>;

fn report(app: &AppHandle, phase: &str, version: Option<String>, error: Option<String>) {
    let status = UpdateStatus {
        phase: phase.into(),
        version,
        error,
    };
    if let Ok(mut state) = app.state::<State>().lock() {
        *state = status.clone();
    }
    let _ = app.emit("app-update-status", status);
}

#[tauri::command]
pub fn get_update_status(state: tauri::State<'_, State>) -> Result<UpdateStatus, String> {
    state
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "Update status unavailable".into())
}

async fn check_and_install(app: &AppHandle) -> Result<(), String> {
    report(app, "checking", None, None);
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        report(app, "current", None, None);
        return Ok(());
    };
    let version = update.version.clone();
    report(app, "downloading", Some(version.clone()), None);
    // The plugin verifies the download, and install verifies it again before execution.
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    report(app, "installing", Some(version), None);
    tokio::time::sleep(Duration::from_secs(1)).await;
    // On Windows the plugin launches the per-user installer and exits this process.
    update.install(bytes).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn start(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Err(error) = check_and_install(&app).await {
            eprintln!("Update check/install failed: {error}");
            report(&app, "error", None, Some(error));
        }
    });
}
