use super::super::hosting::BrainService;
use super::super::test_support::{Memory, fixture};
use super::super::trust::TrustDescriptor;
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
    assert!(app.manage(DesktopNodeState {
        session: NodeSession::new(temp.path().join("node-client"), backend.clone()).unwrap()
    }));
    assert!(app.manage(DesktopBrainState {
        service: BrainService::new(temp.path().join("brain-host"), backend).unwrap()
    }));
    assert!(app.manage(IdentityService::new(IdentityStore::new(
        temp.path().join("identity-v1.json")
    ))));
    app
}
fn window(app: &App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .unwrap()
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
fn two_apps_use_actual_handlers_to_host_join_reconnect_cancel_and_revoke_without_runtime_state() {
    let alice_temp = TempDir::new().unwrap();
    let bob_temp = TempDir::new().unwrap();
    let alice_app = app(&alice_temp);
    let bob_app = app(&bob_temp);
    let alice = window(&alice_app, "main");
    let bob = window(&bob_app, "main");
    assert_eq!(
        invoke(&bob, "get_node_status", json!({})),
        Ok(json!({"state":"notConfigured","registration":null,"binding":null,"revision":0}))
    );
    assert!(!bob_temp.path().join("node-client").exists());
    let port = TcpListener::bind("127.0.0.1:0").unwrap();
    let profile = invoke(&alice, "initialize_local_brain", json!({"params":{"address":port.local_addr().unwrap().to_string(),"serverName":"localhost"}})).unwrap();
    let descriptor =
        TrustDescriptor::decode(&serde_json::to_vec(&profile["descriptor"]).unwrap()).unwrap();
    let fingerprint = descriptor.fingerprint();
    drop(port);
    invoke(&alice, "start_local_brain", json!({})).unwrap();
    invoke(
        &alice,
        "connect_brain_owner",
        json!({"params":{"confirmedFingerprint":fingerprint}}),
    )
    .unwrap();
    let invitation = invoke(&alice, "create_brain_invitation", json!({})).unwrap();
    let canceled = invoke(&alice, "create_brain_invitation", json!({})).unwrap();
    assert_eq!(
        invoke(
            &alice,
            "cancel_brain_invitation",
            json!({"params":{"invitationId":canceled["invitationId"]}})
        ),
        Ok(Value::Null)
    );
    let joined = invoke(&bob, "join_brain", json!({"params":{"descriptor":serde_json::to_string(&descriptor).unwrap(),"confirmedFingerprint":fingerprint,"invitation":invitation}})).unwrap();
    assert_eq!(
        joined["binding"]["deviceId"],
        bob_app.state::<IdentityService>().get().unwrap().device_id
    );
    assert_eq!(joined["state"], "connected");
    invoke(&bob, "disconnect_brain", json!({})).unwrap();
    let reconnected = invoke(&bob, "connect_brain", json!({})).unwrap();
    assert_eq!(reconnected["binding"], joined["binding"]);
    invoke(&alice, "refresh_brain", json!({})).unwrap();
    let directory = invoke(&alice, "list_brain_members", json!({})).unwrap();
    assert_eq!(
        directory["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["type"] == "member")
            .count(),
        2
    );
    for value in [&joined, &directory] {
        assert!(
            !value
                .to_string()
                .contains(invitation["secret"].as_str().unwrap())
        );
    }
    assert_eq!(
        invoke(
            &alice,
            "revoke_brain_member",
            json!({"params":{"memberId":joined["binding"]["memberId"]}})
        ),
        Ok(Value::Null)
    );
    assert_eq!(
        invoke(&bob, "refresh_brain", json!({})),
        Err(json!("rejected"))
    );
    alice_app.state::<DesktopNodeState>().shutdown();
    bob_app.state::<DesktopNodeState>().shutdown();
    assert_eq!(
        invoke(&alice, "connect_brain", json!({})),
        Err(json!("unavailable"))
    );
    invoke(&alice, "stop_local_brain", json!({})).unwrap();
}

#[test]
fn bad_confirmation_invitation_and_caller_owned_fields_cannot_create_registration() {
    let f = fixture();
    let temp = TempDir::new().unwrap();
    let app = app(&temp);
    let main = window(&app, "main");
    let body = json!({"params":{"descriptor":serde_json::to_string(&f.descriptor).unwrap(),"confirmedFingerprint":f.descriptor.fingerprint(),"invitation":{"brainId":"brain-1","invitationId":"a".repeat(64),"expiresAt":f.now+300,"secret":"b".repeat(64)}}});
    let mut invalid = Vec::new();
    for (key, value) in [
        ("confirmedFingerprint", json!("0".repeat(64))),
        ("descriptor", json!("x".repeat(8193))),
    ] {
        let mut case = body.clone();
        case["params"][key] = value;
        invalid.push(case);
    }
    for (key, value) in [
        ("brainId", json!("other")),
        ("expiresAt", json!(0)),
        ("expiresAt", json!(f.now + 3600)),
        ("secret", json!("sensitive-invalid-code")),
    ] {
        let mut case = body.clone();
        case["params"]["invitation"][key] = value;
        invalid.push(case);
    }
    for key in [
        "directory",
        "identityId",
        "deviceId",
        "binding",
        "privateKey",
    ] {
        let mut case = body.clone();
        case["params"][key] = json!("caller-owned");
        invalid.push(case);
    }
    for case in invalid {
        let error = invoke(&main, "join_brain", case).unwrap_err();
        assert!(!error.to_string().contains("sensitive-invalid-code"));
        assert!(!temp.path().join("node-client").exists());
    }
}

#[test]
fn main_window_and_managed_state_are_required_and_owner_profile_cannot_be_injected() {
    let bare = mock_builder()
        .invoke_handler(crate::invoke_handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    let bare_window = window(&bare, "main");
    for command in [
        "get_node_status",
        "connect_brain",
        "refresh_brain",
        "disconnect_brain",
        "list_brain_members",
        "create_brain_invitation",
    ] {
        assert_eq!(
            invoke(&bare_window, command, json!({})),
            Err(json!("unavailable"))
        );
    }
    let temp = TempDir::new().unwrap();
    let app = app(&temp);
    let other = window(&app, "secondary");
    for command in [
        "get_node_status",
        "connect_brain",
        "disconnect_brain",
        "create_brain_invitation",
    ] {
        assert_eq!(invoke(&other, command, json!({})), Err(json!("invalid")));
    }
    let main = window(&app, "main");
    assert_eq!(
        invoke(
            &main,
            "connect_brain_owner",
            json!({"params":{"confirmedFingerprint":"0".repeat(64)}})
        ),
        Err(json!("unavailable"))
    );
    assert!(invoke(&main, "connect_brain_owner", json!({"params":{"confirmedFingerprint":"0".repeat(64),"profile":{"binding":"caller-owned"}}})).is_err());
    assert!(!temp.path().join("node-client").exists());
}

#[test]
fn corrupt_registration_is_reported_without_reset_or_storage_contents_in_errors() {
    let temp = TempDir::new().unwrap();
    let app = app(&temp);
    let main = window(&app, "main");
    let directory = temp.path().join("node-client");
    std::fs::create_dir(&directory).unwrap();
    assert_eq!(
        invoke(&main, "get_node_status", json!({})),
        Err(json!("recoveryRequired"))
    );
    let marker = b"private-malformed-registration";
    let path = directory.join("registration-v1.json");
    std::fs::write(&path, marker).unwrap();
    for command in [
        "get_node_status",
        "connect_brain",
        "refresh_brain",
        "list_brain_members",
    ] {
        assert_eq!(invoke(&main, command, json!({})), Err(json!("invalid")));
    }
    assert_eq!(std::fs::read(path).unwrap(), marker);
}
