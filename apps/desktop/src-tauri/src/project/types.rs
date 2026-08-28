use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalProject {
    pub id: String,
    pub path: String,
    pub name: String,
    pub last_opened_at: i64,
    pub availability: ProjectAvailability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProjectAvailability {
    Available,
    Missing,
    Unreadable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectThread {
    pub id: String,
    pub name: Option<String>,
    pub preview: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub recency_at: Option<i64>,
    pub status: ProjectThreadStatus,
    pub cwd: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProjectThreadStatus {
    NotLoaded,
    Idle,
    SystemError,
    Active,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectThreadPage {
    pub data: Vec<ProjectThread>,
    pub next_cursor: Option<String>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
