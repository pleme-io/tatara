use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AllocationState {
    Pending,
    Running,
    Complete,
    Failed,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunState {
    Pending,
    Running,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allocation {
    pub id: Uuid,
    pub job_id: String,
    pub group_name: String,
    pub node_id: String,
    pub state: AllocationState,
    pub created_at: DateTime<Utc>,
    pub task_states: HashMap<String, TaskState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub state: TaskRunState,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub restarts: u32,
}

impl Allocation {
    pub fn new(job_id: String, group_name: String, node_id: String, task_names: Vec<String>) -> Self {
        let task_states = task_names
            .into_iter()
            .map(|name| (name, TaskState::new()))
            .collect();

        Self {
            id: Uuid::new_v4(),
            job_id,
            group_name,
            node_id,
            state: AllocationState::Pending,
            created_at: Utc::now(),
            task_states,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            AllocationState::Complete | AllocationState::Failed | AllocationState::Lost
        )
    }
}

impl TaskState {
    pub fn new() -> Self {
        Self {
            state: TaskRunState::Pending,
            pid: None,
            exit_code: None,
            started_at: None,
            finished_at: None,
            restarts: 0,
        }
    }
}
