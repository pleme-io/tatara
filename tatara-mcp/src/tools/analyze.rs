//! Analyze tools — compute insights from convergence state (read-only).

use crate::{McpToolDef, ToolCategory};
use serde_json::json;

pub fn register(tools: &mut Vec<McpToolDef>) {
    tools.push(McpToolDef {
        name: "diagnose_oscillation".into(),
        description: "Explain why a convergence point is oscillating and recommend damping adjustments".into(),
        category: ToolCategory::Analyze,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string" }
        }, "required": ["point_id"] }),
    });
    tools.push(McpToolDef {
        name: "identify_bottleneck".into(),
        description: "Find the slowest convergence path across all substrates (critical path analysis)".into(),
        category: ToolCategory::Analyze,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "compliance_gap_analysis".into(),
        description: "Identify unbound compliance controls or schema gaps in emission catalogs".into(),
        category: ToolCategory::Analyze,
        input_schema: json!({ "type": "object", "properties": {
            "framework": { "type": "string", "description": "Filter by compliance framework" }
        }}),
    });
    tools.push(McpToolDef {
        name: "cost_opportunity".into(),
        description: "Find cheaper substrate alternatives for running workloads".into(),
        category: ToolCategory::Analyze,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "blast_radius".into(),
        description: "Show impact of a convergence point failing or re-converging (reverse closure + affected substrates)".into(),
        category: ToolCategory::Analyze,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string" }
        }, "required": ["point_id"] }),
    });
    tools.push(McpToolDef {
        name: "convergence_anomaly".into(),
        description: "Detect unusual convergence patterns or rates across the cluster".into(),
        category: ToolCategory::Analyze,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
}
