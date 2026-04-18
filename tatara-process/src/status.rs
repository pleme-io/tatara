//! `ProcessStatus` sub-structures — conditions, checked boundaries, Flux refs.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::boundary::Condition;

/// Standard K8s Condition (shape of `metav1.Condition`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    pub last_transition_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ProcessCondition {
    pub fn ready(reason: impl Into<String>, message: Option<String>) -> Self {
        Self {
            type_: "Ready".into(),
            status: "True".into(),
            last_transition_time: Utc::now(),
            reason: Some(reason.into()),
            message,
        }
    }

    pub fn not_ready(reason: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            type_: "Ready".into(),
            status: "False".into(),
            last_transition_time: Utc::now(),
            reason: Some(reason.into()),
            message: Some(message.into()),
        }
    }

    pub fn attested(root: &str) -> Self {
        Self {
            type_: "Attested".into(),
            status: "True".into(),
            last_transition_time: Utc::now(),
            reason: Some("AttestationWritten".into()),
            message: Some(format!("composed_root={root}")),
        }
    }
}

/// Reference to a FluxCD resource emitted as part of this Process.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FluxResourceRef {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub namespace: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DateTime<Utc>>,
}

/// A boundary condition paired with its current satisfaction state.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckedCondition {
    #[serde(flatten)]
    pub condition: Condition,
    pub satisfied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Summary of boundary verification.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryStatus {
    #[serde(default)]
    pub preconditions: Vec<CheckedCondition>,
    #[serde(default)]
    pub postconditions: Vec<CheckedCondition>,
    /// Absolute deadline for VERIFY (derived from `spec.boundary.timeout`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
}

/// Summary of compliance checks at the latest attestation.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    pub satisfied: u32,
    pub violated: u32,
    pub total: u32,
    #[serde(default)]
    pub violations: Vec<String>,
}
