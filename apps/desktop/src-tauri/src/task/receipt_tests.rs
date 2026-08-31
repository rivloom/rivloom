use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use sha2::Digest;
use sha2::Sha256;

use super::*;
use crate::task::artifact::MAX_PATCH_BYTES;
use crate::task::artifact::PatchArtifact;
use crate::task::artifact::PatchArtifactMetadata;
use crate::task::artifact::PatchArtifactState;

#[test]
fn success_receipt_is_a_complete_verifiable_object() {
    let input = receipt_input(
        RunReceiptOutcome::Success,
        Some("Implemented the requested change"),
        None,
        TestReport::Reported {
            executions: vec![
                TestExecution {
                    name: "cargo test task::receipt".to_string(),
                    exit_code: 0,
                },
                TestExecution {
                    name: "cargo clippy".to_string(),
                    exit_code: 0,
                },
            ],
        },
        complete_patch(),
    );

    let receipt = RunReceipt::new(input).unwrap();

    assert_eq!(
        receipt,
        RunReceipt {
            schema_version: RUN_RECEIPT_SCHEMA_VERSION,
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            node_id: "node-1".to_string(),
            runtime_id: "codex".to_string(),
            runtime_version: "codex-app-server 1.2.3".to_string(),
            started_at: 1_788_000_000,
            finished_at: 1_788_000_090,
            outcome: RunReceiptOutcome::Success,
            summary: Some("Implemented the requested change".to_string()),
            error: None,
            tests: TestReport::Reported {
                executions: vec![
                    TestExecution {
                        name: "cargo test task::receipt".to_string(),
                        exit_code: 0,
                    },
                    TestExecution {
                        name: "cargo clippy".to_string(),
                        exit_code: 0,
                    },
                ],
            },
            patch: complete_patch_metadata(),
            content_sha256: "2015f14f062f90b1687f41e779c47f36f7a9f1b800f38b652145773bc5893d5e"
                .to_string(),
        }
    );
    assert_eq!(receipt.verify(), Ok(()));
}

#[test]
fn failed_receipt_keeps_failure_and_test_exit_result() {
    let input = receipt_input(
        RunReceiptOutcome::Failed,
        None,
        Some("Tests failed"),
        TestReport::Reported {
            executions: vec![TestExecution {
                name: "cargo test".to_string(),
                exit_code: 101,
            }],
        },
        empty_patch(),
    );

    let receipt = RunReceipt::new(input).unwrap();

    assert_eq!(
        receipt,
        RunReceipt {
            schema_version: RUN_RECEIPT_SCHEMA_VERSION,
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            node_id: "node-1".to_string(),
            runtime_id: "codex".to_string(),
            runtime_version: "codex-app-server 1.2.3".to_string(),
            started_at: 1_788_000_000,
            finished_at: 1_788_000_090,
            outcome: RunReceiptOutcome::Failed,
            summary: None,
            error: Some("Tests failed".to_string()),
            tests: TestReport::Reported {
                executions: vec![TestExecution {
                    name: "cargo test".to_string(),
                    exit_code: 101,
                }],
            },
            patch: empty_patch_metadata(),
            content_sha256: "1173fe8987780e915315f1d0a86e5d396ea773c2096515dd8d52f7c78600d2b7"
                .to_string(),
        }
    );
    assert_eq!(receipt.verify(), Ok(()));
}

#[test]
fn cancelled_receipt_never_invents_a_test_report() {
    let input = receipt_input(
        RunReceiptOutcome::Cancelled,
        Some("Stopped by the local user"),
        None,
        TestReport::NotReported,
        empty_patch(),
    );

    let receipt = RunReceipt::new(input).unwrap();

    assert_eq!(
        receipt,
        RunReceipt {
            schema_version: RUN_RECEIPT_SCHEMA_VERSION,
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            node_id: "node-1".to_string(),
            runtime_id: "codex".to_string(),
            runtime_version: "codex-app-server 1.2.3".to_string(),
            started_at: 1_788_000_000,
            finished_at: 1_788_000_090,
            outcome: RunReceiptOutcome::Cancelled,
            summary: Some("Stopped by the local user".to_string()),
            error: None,
            tests: TestReport::NotReported,
            patch: empty_patch_metadata(),
            content_sha256: "11a9828dc341f664ddd1f2fd00376197bafa440c5e0d4b72cd30e55378a204b0"
                .to_string(),
        }
    );
    assert_eq!(receipt.verify(), Ok(()));
}

