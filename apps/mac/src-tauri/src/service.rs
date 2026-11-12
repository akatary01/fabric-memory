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
        let result = app.state::<crate::backend::Backend>().command(&command, &args);
        #[cfg(target_os = "macos")]
        if command == "send_prompt" && !app.get_webview_window("main")
            .is_some_and(|window| window.is_focused().unwrap_or(false)) {
            let title = result.as_ref().ok().and_then(|answer| answer["conversation_title"].as_str())
                .unwrap_or("Memory Fabric");
            let message = if result.as_ref().is_ok_and(|answer| answer["failure"].is_null()) {
                "Task complete"
            } else { "Task ended. Open Fabric for details." };
            let _ = std::process::Command::new("/usr/bin/osascript")
                .args(["-e", "on run argv", "-e", "display notification (item 1 of argv) with title \"Memory Fabric\" subtitle (item 2 of argv)", "-e", "end run", "--", message, title])
                .status();
        }
        result
    })
    .await
    .map_err(|error| format!("The native operation failed: {error}"))?
}
