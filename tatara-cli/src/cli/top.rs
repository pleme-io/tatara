//! `tatara top` — live cluster monitor (Nodes + Jobs + Allocations).
//!
//! Rendered through `egaku-term`: bordered_block_with for each section,
//! manually painted column-aligned rows for the tabular bodies. The poll
//! loop is unchanged — `event::poll(Duration::from_secs(refresh_secs))`
//! drives both the refresh cadence and the keyboard read.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::style::{Attribute, Color};
use egaku::Rect;
use egaku_term::crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    style::{Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
};
use egaku_term::{Terminal, draw, theme::Palette};
use std::time::Duration;

use super::context::{active_endpoint, endpoint_to_server};

const HEADER_FG: Color = Color::Rgb { r: 235, g: 203, b: 139 }; // yellow
const STATUS_GREEN: Color = Color::Rgb { r: 163, g: 190, b: 140 };
const STATUS_YELLOW: Color = Color::Rgb { r: 235, g: 203, b: 139 };
const STATUS_RED: Color = Color::Rgb { r: 191, g: 97, b: 106 };
const HELP_FG: Color = Color::Rgb { r: 76, g: 86, b: 106 };

/// Style triple — fg / optional bg / attribute.
#[derive(Clone, Copy)]
struct Style {
    fg: Color,
    bg: Option<Color>,
    attr: Attribute,
}

impl Style {
    const fn fg(c: Color) -> Self {
        Self { fg: c, bg: None, attr: Attribute::Reset }
    }
    const fn bold(self) -> Self {
        Self { attr: Attribute::Bold, ..self }
    }
}

pub async fn run(
    node_filter: Option<&str>,
    refresh_secs: u64,
    endpoint: Option<&str>,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    // egaku-term owns terminal lifecycle. Drop restores raw mode + alt
    // screen even on panic.
    let mut term = Terminal::enter()?;
    run_loop(&mut term, &client, &server, node_filter, refresh_secs).await
}

async fn run_loop(
    term: &mut Terminal,
    client: &reqwest::Client,
    server: &str,
    node_filter: Option<&str>,
    refresh_secs: u64,
) -> Result<()> {
    loop {
        let nodes = fetch_nodes(client, server).await.unwrap_or_default();
        let jobs = fetch_jobs(client, server).await.unwrap_or_default();
        let allocs = fetch_allocs(client, server).await.unwrap_or_default();

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

        term.clear()?;
        draw_frame(term, &filtered_nodes, &jobs, &allocs, refresh_secs)?;
        term.flush()?;

        if event::poll(Duration::from_secs(refresh_secs))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                {
                    return Ok(());
                }
            }
        }
    }
}

fn draw_frame(
    term: &mut Terminal,
    nodes: &[&serde_json::Value],
    jobs: &[serde_json::Value],
    allocs: &[serde_json::Value],
    refresh_secs: u64,
) -> Result<()> {
    let pal = palette();
    let (cols, rows) = term.size().map_err(map_err)?;
    if cols < 20 || rows < 10 {
        return Ok(());
    }
    let cols_f = f32::from(cols);
    let rows_f = f32::from(rows);

    // Layout: title(3) | nodes(min) | help(3) | jobs(min)
    let title_h = 3.0;
    let help_h = 3.0;
    let body_h = rows_f - title_h - help_h;
    let nodes_h = body_h * 0.5;
    let jobs_h = body_h - nodes_h;

    let title_rect = Rect::new(0.0, 0.0, cols_f, title_h);
    let nodes_rect = Rect::new(0.0, title_h, cols_f, nodes_h);
    let help_rect = Rect::new(0.0, title_h + nodes_h, cols_f, help_h);
    let jobs_rect = Rect::new(0.0, title_h + nodes_h + help_h, cols_f, jobs_h);

    // Title
    let title = format!(
        " tatara top — {} nodes, {} jobs, {} allocs ",
        nodes.len(),
        jobs.len(),
        allocs.len()
    );
    draw::bordered_block_with(term, title_rect, &title, true, &pal).map_err(map_err)?;

    // Nodes table
    draw::bordered_block_with(term, nodes_rect, " Nodes ", false, &pal).map_err(map_err)?;
    let nodes_inner = draw::block_inner(nodes_rect);
    draw_nodes_table(term, nodes_inner, nodes)?;

    // Help bar
    let help_text = format!(" q: quit | refresh every {refresh_secs}s ");
    draw::bordered_block_with(term, help_rect, &help_text, false, &pal).map_err(map_err)?;

    // Jobs table
    let jobs_label = format!(" Jobs ({}) ", jobs.len());
    draw::bordered_block_with(term, jobs_rect, &jobs_label, false, &pal).map_err(map_err)?;
    let jobs_inner = draw::block_inner(jobs_rect);
    draw_jobs_table(term, jobs_inner, jobs)?;
    Ok(())
}

