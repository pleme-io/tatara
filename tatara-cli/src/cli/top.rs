use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, Table},
    Terminal,
};
use std::io;
use std::time::Duration;

use super::context::{active_endpoint, endpoint_to_server};

pub async fn run(
    node_filter: Option<&str>,
    refresh_secs: u64,
    endpoint: Option<&str>,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &client, &server, node_filter, refresh_secs).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: &reqwest::Client,
    server: &str,
    node_filter: Option<&str>,
    refresh_secs: u64,
) -> Result<()> {
    loop {
        // Fetch data
        let nodes = fetch_nodes(client, server).await.unwrap_or_default();
        let jobs = fetch_jobs(client, server).await.unwrap_or_default();
        let allocs = fetch_allocs(client, server).await.unwrap_or_default();

        // Filter nodes if specified
        let filtered_nodes: Vec<&serde_json::Value> = if let Some(filter) = node_filter {
            nodes
                .iter()
                .filter(|n| {
                    n["hostname"].as_str().unwrap_or("").contains(filter)
                        || n["node_id"].to_string().contains(filter)
                })
                .collect()
        } else {
            nodes.iter().collect()
        };

        terminal.draw(|frame| {
            let chunks = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Min(5),
            ])
            .split(frame.area());

            // Title
            let title = Block::default()
                .title(format!(
                    " tatara top — {} nodes, {} jobs, {} allocs ",
                    filtered_nodes.len(),
                    jobs.len(),
                    allocs.len()
                ))
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan));
            frame.render_widget(title, chunks[0]);

            // Node table
            let node_header = Row::new(vec!["ID", "HOSTNAME", "CPU (MHz)", "MEM (MB)", "ALLOCS", "STATUS"])
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

            let node_rows: Vec<Row> = filtered_nodes
                .iter()
                .map(|n| {
                    let status = n.get("status").and_then(|s| s.as_str()).unwrap_or("ready");
                    let status_style = match status {
                        "ready" => Style::default().fg(Color::Green),
                        "draining" => Style::default().fg(Color::Yellow),
                        _ => Style::default().fg(Color::Red),
                    };

                    Row::new(vec![
                        Cell::from(
                            n["node_id"]
                                .as_u64()
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "?".to_string()),
                        ),
                        Cell::from(n["hostname"].as_str().unwrap_or("?").to_string()),
                        Cell::from(
                            n["total_resources"]["cpu_mhz"]
                                .as_u64()
                                .unwrap_or(0)
                                .to_string(),
                        ),
                        Cell::from(
                            n["total_resources"]["memory_mb"]
                                .as_u64()
                                .unwrap_or(0)
                                .to_string(),
                        ),
                        Cell::from(
                            n["allocations_running"]
                                .as_u64()
                                .unwrap_or(0)
                                .to_string(),
                        ),
                        Cell::from(Span::styled(status.to_string(), status_style)),
                    ])
                })
                .collect();

            let node_table = Table::new(
                node_rows,
                [
                    Constraint::Length(12),
                    Constraint::Length(20),
                    Constraint::Length(12),
                    Constraint::Length(12),
                    Constraint::Length(8),
                    Constraint::Length(10),
                ],
            )
            .header(node_header)
            .block(Block::default().title(" Nodes ").borders(Borders::ALL));

            frame.render_widget(node_table, chunks[1]);

            // Job summary
            let job_header = Row::new(vec!["ID", "TYPE", "STATUS", "GROUPS", "VERSION"])
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

            let job_rows: Vec<Row> = jobs
                .iter()
                .take(20)
                .map(|j| {
                    let status = j["status"].as_str().unwrap_or("?");
                    let status_style = match status {
                        "running" => Style::default().fg(Color::Green),
                        "pending" => Style::default().fg(Color::Yellow),
                        _ => Style::default().fg(Color::Red),
                    };

                    Row::new(vec![
                        Cell::from(j["id"].as_str().unwrap_or("?").to_string()),
                        Cell::from(j["job_type"].as_str().unwrap_or("?").to_string()),
                        Cell::from(Span::styled(status.to_string(), status_style)),
                        Cell::from(
                            j["groups"]
                                .as_array()
                                .map(|g| g.len().to_string())
                                .unwrap_or_else(|| "0".to_string()),
                        ),
                        Cell::from(j["version"].as_u64().unwrap_or(0).to_string()),
                    ])
                })
                .collect();

            let job_label = format!(" Jobs ({}) ", jobs.len());
            let job_table = Table::new(
                job_rows,
                [
                    Constraint::Length(20),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(8),
                    Constraint::Length(10),
                ],
            )
            .header(job_header)
            .block(Block::default().title(job_label).borders(Borders::ALL));

            frame.render_widget(job_table, chunks[3]);

            // Help bar
            let help = Block::default()
                .title(" q: quit | refresh every {}s ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(help, chunks[2]);
        })?;

        // Handle input with timeout for refresh
        if event::poll(Duration::from_secs(refresh_secs))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        _ => {}
                    }
                }
            }
        }
    }
}

async fn fetch_nodes(
    client: &reqwest::Client,
    server: &str,
) -> Result<Vec<serde_json::Value>> {
    let url = format!("http://{}/api/v1/nodes", server);
    let resp = client.get(&url).send().await?;
    Ok(resp.json().await?)
}

async fn fetch_jobs(
    client: &reqwest::Client,
    server: &str,
) -> Result<Vec<serde_json::Value>> {
    let url = format!("http://{}/api/v1/jobs", server);
    let resp = client.get(&url).send().await?;
    Ok(resp.json().await?)
}

async fn fetch_allocs(
    client: &reqwest::Client,
    server: &str,
) -> Result<Vec<serde_json::Value>> {
    let url = format!("http://{}/api/v1/allocations", server);
    let resp = client.get(&url).send().await?;
    Ok(resp.json().await?)
}
