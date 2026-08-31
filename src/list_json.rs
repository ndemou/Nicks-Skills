use serde::{Deserialize, Serialize};

pub const TASK_LIST_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskListDocument {
    pub schema_version: u32,
    pub repository: String,
    pub mode: String,
    pub tasks: Vec<TaskListItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskListItem {
    pub id: String,
    pub title: String,
    pub body_markdown: String,
    pub created_at: Option<String>,
    pub completed_at: Option<String>,
    pub completions: Vec<String>,
    pub priority: f64,
    pub priority_text: String,
    #[serde(rename = "type")]
    pub task_type: String,
    pub status: String,
    pub filename: String,
    pub relationships: TaskRelationships,
    pub blocking_causes: Vec<BlockingCause>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskRelationships {
    pub parent: Option<TaskReference>,
    pub children: Vec<TaskReference>,
    pub blocks: Vec<TaskReference>,
    pub blocked_by: Vec<TaskReference>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskReference {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority_text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockingCause {
    pub blocker: TaskReference,
    pub blocked_target: TaskReference,
    pub kind: String,
}