#[test]
fn unknown_outcome_is_explicit_and_never_becomes_success() {
    let input = receipt_input(
        RunReceiptOutcome::OutcomeUnknown,
        None,
        Some("Runtime disconnected before a terminal event"),
        TestReport::NotReported,
        empty_patch(),
    );

    let receipt = RunReceipt::new(input).unwrap();

    assert_eq!(
        receipt,
        RunReceipt {
            schema_version: RUN_RECEIPT_SCHEMA_VERSION,
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            node_id: "node-1".to_string(),
            runtime_id: "codex".to_string(),
            runtime_version: "codex-app-server 1.2.3".to_string(),
            started_at: 1_788_000_000,
            finished_at: 1_788_000_090,
            outcome: RunReceiptOutcome::OutcomeUnknown,
            summary: None,
            error: Some("Runtime disconnected before a terminal event".to_string()),
            tests: TestReport::NotReported,
            patch: empty_patch_metadata(),
            content_sha256: "f44df6e405b0d7d3464b0bd4da720c310624a67a935d91d07dc87fff10e70632"
                .to_string(),
        }
    );
    assert_eq!(receipt.verify(), Ok(()));
    let serialized = serde_json::to_string(&receipt).unwrap();
    assert!(serialized.contains("\"schemaVersion\":1"));
    assert!(serialized.contains("\"tests\":{\"state\":\"notReported\"}"));
    assert!(!serialized.contains("diff --git"));
    assert!(!serialized.contains("runtime-token-must-not-leak"));
    assert!(!serialized.contains("C:\\\\Users\\\\alice\\\\project"));
    assert!(!serialized.contains("/home/alice/project"));
}

#[test]
fn wire_contract_exposes_no_runtime_secret_or_absolute_path_fields() {
    let receipt = RunReceipt::new(valid_input()).unwrap();
    let serialized = serde_json::to_value(receipt).unwrap();
    let fields = serialized
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        fields,
        BTreeSet::from([
            "contentSha256",
            "error",
            "finishedAt",
            "nodeId",
            "outcome",
            "patch",
            "runId",
            "runtimeId",
            "runtimeVersion",
            "schemaVersion",
            "startedAt",
            "summary",
            "taskId",
            "tests",
        ])
    );
    let patch_fields = serialized["patch"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        patch_fields,
        BTreeSet::from([
            "baselineCommit",
            "byteCount",
            "limitBytes",
            "sha256",
            "state",
        ])
    );
}

#[test]
fn ids_versions_timestamps_summaries_and_test_reports_are_bounded() {
    let mut input = valid_input();
    input.task_id = "x".repeat(MAX_RECEIPT_ID_BYTES + 1);
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidIdentity));

    let mut input = valid_input();
    input.runtime_version = "x".repeat(MAX_RUNTIME_VERSION_BYTES + 1);
    assert_eq!(
        RunReceipt::new(input),
        Err(ReceiptError::InvalidRuntimeVersion)
    );

    let mut input = valid_input();
    input.finished_at = input.started_at - 1;
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidTimestamp));

    let mut input = valid_input();
    input.started_at = -1;
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidTimestamp));

    let mut input = valid_input();
    input.summary = Some("x".repeat(MAX_RECEIPT_SUMMARY_BYTES + 1));
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidSummary));

    let mut input = valid_input();
    input.error = Some("x".repeat(MAX_RECEIPT_ERROR_BYTES + 1));
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidError));

    let mut input = valid_input();
    input.tests = TestReport::Reported {
        executions: (0..=MAX_TEST_EXECUTIONS)
            .map(|index| TestExecution {
                name: format!("test-{index}"),
                exit_code: 0,
            })
            .collect(),
    };
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidTests));

    let mut input = valid_input();
    input.tests = TestReport::Reported {
        executions: vec![TestExecution {
            name: "x".repeat(MAX_TEST_NAME_BYTES + 1),
            exit_code: 0,
        }],
    };
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidTests));

    let mut input = valid_input();
    input.tests = TestReport::Reported {
        executions: (0..17)
            .map(|_| TestExecution {
                name: "x".repeat(MAX_TEST_NAME_BYTES),
                exit_code: 0,
            })
            .collect(),
    };
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidTests));
}

