use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub directory: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    #[serde(default)]
    pub directory: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "workspaceID"
    )]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectInfo>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::LocationInfo;

    #[test]
    fn deserializes_project_object_shape() {
        let location: LocationInfo = serde_json::from_value(serde_json::json!({
            "directory": "/tmp/project",
            "project": {
                "id": "project-1",
                "directory": "/tmp/project"
            }
        }))
        .expect("location should deserialize");

        let project = location.project.expect("project should be present");
        assert_eq!(project.id, "project-1");
        assert_eq!(project.directory, "/tmp/project");
    }

    #[test]
    fn deserializes_without_project() {
        let location: LocationInfo = serde_json::from_value(serde_json::json!({
            "directory": "/tmp/project"
        }))
        .expect("location should deserialize");

        assert_eq!(location.directory, "/tmp/project");
        assert!(location.project.is_none());
    }
}
