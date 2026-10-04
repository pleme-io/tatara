use clap::ValueEnum;
use kazari::{Capability, Fragment, Print, Role, Table};
use serde::Serialize;

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Yaml,
    Wide,
}

/// Render any serializable value in the requested format.
/// For table/wide, callers should use `render_table` instead.
pub fn render_value<T: Serialize>(value: &T, format: OutputFormat) -> anyhow::Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(value)?),
        OutputFormat::Yaml => Ok(serde_yaml::to_string(value)?),
        OutputFormat::Table | OutputFormat::Wide => {
            // Fallback to JSON for generic values in table mode
            Ok(serde_json::to_string_pretty(value)?)
        }
    }
}

/// Build a styled table with headers.
pub fn build_table(headers: &[&str]) -> Table {
    Table::new(headers.iter().copied())
}

pub fn cell(text: impl std::fmt::Display) -> Fragment {
    Fragment::styled(text.to_string(), Role::Text)
}

/// Color a status string based on its value.
pub fn status_cell(status: &str) -> Fragment {
    let role = match status {
        "running" | "ready" | "active" => Role::Ok,
        "pending" | "draining" => Role::Pending,
        "dead" | "failed" | "lost" | "down" => Role::Error,
        "complete" | "completed" | "superseded" => Role::Info,
        _ => Role::Text,
    };
    Fragment::styled(status, role)
}

/// Render a list of items as a table string.
pub fn render_table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let table = rows.into_iter().fold(build_table(headers), |t, row| {
        t.row_fragments(row.into_iter().map(cell))
    });
    table
        .to_string_at(Capability::probe())
        .trim_end_matches('\n')
        .to_string()
}

/// Format a chrono timestamp as a human-readable relative duration.
pub fn human_duration_since(ts: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return ts.to_string();
    };
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);

    if diff.num_days() > 0 {
        format!("{}d ago", diff.num_days())
    } else if diff.num_hours() > 0 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_minutes() > 0 {
        format!("{}m ago", diff.num_minutes())
    } else {
        format!("{}s ago", diff.num_seconds().max(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_value_json() {
        let val = serde_json::json!({"name": "test", "count": 42});
        let result = render_value(&val, OutputFormat::Json).unwrap();
        assert!(result.contains("\"name\": \"test\""));
        assert!(result.contains("\"count\": 42"));
    }

    #[test]
    fn test_render_value_yaml() {
        let val = serde_json::json!({"name": "test"});
        let result = render_value(&val, OutputFormat::Yaml).unwrap();
        assert!(result.contains("name: test"));
    }

    #[test]
    fn test_build_table_has_headers() {
        let table = build_table(&["ID", "NAME", "STATUS"]);
        let output = table.to_string_at(Capability::plain());
        assert!(output.contains("ID"));
        assert!(output.contains("NAME"));
        assert!(output.contains("STATUS"));
    }

    #[test]
    fn test_render_table_with_rows() {
        let rows = vec![
            vec!["1".to_string(), "job1".to_string(), "running".to_string()],
            vec!["2".to_string(), "job2".to_string(), "pending".to_string()],
        ];
        let output = render_table(&["ID", "NAME", "STATUS"], rows);
        assert!(output.contains("job1"));
        assert!(output.contains("job2"));
        assert!(output.contains("running"));
    }

    #[test]
    fn test_plain_table_has_no_escapes() {
        let table =
            build_table(&["ID", "STATUS"]).row_fragments([cell("1"), status_cell("failed")]);
        let output = table.to_string_at(Capability::plain());
        assert!(!output.contains('\u{1b}'));
        assert!(output.contains("failed"));
    }

    #[test]
    fn test_status_cell_roles() {
        assert_eq!(status_cell("running").role, Some(Role::Ok));
        assert_eq!(status_cell("pending").role, Some(Role::Pending));
        assert_eq!(status_cell("lost").role, Some(Role::Error));
        assert_eq!(status_cell("completed").role, Some(Role::Info));
        assert_eq!(status_cell("other").role, Some(Role::Text));
    }

    #[test]
    fn test_human_duration_invalid_timestamp() {
        let result = human_duration_since("not-a-timestamp");
        assert_eq!(result, "not-a-timestamp");
    }

    #[test]
    fn test_human_duration_recent() {
        let now = chrono::Utc::now();
        let ts = now.to_rfc3339();
        let result = human_duration_since(&ts);
        assert!(result.ends_with("s ago"));
    }
}