#[test]
fn outcome_details_and_patch_metadata_must_be_internally_consistent() {
    let mut input = valid_input();
    input.outcome = RunReceiptOutcome::Success;
    input.error = Some("success cannot carry an error".to_string());
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidOutcome));

    let mut input = valid_input();
    input.outcome = RunReceiptOutcome::Failed;
    input.error = None;
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidOutcome));

    let mut input = valid_input();
    input.patch.sha256 = Some("0".repeat(64));
    assert_eq!(RunReceipt::new(input), Err(ReceiptError::InvalidPatch));
}

#[test]
fn content_hash_detects_mutation_and_is_stable_for_the_same_receipt() {
    let first = RunReceipt::new(valid_input()).unwrap();
    let second = RunReceipt::new(valid_input()).unwrap();
    assert_eq!(first, second);

    let mut changed = first.clone();
    changed.finished_at += 1;
    assert_eq!(changed.verify(), Err(ReceiptError::ContentHashMismatch));
}

#[test]
fn bounded_non_text_patch_states_remain_verifiable() {
    let patches = [
        PatchArtifact {
            baseline_commit: "a".repeat(40),
            state: PatchArtifactState::TooLarge,
            limit_bytes: MAX_PATCH_BYTES,
            byte_count: None,
            sha256: None,
            patch: None,
        },
        PatchArtifact {
            baseline_commit: "a".repeat(40),
            state: PatchArtifactState::UnsupportedEncoding,
            limit_bytes: MAX_PATCH_BYTES,
            byte_count: Some(12),
            sha256: Some("b".repeat(64)),
            patch: None,
        },
    ];

    for patch in patches {
        let mut input = valid_input();
        input.patch = patch;
        let receipt = RunReceipt::new(input).unwrap();
        assert_eq!(receipt.verify(), Ok(()));
    }
}

fn valid_input() -> RunReceiptInput {
    receipt_input(
        RunReceiptOutcome::Success,
        Some("Implemented the requested change"),
        None,
        TestReport::Reported {
            executions: vec![TestExecution {
                name: "cargo test".to_string(),
                exit_code: 0,
            }],
        },
        complete_patch(),
    )
}

fn receipt_input(
    outcome: RunReceiptOutcome,
    summary: Option<&str>,
    error: Option<&str>,
    tests: TestReport,
    patch: PatchArtifact,
) -> RunReceiptInput {
    RunReceiptInput {
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        node_id: "node-1".to_string(),
        runtime_id: "codex".to_string(),
        runtime_version: "codex-app-server 1.2.3".to_string(),
        started_at: 1_788_000_000,
        finished_at: 1_788_000_090,
        outcome,
        summary: summary.map(str::to_string),
        error: error.map(str::to_string),
        tests,
        patch,
    }
}

fn complete_patch() -> PatchArtifact {
    let patch = "diff --git a/file.txt b/file.txt\n".to_string();
    PatchArtifact {
        baseline_commit: "a".repeat(40),
        state: PatchArtifactState::Complete,
        limit_bytes: MAX_PATCH_BYTES,
        byte_count: Some(patch.len() as u64),
        sha256: Some(sha256(patch.as_bytes())),
        patch: Some(patch),
    }
}

fn empty_patch() -> PatchArtifact {
    PatchArtifact {
        baseline_commit: "a".repeat(40),
        state: PatchArtifactState::Empty,
        limit_bytes: MAX_PATCH_BYTES,
        byte_count: Some(0),
        sha256: Some(sha256([])),
        patch: Some(String::new()),
    }
}

fn complete_patch_metadata() -> PatchArtifactMetadata {
    complete_patch().metadata().unwrap()
}

fn empty_patch_metadata() -> PatchArtifactMetadata {
    empty_patch().metadata().unwrap()
}

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
