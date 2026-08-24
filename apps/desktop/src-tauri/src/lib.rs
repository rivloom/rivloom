mod app_server;
pub mod runtime_status;

use app_server::process::AppServerState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let codex_home = app.path().app_local_data_dir()?.join("codex-home");
            let state = AppServerState::new(app.handle().clone(), codex_home);
            if !app.manage(state) {
                return Err(std::io::Error::other("App Server state was already managed").into());
            }

            let app_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("rivloom-app-server-startup".to_string())
                .spawn(move || {
                    if let Some(state) = app_handle.try_state::<AppServerState>() {
                        state.start();
                    }
                })?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Rivloom desktop application");

    app.run(|app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) && let Some(state) = app_handle.try_state::<AppServerState>()
        {
            state.shutdown();
        }
    });
}
