//! Observe tools — read convergence state (read-only).

use crate::{McpToolDef, ToolCategory};
use serde_json::json;

pub fn register(tools: &mut Vec<McpToolDef>) {
    tools.push(McpToolDef {
        name: "convergence_graph".into(),
        description: "Get the full typed convergence DAG across all substrates".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "convergence_distance".into(),
        description: "Get per-substrate convergence distance vector".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {
            "substrate": { "type": "string", "description": "Filter by substrate type" }
        }}),
    });
    tools.push(McpToolDef {
        name: "convergence_rate".into(),
        description: "Get convergence rate per point and cluster-wide".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "convergence_plan".into(),
        description: "Get the current convergence plan (execution order, critical path, cache hits)".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "convergence_closure".into(),
        description: "Compute forward or reverse dependency closure for a convergence point".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string" },
            "direction": { "type": "string", "enum": ["forward", "reverse"] }
        }, "required": ["point_id"] }),
    });
    tools.push(McpToolDef {
        name: "compliance_closure".into(),
        description: "Get all compliance controls bound to a convergence DAG".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "attestation_history".into(),
        description: "Get the generational attestation chain for a convergence point".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string" }
        }, "required": ["point_id"] }),
    });
    tools.push(McpToolDef {
        name: "cluster_health".into(),
        description: "Get cluster-wide convergence health (bounded converged + asymptotic rates)".into(),
        category: ToolCategory::Observe,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
}
