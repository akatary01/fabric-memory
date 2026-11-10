// Native IPC is the only UI transport. The model and ledger remain in Rust.
use tauri::Manager;

#[tauri::command]
pub async fn app_command(
    app: tauri::AppHandle,
    command: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if command.starts_with("window_") {
        let window = app.get_webview_window("main").ok_or("Missing app window")?;
        match command.as_str() {
            "window_minimize" => window.minimize(),
            "window_maximize" => {
                if window.is_maximized().map_err(|e| e.to_string())? {
                    window.unmaximize()
                } else {
                    window.maximize()
                }
            }
            "window_close" => window.close(),
            "window_drag" => window.start_dragging(),
            _ => return Err("Unknown window action".into()),
        }
        .map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::Null);
    }
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::backend::Backend>()
            .command(&command, &args)
    })
    .await
    .map_err(|error| format!("The native operation failed: {error}"))?
}
