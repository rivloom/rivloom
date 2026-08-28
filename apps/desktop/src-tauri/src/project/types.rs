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

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
