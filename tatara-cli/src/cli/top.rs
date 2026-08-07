//! `tatara top` — live cluster monitor (Nodes + Jobs + Allocations).
//!
//! Rendered through `egaku-term`: bordered_block_with for each section,
//! column-aligned rows painted into the frame buffer for the tabular bodies.
//! The poll loop is unchanged — `event::poll(Duration::from_secs(refresh_secs))`
//! drives both the refresh cadence and the keyboard read.
//!
//! ## Double-buffered, not written straight to the terminal
//!
//! egaku-term's draw API is `Buffer`-based and infallible: all of `draw::*`
//! takes `&mut Buffer`, none takes `&mut Terminal`, and `Terminal` exposes no
//! buffer of its own. So a frame is composed into a back buffer and shipped by
//! [`render_diff`], which emits only the cells that actually changed — the same
//! shape egaku-term's own `app.rs` run loop uses. Cursor moves and colour
//! escapes are the renderer's business now, not this module's.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::style::Color;
use egaku::Rect;
use egaku_term::{Buffer, Style, Terminal, draw, render::render_diff, theme::Palette};
use std::time::Duration;

use super::context::{active_endpoint, endpoint_to_server};

const HEADER_FG: Color = Color::Rgb { r: 235, g: 203, b: 139 }; // yellow
const STATUS_GREEN: Color = Color::Rgb { r: 163, g: 190, b: 140 };
const STATUS_YELLOW: Color = Color::Rgb { r: 235, g: 203, b: 139 };
const STATUS_RED: Color = Color::Rgb { r: 191, g: 97, b: 106 };
const HELP_FG: Color = Color::Rgb { r: 76, g: 86, b: 106 };

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
    // Double buffer. `prev` is what the terminal currently shows, `back` is the
    // frame being composed; render_diff emits only the delta between them, so a
    // refresh that changes one status cell writes one cell. The old code did a
    // full clear + full repaint every tick, which is what made the whole screen
    // flicker at the refresh cadence.
    let (mut cols, mut rows) = term.size().map_err(map_err)?;
    let mut prev = Buffer::empty(cols, rows);
    let mut back = Buffer::empty(cols, rows);
    term.clear()?;
    term.flush()?;

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

        // A resize invalidates both buffers: prev must describe a screen of the
        // same shape or the diff is meaningless, so re-allocate and force a full
        // repaint by clearing the terminal too.
        let (w, h) = term.size().map_err(map_err)?;
        if (w, h) != (cols, rows) {
            cols = w;
            rows = h;
            prev = Buffer::empty(cols, rows);
            back = Buffer::empty(cols, rows);
            term.clear()?;
        }

        back.reset();
        draw_frame(&mut back, cols, rows, &filtered_nodes, &jobs, &allocs, refresh_secs);
        // sync_output() read BEFORE term.out() — the latter borrows mutably,
        // so reading it inline is an E0502.
        let sync = term.sync_output();
        render_diff(term.out(), &prev, &back, sync)?;
        term.flush()?;
        std::mem::swap(&mut prev, &mut back);

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

/// Compose one frame into `buf`.
///
/// Takes `cols`/`rows` explicitly rather than asking the terminal: a `Buffer`
/// carries its own dimensions, and the caller already had to read the size to
/// detect a resize. Passing them keeps this function pure — it touches no I/O,
/// which is why it no longer returns a `Result`.
fn draw_frame(
    buf: &mut Buffer,
    cols: u16,
    rows: u16,
    nodes: &[&serde_json::Value],
    jobs: &[serde_json::Value],
    allocs: &[serde_json::Value],
    refresh_secs: u64,
) {
    let pal = palette();
    if cols < 20 || rows < 10 {
        return;
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
    draw::bordered_block_with(buf, title_rect, &title, true, &pal);

    // Nodes table
    draw::bordered_block_with(buf, nodes_rect, " Nodes ", false, &pal);
    let nodes_inner = draw::block_inner(nodes_rect);
    draw_nodes_table(buf, nodes_inner, nodes);

    // Help bar
    let help_text = format!(" q: quit | refresh every {refresh_secs}s ");
    draw::bordered_block_with(buf, help_rect, &help_text, false, &pal);

    // Jobs table
    let jobs_label = format!(" Jobs ({}) ", jobs.len());
    draw::bordered_block_with(buf, jobs_rect, &jobs_label, false, &pal);
    let jobs_inner = draw::block_inner(jobs_rect);
    draw_jobs_table(buf, jobs_inner, jobs);
}

fn draw_nodes_table(buf: &mut Buffer, rect: Rect, nodes: &[&serde_json::Value]) {
    let widths = [12u16, 20, 12, 12, 8, 10];
    let header = ["ID", "HOSTNAME", "CPU (MHz)", "MEM (MB)", "ALLOCS", "STATUS"];

    let (ix, iy, iw, ih) = cells(rect);
    if iw == 0 || ih == 0 {
        return;
    }

    paint_row(buf, ix, iy, iw, &widths, &header, Style::DEFAULT.fg(HEADER_FG).bold(), None);

    for (i, n) in nodes.iter().enumerate().take(usize::from(ih).saturating_sub(1)) {
        let row = u16::try_from(i + 1).unwrap_or(u16::MAX);
        let status = n.get("status").and_then(|s| s.as_str()).unwrap_or("ready");
        let status_style = match status {
            "ready" => Style::DEFAULT.fg(STATUS_GREEN),
            "draining" => Style::DEFAULT.fg(STATUS_YELLOW),
            _ => Style::DEFAULT.fg(STATUS_RED),
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
            buf,
            ix,
            iy + row,
            iw,
            &widths,
            &cells_ref,
            Style::DEFAULT.fg(Color::Rgb { r: 216, g: 222, b: 233 }),
            Some((5, status_style)),
        );
    }
}

fn draw_jobs_table(buf: &mut Buffer, rect: Rect, jobs: &[serde_json::Value]) {
    let widths = [20u16, 10, 10, 8, 10];
    let header = ["ID", "TYPE", "STATUS", "GROUPS", "VERSION"];

    let (ix, iy, iw, ih) = cells(rect);
    if iw == 0 || ih == 0 {
        return;
    }

    paint_row(buf, ix, iy, iw, &widths, &header, Style::DEFAULT.fg(HEADER_FG).bold(), None);

    for (i, j) in jobs.iter().enumerate().take(usize::from(ih).saturating_sub(1).min(20)) {
        let row = u16::try_from(i + 1).unwrap_or(u16::MAX);
        let status = j["status"].as_str().unwrap_or("?");
        let status_style = match status {
            "running" => Style::DEFAULT.fg(STATUS_GREEN),
            "pending" => Style::DEFAULT.fg(STATUS_YELLOW),
            _ => Style::DEFAULT.fg(STATUS_RED),
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
            buf,
            ix,
            iy + row,
            iw,
            &widths,
            &cells_ref,
            Style::DEFAULT.fg(Color::Rgb { r: 216, g: 222, b: 233 }),
            Some((2, status_style)),
        );
    }
}

/// Paint a row of column-aligned cells. Each cell is left-aligned and
/// padded/truncated to its declared width. `accent` overrides the default
/// style for one specific column index (used for status colors).
fn paint_row(
    buf: &mut Buffer,
    col: u16,
    row: u16,
    max_w: u16,
    widths: &[u16],
    cells_text: &[&str],
    default: Style,
    accent: Option<(usize, Style)>,
) {
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
        buf.set_stringn(x, row, &padded, cell_w, style);
        x += cell_w + 1;
    }
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
