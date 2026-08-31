use super::super::test_support::Memory;
use super::*;
use crate::identity::IdentityStore;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::net::TcpListener;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindowBuilder};
use tempfile::TempDir;

fn app(temp: &TempDir) -> App<MockRuntime> {
    let app = mock_builder()
        .invoke_handler(crate::invoke_handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    let backend: Arc<dyn SecretBackend + Send + Sync> = Arc::new(Memory::default());
    assert!(app.manage(DesktopBrainState {
        service: BrainService::new(temp.path().join("brain-host"), backend).unwrap()
    }));
    assert!(app.manage(IdentityService::new(IdentityStore::new(
        temp.path().join("identity-v1.json")
    ))));
    app
}

fn invoke(window: &WebviewWindow<MockRuntime>, command: &str, body: Value) -> Result<Value, Value> {
    tauri::test::get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .map(|response| response.deserialize::<Value>().unwrap())
}

#[test]
fn actual_handler_provisions_starts_and_stops_without_a_runtime_state() {
    let temp = TempDir::new().unwrap();
    let app = app(&temp);
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(&window, "get_local_brain_status", json!({})),
        Ok(json!({"state":"notConfigured"}))
    );
    assert!(!temp.path().join("brain-host").exists());
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let profile = invoke(
        &window,
        "initialize_local_brain",
        json!({"params":{
            "address":reservation.local_addr().unwrap().to_string(),"serverName":"localhost"
        }}),
    )
    .unwrap();
    let identity = app.state::<IdentityService>().get().unwrap();
    assert_eq!(profile["binding"]["deviceId"], identity.device_id);
    assert_eq!(
        invoke(&window, "get_local_brain_status", json!({})),
        Ok(json!({"state":"stopped","profile":profile}))
    );
    drop(reservation);
    assert_eq!(
        invoke(&window, "start_local_brain", json!({})),
        Ok(json!({"state":"running","profile":profile}))
    );
    assert_eq!(
        invoke(&window, "start_local_brain", json!({})),
        Err(json!("busy"))
    );
    assert_eq!(
        invoke(&window, "stop_local_brain", json!({})),
        Ok(Value::Null)
    );
    assert_eq!(
        invoke(&window, "get_local_brain_status", json!({})),
        Ok(json!({"state":"stopped","profile":profile}))
    );
    app.state::<DesktopBrainState>().shutdown();
    assert_eq!(
        invoke(&window, "start_local_brain", json!({})),
        Err(json!("unavailable"))
    );
}

#[test]
fn secondary_windows_and_unbounded_or_path_bearing_parameters_are_rejected() {
    let temp = TempDir::new().unwrap();
    let app = app(&temp);
    let other = WebviewWindowBuilder::new(&app, "secondary", Default::default())
        .build()
        .unwrap();
    let body = json!({"params":{"address":"127.0.0.1:7443","serverName":"localhost"}});
    assert_eq!(
        invoke(&other, "initialize_local_brain", body.clone()),
        Err(json!("invalid"))
    );
    assert!(!temp.path().join("brain-host").exists());
    let main = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for address in [
        "0.0.0.0:7443",
        "8.8.8.8:7443",
        "127.0.0.1:0",
        "https://localhost:7443",
    ] {
        assert!(
            invoke(
                &main,
                "initialize_local_brain",
                json!({"params":{"address":address,"serverName":"localhost"}})
            )
            .is_err()
        );
        assert!(!temp.path().join("brain-host").exists());
    }
    for field in [
        "directory",
        "identityId",
        "deviceId",
        "secret",
        "privateKey",
    ] {
        let mut invalid = body.clone();
        invalid["params"][field] = json!("caller-controlled");
        assert!(invoke(&main, "initialize_local_brain", invalid).is_err());
        assert!(!temp.path().join("brain-host").exists());
    }
    assert_eq!(
        invoke(
            &main,
            "initialize_local_brain",
            json!({"params":{"address":"127.0.0.1:7443","serverName":"x".repeat(254)}})
        ),
        Err(json!("invalid"))
    );
}

#[test]
fn missing_managed_state_is_a_sanitized_error_not_an_implicit_service() {
    let app = mock_builder()
        .invoke_handler(crate::invoke_handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for command in [
        "get_local_brain_status",
        "start_local_brain",
        "stop_local_brain",
    ] {
        assert_eq!(
            invoke(&window, command, json!({})),
            Err(json!("unavailable"))
        );
    }
    let temp = TempDir::new().unwrap();
    let backend: Arc<dyn SecretBackend + Send + Sync> = Arc::new(Memory::default());
    assert!(app.manage(DesktopBrainState {
        service: BrainService::new(temp.path().join("brain-host"), backend).unwrap()
    }));
    assert_eq!(
        invoke(
            &window,
            "initialize_local_brain",
            json!({"params":{"address":"127.0.0.1:7443","serverName":"localhost"}})
        ),
        Err(json!("unavailable"))
    );
    assert!(!temp.path().join("brain-host").exists());
}

#[test]
fn corrupt_or_incomplete_setup_is_reported_without_reset_or_error_body_leakage() {
    let temp = TempDir::new().unwrap();
    let app = app(&temp);
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let directory = temp.path().join("brain-host");
    std::fs::create_dir(&directory).unwrap();
    assert_eq!(
        invoke(&window, "get_local_brain_status", json!({})),
        Err(json!("incomplete"))
    );
    let marker = b"private-malformed-input-that-must-not-be-returned";
    std::fs::write(directory.join("host-v1.json"), marker).unwrap();
    assert_eq!(
        invoke(&window, "get_local_brain_status", json!({})),
        Err(json!("invalid"))
    );
    assert_eq!(
        invoke(&window, "start_local_brain", json!({})),
        Err(json!("invalid"))
    );
    assert_eq!(
        invoke(
            &window,
            "initialize_local_brain",
            json!({"params":{"address":"127.0.0.1:7443","serverName":"localhost"}})
        ),
        Err(json!("existing"))
    );
    assert_eq!(
        std::fs::read(directory.join("host-v1.json")).unwrap(),
        marker
    );
}