fn draw_nodes_table(
    term: &mut Terminal,
    rect: Rect,
    nodes: &[&serde_json::Value],
) -> Result<()> {
    let widths = [12u16, 20, 12, 12, 8, 10];
    let header = ["ID", "HOSTNAME", "CPU (MHz)", "MEM (MB)", "ALLOCS", "STATUS"];

    let (ix, iy, iw, ih) = cells(rect);
    if iw == 0 || ih == 0 {
        return Ok(());
    }

    paint_row(term, ix, iy, iw, &widths, &header, Style::fg(HEADER_FG).bold(), None)?;

    for (i, n) in nodes.iter().enumerate().take(usize::from(ih).saturating_sub(1)) {
        let row = u16::try_from(i + 1).unwrap_or(u16::MAX);
        let status = n.get("status").and_then(|s| s.as_str()).unwrap_or("ready");
        let status_style = match status {
            "ready" => Style::fg(STATUS_GREEN),
            "draining" => Style::fg(STATUS_YELLOW),
            _ => Style::fg(STATUS_RED),
        };

        let cells_text = [
            n["node_id"]
                .as_u64()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "?".to_string()),
            n["hostname"].as_str().unwrap_or("?").to_string(),
            n["total_resources"]["cpu_mhz"].as_u64().unwrap_or(0).to_string(),
            n["total_resources"]["memory_mb"].as_u64().unwrap_or(0).to_string(),
            n["allocations_running"].as_u64().unwrap_or(0).to_string(),
            status.to_string(),
        ];
        let cells_ref: Vec<&str> = cells_text.iter().map(String::as_str).collect();
        paint_row(
            term,
            ix,
            iy + row,
            iw,
            &widths,
            &cells_ref,
            Style::fg(Color::Rgb { r: 216, g: 222, b: 233 }),
            Some((5, status_style)),
        )?;
    }
    Ok(())
}

fn draw_jobs_table(
    term: &mut Terminal,
    rect: Rect,
    jobs: &[serde_json::Value],
) -> Result<()> {
    let widths = [20u16, 10, 10, 8, 10];
    let header = ["ID", "TYPE", "STATUS", "GROUPS", "VERSION"];

    let (ix, iy, iw, ih) = cells(rect);
    if iw == 0 || ih == 0 {
        return Ok(());
    }

    paint_row(term, ix, iy, iw, &widths, &header, Style::fg(HEADER_FG).bold(), None)?;

    for (i, j) in jobs.iter().enumerate().take(usize::from(ih).saturating_sub(1).min(20)) {
        let row = u16::try_from(i + 1).unwrap_or(u16::MAX);
        let status = j["status"].as_str().unwrap_or("?");
        let status_style = match status {
            "running" => Style::fg(STATUS_GREEN),
            "pending" => Style::fg(STATUS_YELLOW),
            _ => Style::fg(STATUS_RED),
        };

        let cells_text = [
            j["id"].as_str().unwrap_or("?").to_string(),
            j["job_type"].as_str().unwrap_or("?").to_string(),
            status.to_string(),
            j["groups"]
                .as_array()
                .map_or_else(|| "0".to_string(), |g| g.len().to_string()),
            j["version"].as_u64().unwrap_or(0).to_string(),
        ];
        let cells_ref: Vec<&str> = cells_text.iter().map(String::as_str).collect();
        paint_row(
            term,
            ix,
            iy + row,
            iw,
            &widths,
            &cells_ref,
            Style::fg(Color::Rgb { r: 216, g: 222, b: 233 }),
            Some((2, status_style)),
        )?;
    }
    Ok(())
}

