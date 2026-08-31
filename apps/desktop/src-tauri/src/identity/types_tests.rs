use pretty_assertions::assert_eq;
use serde_json::json;

use super::BrainMembershipRole;
use super::BrainMembershipSummary;
use super::RivloomIdentity;
use crate::account::CodexRuntimeAuthStatus;

#[test]
fn identity_and_codex_runtime_auth_have_distinct_wire_contracts() {
    let identity = RivloomIdentity {
        identity_id: "identity-local-1".to_string(),
        display_name: "Alice".to_string(),
        device_id: "device-local-1".to_string(),
        brain_membership: Some(BrainMembershipSummary {
            brain_id: "brain-1".to_string(),
            member_id: "member-1".to_string(),
            role: BrainMembershipRole::Member,
        }),
    };
    let runtime_auth = CodexRuntimeAuthStatus::SignedIn {
        email: Some("alice@example.com".to_string()),
        plan_type: "plus".to_string(),
    };

    assert_eq!(
        json!({
            "identity": serde_json::to_value(identity).unwrap(),
            "codexRuntimeAuth": serde_json::to_value(runtime_auth).unwrap(),
        }),
        json!({
            "identity": {
                "identityId": "identity-local-1",
                "displayName": "Alice",
                "deviceId": "device-local-1",
                "brainMembership": {
                    "brainId": "brain-1",
                    "memberId": "member-1",
                    "role": "member",
                },
            },
            "codexRuntimeAuth": {
                "state": "signedIn",
                "email": "alice@example.com",
                "planType": "plus",
            },
        })
    );
}
