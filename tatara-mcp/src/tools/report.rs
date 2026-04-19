//! Report tools — generate compliance and performance reports.

use crate::{McpToolDef, ToolCategory};
use serde_json::json;

pub fn register(tools: &mut Vec<McpToolDef>) {
    tools.push(McpToolDef {
        name: "compliance_report".into(),
        description: "Generate a framework-specific compliance report (NIST, SOC2, FedRAMP, PCI)".into(),
        category: ToolCategory::Report,
        input_schema: json!({ "type": "object", "properties": {
            "framework": { "type": "string", "description": "Compliance framework to report on" },
            "format": { "type": "string", "enum": ["oscal", "json", "text"], "description": "Output format" }
        }, "required": ["framework"] }),
    });
    tools.push(McpToolDef {
        name: "convergence_health_report".into(),
        description: "Generate an overall convergence health report across all substrates".into(),
        category: ToolCategory::Report,
        input_schema: json!({ "type": "object", "properties": {
            "format": { "type": "string", "enum": ["json", "text"] }
        }}),
    });
    tools.push(McpToolDef {
        name: "cost_optimization_report".into(),
        description: "Generate financial substrate analysis with cost reduction recommendations"
            .into(),
        category: ToolCategory::Report,
        input_schema: json!({ "type": "object", "properties": {} }),
    });
    tools.push(McpToolDef {
        name: "attestation_audit_trail".into(),
        description: "Trace the full attestation chain for a specific operation or time range"
            .into(),
        category: ToolCategory::Report,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string", "description": "Starting point for audit" },
            "since": { "type": "string", "description": "ISO 8601 start time" }
        }}),
    });
    tools.push(McpToolDef {
        name: "incident_analysis".into(),
        description: "Analyze a divergence event — root cause, affected points, recovery timeline"
            .into(),
        category: ToolCategory::Report,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string", "description": "The point that diverged" },
            "timestamp": { "type": "string", "description": "When the divergence occurred" }
        }, "required": ["point_id"] }),
    });
}
