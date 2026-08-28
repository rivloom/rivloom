mod account;
mod app_server;
pub mod runtime_status;

use account::AccountCommand;
use account::AccountState;
use account::AccountStatus;
use app_server::state::AppServerState;
use runtime_status::RuntimeStatus;
use tauri::AppHandle;
use tauri::Manager;
use tauri::State;

const COMMAND_ERROR_MESSAGE: &str = "核心服务暂时无法连接。";
const ACCOUNT_COMMAND_ERROR_MESSAGE: &str = "账号状态暂时不可用。";

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

#[tauri::command]
async fn get_account_status(app_handle: AppHandle) -> AccountStatus {
    run_account_command(app_handle, AccountCommand::GetStatus).await
}

#[tauri::command]
async fn start_chatgpt_login(app_handle: AppHandle) -> AccountStatus {
    run_account_command(app_handle, AccountCommand::StartChatgptLogin).await
}

#[tauri::command]
async fn cancel_account_login(app_handle: AppHandle) -> AccountStatus {
    run_account_command(app_handle, AccountCommand::CancelLogin).await
}

#[tauri::command]
async fn logout_account(app_handle: AppHandle) -> AccountStatus {
    run_account_command(app_handle, AccountCommand::Logout).await
}

async fn run_account_command(app_handle: AppHandle, command: AccountCommand) -> AccountStatus {
    tauri::async_runtime::spawn_blocking(move || {
        app_handle
            .try_state::<AccountState>()
            .map(|state| state.execute(command))
            .unwrap_or_else(account_command_error_status)
    })
    .await
    .unwrap_or_else(|_| account_command_error_status())
}

fn command_error_status() -> RuntimeStatus {
    RuntimeStatus::Error {
        message: COMMAND_ERROR_MESSAGE.to_string(),
        retryable: true,
    }
}

fn account_command_error_status() -> AccountStatus {
    AccountStatus::Error {
        message: ACCOUNT_COMMAND_ERROR_MESSAGE.to_string(),
        retryable: true,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            retry_app_server,
            get_account_status,
            start_chatgpt_login,
            cancel_account_login,
            logout_account,
        ])
        .setup(|app| {
            let codex_home = app.path().app_local_data_dir()?.join("codex-home");
            let account_state = AccountState::new(app.handle().clone());
            let account_service = account_state.service();
            if !app.manage(account_state) {
                return Err(std::io::Error::other("Account state was already managed").into());
            }
            let state = AppServerState::new(app.handle().clone(), codex_home, account_service);
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
