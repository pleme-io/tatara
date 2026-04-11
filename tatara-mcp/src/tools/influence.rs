//! Influence tools — take bounded actions within emission catalogs.

use crate::{McpToolDef, ToolCategory};
use serde_json::json;

pub fn register(tools: &mut Vec<McpToolDef>) {
    tools.push(McpToolDef {
        name: "emit_bounded_dag".into(),
        description: "Instantiate a bounded convergence DAG from an emission schema template".into(),
        category: ToolCategory::Influence,
        input_schema: json!({ "type": "object", "properties": {
            "template_name": { "type": "string", "description": "Name of the bounded DAG template" },
            "params": { "type": "object", "description": "Runtime parameters for the template" }
        }, "required": ["template_name"] }),
    });
    tools.push(McpToolDef {
        name: "adjust_substrate_priority".into(),
        description: "Change priority ordering between substrates (soft substrates only)".into(),
        category: ToolCategory::Influence,
        input_schema: json!({ "type": "object", "properties": {
            "priorities": { "type": "array", "items": { "type": "string" }, "description": "Ordered list of substrate types" }
        }, "required": ["priorities"] }),
    });
    tools.push(McpToolDef {
        name: "defer_convergence".into(),
        description: "Pause a convergence point temporarily (bounded duration, logged)".into(),
        category: ToolCategory::Influence,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string" },
            "duration_seconds": { "type": "integer" },
            "reason": { "type": "string" }
        }, "required": ["point_id", "reason"] }),
    });
    tools.push(McpToolDef {
        name: "resume_convergence".into(),
        description: "Unpause a deferred convergence point".into(),
        category: ToolCategory::Influence,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string" }
        }, "required": ["point_id"] }),
    });
    tools.push(McpToolDef {
        name: "escalate_schema_gap".into(),
        description: "Flag a pattern that needs a new bounded DAG template in the emission catalog".into(),
        category: ToolCategory::Influence,
        input_schema: json!({ "type": "object", "properties": {
            "description": { "type": "string", "description": "What pattern was encountered" },
            "suggested_template": { "type": "string", "description": "Proposed template name" }
        }, "required": ["description"] }),
    });
    tools.push(McpToolDef {
        name: "trigger_re_convergence".into(),
        description: "Force a convergence point to re-evaluate (idempotent)".into(),
        category: ToolCategory::Influence,
        input_schema: json!({ "type": "object", "properties": {
            "point_id": { "type": "string" }
        }, "required": ["point_id"] }),
    });
}
