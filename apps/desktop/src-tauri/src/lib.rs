mod app_server;
pub mod runtime_status;

use app_server::process::AppServerState;
use runtime_status::RuntimeStatus;
use tauri::AppHandle;
use tauri::Manager;
use tauri::State;

const COMMAND_ERROR_MESSAGE: &str = "核心服务暂时无法连接。";

#[tauri::command]
fn get_runtime_status(state: State<'_, AppServerState>) -> RuntimeStatus {
    state.current_status()
}

#[tauri::command]
async fn retry_app_server(app_handle: AppHandle) -> RuntimeStatus {
    tauri::async_runtime::spawn_blocking(move || {
        app_handle
            .try_state::<AppServerState>()
            .map(|state| state.retry())
            .unwrap_or_else(command_error_status)
    })
    .await
    .unwrap_or_else(|_| command_error_status())
}

fn command_error_status() -> RuntimeStatus {
    RuntimeStatus::Error {
        message: COMMAND_ERROR_MESSAGE.to_string(),
        retryable: true,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            retry_app_server
        ])
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