/// Paint a row of column-aligned cells. Each cell is left-aligned and
/// padded/truncated to its declared width. `accent` overrides the default
/// style for one specific column index (used for status colors).
fn paint_row(
    term: &mut Terminal,
    col: u16,
    row: u16,
    max_w: u16,
    widths: &[u16],
    cells_text: &[&str],
    default: Style,
    accent: Option<(usize, Style)>,
) -> Result<()> {
    let mut x = col;
    for (i, (text, &w)) in cells_text.iter().zip(widths.iter()).enumerate() {
        if x.saturating_sub(col) >= max_w {
            break;
        }
        let cell_w = w.min(max_w.saturating_sub(x.saturating_sub(col)));
        let style = match accent {
            Some((idx, s)) if idx == i => s,
            _ => default,
        };
        let chars: String = text.chars().take(usize::from(cell_w)).collect();
        let padded = format!("{chars:<width$}", width = usize::from(cell_w));
        paint_styled(term, x, row, cell_w, &padded, style)?;
        x += cell_w + 1;
    }
    Ok(())
}

fn paint_styled(
    term: &mut Terminal,
    col: u16,
    row: u16,
    max: u16,
    text: &str,
    style: Style,
) -> Result<()> {
    if max == 0 {
        return Ok(());
    }
    term.out()
        .queue(MoveTo(col, row))?
        .queue(SetForegroundColor(style.fg))?;
    if let Some(bg) = style.bg {
        term.out().queue(SetBackgroundColor(bg))?;
    }
    if !matches!(style.attr, Attribute::Reset) {
        term.out().queue(SetAttribute(style.attr))?;
    }
    term.out().queue(Print(text))?;
    if !matches!(style.attr, Attribute::Reset) {
        term.out().queue(SetAttribute(Attribute::Reset))?;
    }
    term.out().queue(ResetColor)?;
    Ok(())
}

fn palette() -> Palette {
    Palette {
        background: Color::Rgb { r: 46, g: 52, b: 64 },
        foreground: Color::Rgb { r: 216, g: 222, b: 233 },
        accent: Color::Rgb { r: 136, g: 192, b: 208 },
        error: STATUS_RED,
        warning: STATUS_YELLOW,
        success: STATUS_GREEN,
        selection: Color::Rgb { r: 67, g: 76, b: 94 },
        muted: HELP_FG,
        border: HELP_FG,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn cells(rect: Rect) -> (u16, u16, u16, u16) {
    let to_u16 = |f: f32| f.max(0.0).round().min(f32::from(u16::MAX)) as u16;
    (
        to_u16(rect.x),
        to_u16(rect.y),
        to_u16(rect.width),
        to_u16(rect.height),
    )
}

fn map_err(e: egaku_term::Error) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

async fn fetch_nodes(client: &reqwest::Client, server: &str) -> Result<Vec<serde_json::Value>> {
    let url = format!("http://{server}/api/v1/nodes");
    let resp = client.get(&url).send().await?;
    Ok(resp.json().await?)
}

async fn fetch_jobs(client: &reqwest::Client, server: &str) -> Result<Vec<serde_json::Value>> {
    let url = format!("http://{server}/api/v1/jobs");
    let resp = client.get(&url).send().await?;
    Ok(resp.json().await?)
}

async fn fetch_allocs(client: &reqwest::Client, server: &str) -> Result<Vec<serde_json::Value>> {
    let url = format!("http://{server}/api/v1/allocations");
    let resp = client.get(&url).send().await?;
    Ok(resp.json().await?)
}
