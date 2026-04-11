//! Tatara MCP server — AI-assisted convergence computing.
//!
//! Provides 30 MCP tools organized in 5 categories for AI agents to
//! observe, analyze, influence, verify, and report on convergence state.
//!
//! This is the Intelligence dimension (6th classification axis) of the
//! convergence computing model. AI interacts with the convergence ether
//! through structured, typed, purpose-built tools.

pub mod tools;

use serde::{Deserialize, Serialize};

/// MCP tool categories — every tool belongs to exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    /// Read convergence state (read-only).
    Observe,
    /// Compute insights from convergence state (read-only).
    Analyze,
    /// Take bounded actions within emission catalogs.
    Influence,
    /// Validate convergence correctness and compliance.
    Verify,
    /// Generate compliance and performance reports.
    Report,
}

/// An MCP tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    /// Tool name (used in MCP protocol).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Which category this tool belongs to.
    pub category: ToolCategory,
    /// JSON schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// Registry of all convergence MCP tools.
pub struct ToolRegistry {
    tools: Vec<McpToolDef>,
}

impl ToolRegistry {
    /// Create a registry with all 30 convergence tools.
    pub fn new() -> Self {
        let mut tools = Vec::new();

        // Register all tools by category
        tools::observe::register(&mut tools);
        tools::analyze::register(&mut tools);
        tools::influence::register(&mut tools);
        tools::verify::register(&mut tools);
        tools::report::register(&mut tools);

        Self { tools }
    }

    /// Get all registered tools.
    pub fn all_tools(&self) -> &[McpToolDef] {
        &self.tools
    }

    /// Get tools by category.
    pub fn by_category(&self, category: ToolCategory) -> Vec<&McpToolDef> {
        self.tools
            .iter()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&McpToolDef> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Total number of registered tools.
    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tools_registered() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.count(), 30);
    }

    #[test]
    fn test_observe_tools() {
        let registry = ToolRegistry::new();
        let observe = registry.by_category(ToolCategory::Observe);
        assert_eq!(observe.len(), 8);
    }

    #[test]
    fn test_analyze_tools() {
        let registry = ToolRegistry::new();
        let analyze = registry.by_category(ToolCategory::Analyze);
        assert_eq!(analyze.len(), 6);
    }

    #[test]
    fn test_influence_tools() {
        let registry = ToolRegistry::new();
        let influence = registry.by_category(ToolCategory::Influence);
        assert_eq!(influence.len(), 6);
    }

    #[test]
    fn test_verify_tools() {
        let registry = ToolRegistry::new();
        let verify = registry.by_category(ToolCategory::Verify);
        assert_eq!(verify.len(), 5);
    }

    #[test]
    fn test_report_tools() {
        let registry = ToolRegistry::new();
        let report = registry.by_category(ToolCategory::Report);
        assert_eq!(report.len(), 5);
    }

    #[test]
    fn test_lookup_by_name() {
        let registry = ToolRegistry::new();
        assert!(registry.get("convergence_graph").is_some());
        assert!(registry.get("diagnose_oscillation").is_some());
        assert!(registry.get("emit_bounded_dag").is_some());
        assert!(registry.get("verify_convergence_plan").is_some());
        assert!(registry.get("compliance_report").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_all_tools_have_schemas() {
        let registry = ToolRegistry::new();
        for tool in registry.all_tools() {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert!(tool.input_schema.is_object());
        }
    }
}
