//! Verify tools — validate convergence correctness and compliance.

use crate::{McpToolDef, ToolCategory};
use serde_json::json;

pub fn register(tools: &mut Vec<McpToolDef>) {
    tools.push(McpToolDef {
        name: "verify_convergence_plan".into(),
        description: "Validate a convergence plan before execution — check graph well-typedness, dependency cycles, compliance coverage".into(),
        category: ToolCategory::Verify,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "verify_attestation_chain".into(),
        description: "Validate cryptographic attestation chain integrity for a convergence DAG".into(),
        category: ToolCategory::Verify,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string", "description": "Starting point for chain verification" }
        }}),
    });
    tools.push(McpToolDef {
        name: "verify_compliance_posture".into(),
        description: "Check that all compliance bindings are satisfied across the convergence graph".into(),
        category: ToolCategory::Verify,
        input_schema: json!({ "type": "object", "properties": {
            "framework": { "type": "string", "description": "Filter by compliance framework" }
        }}),
    });
    tools.push(McpToolDef {
        name: "verify_graph_well_typed".into(),
        description: "Validate that all typed edges connect compatible convergence point types".into(),
        category: ToolCategory::Verify,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "verify_security_posture".into(),
        description: "Review security substrate convergence for vulnerabilities or misconfigurations".into(),
        category: ToolCategory::Verify,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
}
