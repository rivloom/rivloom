use pretty_assertions::assert_eq;
use serde_json::json;

use super::LocalProject;
use super::ProjectAvailability;

#[test]
fn local_projects_expose_only_the_frontend_contract() {
    let projects = [
        LocalProject {
            id: "project-1".to_string(),
            path: r"C:\work\rivloom".to_string(),
            name: "rivloom".to_string(),
            last_opened_at: 1_777_000_000,
            availability: ProjectAvailability::Available,
        },
        LocalProject {
            id: "project-2".to_string(),
            path: "/workspace/missing".to_string(),
            name: "missing".to_string(),
            last_opened_at: 1_776_000_000,
            availability: ProjectAvailability::Missing,
        },
        LocalProject {
            id: "project-3".to_string(),
            path: "/workspace/unreadable".to_string(),
            name: "unreadable".to_string(),
            last_opened_at: 1_775_000_000,
            availability: ProjectAvailability::Unreadable,
        },
    ];

    let actual = projects
        .iter()
        .map(|project| serde_json::to_value(project).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        vec![
            json!({
                "id": "project-1",
                "path": r"C:\work\rivloom",
                "name": "rivloom",
                "lastOpenedAt": 1_777_000_000,
                "availability": "available",
            }),
            json!({
                "id": "project-2",
                "path": "/workspace/missing",
                "name": "missing",
                "lastOpenedAt": 1_776_000_000,
                "availability": "missing",
            }),
            json!({
                "id": "project-3",
                "path": "/workspace/unreadable",
                "name": "unreadable",
                "lastOpenedAt": 1_775_000_000,
                "availability": "unreadable",
            }),
        ]
    );
}
