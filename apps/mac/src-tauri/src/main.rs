mod agent_docs;
mod attachments;
mod agent_settings;
mod failure;
// The WebView is a thin client; memory, inference and persistence stay in Rust.
mod agent;
mod backend;
mod beacon;
mod cloud;
mod cloud_models;
mod cloud_settings;
mod computer;
mod delegation;
mod blender;
mod conversations;
mod generation;
mod mcp;
mod local_beacon;
mod model_config;
#[cfg(target_os = "macos")]
mod notifications;
mod privacy;
mod qr;
mod recall;
mod routing;
mod service;
mod standalone;
mod sync;
mod usage;
mod verification;
mod web;
mod workspace;
use tauri::Manager;

fn main() {
    let _bundled_services = standalone::prepare().expect("Standalone runtime setup failed");
    // Require an explicit dotenv path. Never search arbitrary parent directories.
    if let Some(path) = std::env::var_os("FABRIC_ENV_FILE") {
        dotenvy::from_path(path).expect("could not load backend environment file");
    }
    if std::env::args()
        .any(|arg| arg == "--check" || arg == "--sync-once" || arg == "--prompt-stdin")
    {
        let path =
            std::env::var_os("FABRIC_DATA_DIR").expect("set FABRIC_DATA_DIR for backend check");
        let backend = backend::Backend::open(path.into()).expect("backend initialization failed");
        if std::env::args().any(|arg| arg == "--prompt-stdin") {
            use std::io::Read;
            let mut text = String::new();
            std::io::stdin()
                .take(4001)
                .read_to_string(&mut text)
                .expect("read prompt");
            let mode = serde_json::from_value(serde_json::json!(
                std::env::var("FABRIC_MODE").unwrap_or_else(|_| "local".into())
            ))
            .expect("FABRIC_MODE must be local, cloud, or auto");
            let web = serde_json::from_value(serde_json::json!(
                std::env::var("FABRIC_WEB").unwrap_or_else(|_| "off".into())
            ))
            .expect("FABRIC_WEB must be off, auto, or on");
            println!(
                "{}",
                backend
                    .answer_with_options(text, mode, web)
                    .expect("prompt failed")
            );
        } else if std::env::args().any(|arg| arg == "--sync-once") {
            println!("{}", backend.sync_once().expect("sync failed"));
        } else {
            println!("{}", backend.status_json().expect("backend status failed"));
        }
        return;
    }
    tauri::Builder::default()
        .setup(|app| {
            let backend = backend::Backend::open(app.path().app_data_dir()?)?;
            app.manage(backend);
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.set_focus()?;
            }
            #[cfg(target_os = "macos")]
            {
                notifications::request_permission();
                use tauri::menu::{Menu, MenuItem};
                let status = MenuItem::with_id(app, "screen_control_status", "Screen idle", false, None::<&str>)?;
                let menu = Menu::with_items(app, &[&status])?;
                const MENU_ICON: &[u8; 36 * 36 * 4] = include_bytes!("../icons/menu-template.rgba");
                let icon = tauri::image::Image::new(MENU_ICON, 36, 36);
                let tray = tauri::tray::TrayIconBuilder::with_id("memory-fabric")
                    .menu(&menu)
                    .icon(icon)
                    .icon_as_template(true)
                    .tooltip("Fabric Memory: screen idle")
                    .build(app)?;
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let mut active = false;
                    loop {
                        let current = handle.try_state::<backend::Backend>()
                            .is_some_and(|backend| backend.screen_active());
                        if current != active {
                            active = current;
                            let tooltip = if active { "Fabric Memory: controlling the screen" } else { "Fabric Memory: screen idle" };
                            let _ = tray.set_tooltip(Some(tooltip));
                            let _ = status.set_text(if active { "Controlling the screen" } else { "Screen idle" });
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                });
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                if let Some(backend) = handle.try_state::<backend::Backend>() {
                    backend.sync_automatically();
                    backend.sync_wake.wait(std::time::Duration::from_secs(10));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![service::app_command])
        .run(tauri::generate_context!())
        .expect("could not run fabric-memory");
}
