use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

use super::artifact::PatchArtifact;
use super::artifact::PatchArtifactMetadata;

pub(crate) const RUN_RECEIPT_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_RECEIPT_ID_BYTES: usize = 128;
pub(crate) const MAX_RUNTIME_VERSION_BYTES: usize = 128;
pub(crate) const MAX_RECEIPT_SUMMARY_BYTES: usize = 4 * 1024;
pub(crate) const MAX_RECEIPT_ERROR_BYTES: usize = 2 * 1024;
pub(crate) const MAX_TEST_EXECUTIONS: usize = 32;
const MAX_TEST_NAME_BYTES: usize = 256;
const MAX_TEST_NAME_TOTAL_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RunReceiptOutcome {
    Success,
    Failed,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestExecution {
    pub(crate) name: String,
    pub(crate) exit_code: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum TestReport {
    NotReported,
    Reported { executions: Vec<TestExecution> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunReceiptInput {
    pub(crate) task_id: String,
    pub(crate) run_id: String,
    pub(crate) node_id: String,
    pub(crate) runtime_id: String,
    pub(crate) runtime_version: String,
    pub(crate) started_at: i64,
    pub(crate) finished_at: i64,
    pub(crate) outcome: RunReceiptOutcome,
    pub(crate) summary: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) tests: TestReport,
    pub(crate) patch: PatchArtifact,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunReceipt {
    pub(crate) schema_version: u32,
    pub(crate) task_id: String,
    pub(crate) run_id: String,
    pub(crate) node_id: String,
    pub(crate) runtime_id: String,
    pub(crate) runtime_version: String,
    pub(crate) started_at: i64,
    pub(crate) finished_at: i64,
    pub(crate) outcome: RunReceiptOutcome,
    pub(crate) summary: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) tests: TestReport,
    pub(crate) patch: PatchArtifactMetadata,
    pub(crate) content_sha256: String,
}

impl RunReceipt {
    pub(crate) fn new(input: RunReceiptInput) -> Result<Self, ReceiptError> {
        let patch = input.patch.metadata().ok_or(ReceiptError::InvalidPatch)?;
        let mut receipt = Self {
            schema_version: RUN_RECEIPT_SCHEMA_VERSION,
            task_id: input.task_id,
            run_id: input.run_id,
            node_id: input.node_id,
            runtime_id: input.runtime_id,
            runtime_version: input.runtime_version,
            started_at: input.started_at,
            finished_at: input.finished_at,
            outcome: input.outcome,
            summary: input.summary,
            error: input.error,
            tests: input.tests,
            patch,
            content_sha256: String::new(),
        };
        receipt.validate_fields()?;
        receipt.content_sha256 = receipt.calculate_content_sha256()?;
        Ok(receipt)
    }

    pub(crate) fn verify(&self) -> Result<(), ReceiptError> {
        self.validate_fields()?;
        if self.content_sha256 != self.calculate_content_sha256()? {
            return Err(ReceiptError::ContentHashMismatch);
        }
        Ok(())
    }

    fn validate_fields(&self) -> Result<(), ReceiptError> {
        if self.schema_version != RUN_RECEIPT_SCHEMA_VERSION
            || !valid_id(&self.task_id)
            || !valid_id(&self.run_id)
            || !valid_id(&self.node_id)
            || !valid_id(&self.runtime_id)
        {
            return Err(ReceiptError::InvalidIdentity);
        }
        if self.runtime_version.trim().is_empty()
            || self.runtime_version.len() > MAX_RUNTIME_VERSION_BYTES
        {
            return Err(ReceiptError::InvalidRuntimeVersion);
        }
        if self.started_at < 0 || self.finished_at < self.started_at {
            return Err(ReceiptError::InvalidTimestamp);
        }
        if !valid_optional_text(&self.summary, MAX_RECEIPT_SUMMARY_BYTES) {
            return Err(ReceiptError::InvalidSummary);
        }
        if !valid_optional_text(&self.error, MAX_RECEIPT_ERROR_BYTES) {
            return Err(ReceiptError::InvalidError);
        }
        let outcome_is_valid = match self.outcome {
            RunReceiptOutcome::Success | RunReceiptOutcome::Cancelled => self.error.is_none(),
            RunReceiptOutcome::Failed | RunReceiptOutcome::OutcomeUnknown => self.error.is_some(),
        };
        if !outcome_is_valid {
            return Err(ReceiptError::InvalidOutcome);
        }
        validate_tests(&self.tests)?;
        if !self.patch.is_valid() {
            return Err(ReceiptError::InvalidPatch);
        }
        Ok(())
    }

    fn calculate_content_sha256(&self) -> Result<String, ReceiptError> {
        let payload = ReceiptPayload {
            schema_version: self.schema_version,
            task_id: &self.task_id,
            run_id: &self.run_id,
            node_id: &self.node_id,
            runtime_id: &self.runtime_id,
            runtime_version: &self.runtime_version,
            started_at: self.started_at,
            finished_at: self.finished_at,
            outcome: self.outcome,
            summary: self.summary.as_deref(),
            error: self.error.as_deref(),
            tests: &self.tests,
            patch: &self.patch,
        };
        let bytes = serde_json::to_vec(&payload).map_err(|_| ReceiptError::Serialization)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptPayload<'a> {
    schema_version: u32,
    task_id: &'a str,
    run_id: &'a str,
    node_id: &'a str,
    runtime_id: &'a str,
    runtime_version: &'a str,
    started_at: i64,
    finished_at: i64,
    outcome: RunReceiptOutcome,
    summary: Option<&'a str>,
    error: Option<&'a str>,
    tests: &'a TestReport,
    patch: &'a PatchArtifactMetadata,
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_RECEIPT_ID_BYTES
}

fn valid_optional_text(value: &Option<String>, max_bytes: usize) -> bool {
    !value
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.len() > max_bytes)
}

fn validate_tests(tests: &TestReport) -> Result<(), ReceiptError> {
    let TestReport::Reported { executions } = tests else {
        return Ok(());
    };
    if executions.len() > MAX_TEST_EXECUTIONS {
        return Err(ReceiptError::InvalidTests);
    }
    let mut total_bytes = 0;
    for execution in executions {
        if execution.name.trim().is_empty() || execution.name.len() > MAX_TEST_NAME_BYTES {
            return Err(ReceiptError::InvalidTests);
        }
        total_bytes += execution.name.len();
    }
    if total_bytes > MAX_TEST_NAME_TOTAL_BYTES {
        return Err(ReceiptError::InvalidTests);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ReceiptError {
    #[error("RunReceipt identity is invalid")]
    InvalidIdentity,
    #[error("RunReceipt Runtime version is invalid")]
    InvalidRuntimeVersion,
    #[error("RunReceipt timestamp is invalid")]
    InvalidTimestamp,
    #[error("RunReceipt summary is invalid")]
    InvalidSummary,
    #[error("RunReceipt error detail is invalid")]
    InvalidError,
    #[error("RunReceipt outcome is inconsistent")]
    InvalidOutcome,
    #[error("RunReceipt test report is invalid")]
    InvalidTests,
    #[error("RunReceipt Patch metadata is invalid")]
    InvalidPatch,
    #[error("RunReceipt content hash does not match")]
    ContentHashMismatch,
    #[error("RunReceipt could not be serialized")]
    Serialization,
}

#[cfg(test)]
#[path = "receipt_tests.rs"]
mod tests;
