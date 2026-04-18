//! tatara-boot-gen — read a tatara-lisp boot file, emit every artifact
//! needed to run a tatara-os guest on Darwin (or any host with vfkit).
//!
//! Usage:
//!   tatara-boot-gen <in.lisp> <out-dir> [--init-path PATH] [--no-busybox]
//!                   [--no-color] [--quiet] [--replay]
//!
//! The input file should contain at least one `(defsystem …)` form. Any
//! `(defvm …)` form present is used as the VM-shape override.
//!
//! UX is Nord-themed via `tatara-ui`: every artifact line shows its name,
//! a BLAKE3 short hash, and `⚡ cached` / `⚙ built …s` so cache hits tell
//! a visual story. The whole run produces a content-root hash you can use
//! with `--replay` to re-paint an identical past run.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tatara_lisp::{domain::TataraDomain, read, Sexp};
use tatara_nix::MultiSynthesizer;
use tatara_os::SystemConfig;
use tatara_ui::{
    ArtifactState, EventStream, LogLevel, Renderer, ShortHash, ThemeSpec, UiEvent,
};
use tatara_vm::{boot::BootSynthesizer, VmSpec};

struct Opts {
    in_path: PathBuf,
    out_dir: PathBuf,
    init_path: Option<String>,
    busybox: bool,
    color: bool,
    quiet: bool,
    replay: Option<PathBuf>,
}

fn parse_args() -> Result<Opts, String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err(
            "usage: tatara-boot-gen <in.lisp> <out-dir> [--init-path PATH] \
             [--no-busybox] [--no-color] [--quiet] [--replay FILE]"
                .into(),
        );
    }
    let mut o = Opts {
        in_path: PathBuf::from(&args[1]),
        out_dir: PathBuf::from(&args[2]),
        init_path: None,
        busybox: true,
        color: tatara_ui::should_color(),
        quiet: false,
        replay: None,
    };
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--init-path" => {
                o.init_path = Some(args.get(i + 1).cloned().ok_or("--init-path needs arg")?);
                i += 2;
            }
            "--no-busybox" => {
                o.busybox = false;
                i += 1;
            }
            "--no-color" => {
                o.color = false;
                i += 1;
            }
            "--quiet" => {
                o.quiet = true;
                i += 1;
            }
            "--replay" => {
                o.replay = Some(PathBuf::from(
                    args.get(i + 1).cloned().ok_or("--replay needs path")?,
                ));
                i += 2;
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    Ok(o)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = parse_args().map_err(|e| {
        eprintln!("{e}");
        e
    })?;

    let theme = ThemeSpec::nord_arctic();
    let role_map = theme.to_role_map();
    let renderer = Renderer::new(role_map).with_color(opts.color);
    let mut stream = EventStream::new();
    let mut stderr = std::io::stderr();

    // ── replay mode: load a past run hash and re-paint it ───────────
    if let Some(p) = &opts.replay {
        let json = std::fs::read_to_string(p)?;
        let past: EventStream = serde_json::from_str(&json)?;
        renderer.render(&past, &mut stderr)?;
        return Ok(());
    }

    // ── banner ──────────────────────────────────────────────────────
    if !opts.quiet {
        emit(&renderer, &mut stream, &mut stderr, UiEvent::Banner {
            title: "tatara-boot-gen".into(),
            subtitle: Some(format!("{}", opts.in_path.display())),
        })?;
    }

    // ── parse (defsystem …) + optional (defvm …) ───────────────────
    if !opts.quiet {
        emit(&renderer, &mut stream, &mut stderr, UiEvent::Section {
            title: "parse".into(),
        })?;
    }
    let src = std::fs::read_to_string(&opts.in_path)?;
    let forms = read(&src)?;

    let mut system: Option<SystemConfig> = None;
    let mut vm: Option<VmSpec> = None;
    for f in &forms {
        match head_keyword(f).as_deref() {
            Some("defsystem") => system = Some(SystemConfig::compile_from_sexp(f)?),
            Some("defvm") => vm = Some(VmSpec::compile_from_sexp(f)?),
            _ => {}
        }
    }
    let system = system.ok_or("no (defsystem …) form found in input")?;

    if !opts.quiet {
        emit(&renderer, &mut stream, &mut stderr, UiEvent::Log {
            level: LogLevel::Success,
            message: format!(
                "parsed {} form(s) → system:{} vm:{}",
                forms.len(),
                system.hostname,
                vm.as_ref().map(|v| v.name.as_str()).unwrap_or("default"),
            ),
        })?;
    }

    // ── synthesize ──────────────────────────────────────────────────
    if !opts.quiet {
        emit(&renderer, &mut stream, &mut stderr, UiEvent::Section {
            title: "synthesize".into(),
        })?;
    }
    let synth = {
        let mut s = BootSynthesizer::new()
            .with_out_prefix(".")
            .with_busybox(opts.busybox)
            .with_init_binary_path(opts.init_path.clone().unwrap_or_else(|| {
                "${pkgs.hello}/bin/hello".into()
            }));
        if let Some(v) = vm {
            s = s.with_vm_override(v);
        }
        s
    };
    let t0 = Instant::now();
    let arts = synth.generate_all(&system);
    let synth_ms = t0.elapsed().as_millis() as u64;
    if !opts.quiet {
        emit(&renderer, &mut stream, &mut stderr, UiEvent::Log {
            level: LogLevel::Info,
            message: format!("synthesized {} artifact(s)", arts.len()),
        })?;
    }

    // ── write artifacts, painting a line per file with cache state ──
    if !opts.quiet {
        emit(&renderer, &mut stream, &mut stderr, UiEvent::Section {
            title: "emit".into(),
        })?;
    }
    std::fs::create_dir_all(&opts.out_dir)?;
    let mut built = 0usize;
    let mut cached = 0usize;
    let mut failed = 0usize;
    for a in &arts {
        let dst = opts.out_dir.join(trim_leading_dot(&a.path));
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Content-address the artifact by its bytes so we can emit a
        // stable short hash alongside the cache-state chip.
        let hash_full = blake3::hash(a.content.as_bytes()).to_hex();
        let short = ShortHash::from_blake3_hex(hash_full.as_str());
        let start = Instant::now();
        let existed = dst.exists() && std::fs::read(&dst).ok().is_some_and(|b| b == a.content.as_bytes());
        let state = if existed {
            cached += 1;
            ArtifactState::Cached
        } else {
            match std::fs::write(&dst, &a.content) {
                Ok(_) => {
                    if dst.file_name().is_some_and(|n| n == "boot.sh") {
                        let _ = set_executable(&dst);
                    }
                    built += 1;
                    ArtifactState::Built {
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    }
                }
                Err(e) => {
                    failed += 1;
                    ArtifactState::Failed {
                        reason: e.to_string(),
                    }
                }
            }
        };
        if !opts.quiet {
            emit(&renderer, &mut stream, &mut stderr, UiEvent::Artifact {
                name: display_name(&a.path),
                hash: short,
                state,
            })?;
        }
    }

    // ── summary ─────────────────────────────────────────────────────
    if !opts.quiet {
        let run_hash = ShortHash::from_blake3_hex(&stream.run_hash());
        // IMPORTANT: render summary from renderer only, don't push to
        // stream (would change the stream hash we just computed).
        let summary = UiEvent::Summary {
            root_hash: run_hash,
            total: arts.len(),
            built,
            cached,
            failed,
        };
        renderer.render_one(&summary, &mut stderr)?;
        let _ = writeln!(
            &mut stderr,
            "  {} synth {:.1}s · out {}",
            tatara_ui::Sigil::Clock.glyph(),
            synth_ms as f64 / 1000.0,
            opts.out_dir.display()
        );

        // Persist the event stream so `--replay <file>` works deterministically.
        let replay_path = opts.out_dir.join(".tatara-run.json");
        if let Ok(json) = serde_json::to_string_pretty(&stream) {
            let _ = std::fs::write(&replay_path, json);
        }
    }

    Ok(())
}

fn emit(
    r: &Renderer,
    s: &mut EventStream,
    w: &mut impl Write,
    e: UiEvent,
) -> std::io::Result<()> {
    r.render_one(&e, w)?;
    s.push(e);
    Ok(())
}

fn head_keyword(s: &Sexp) -> Option<String> {
    match s {
        Sexp::List(items) if !items.is_empty() => items[0].as_symbol().map(String::from),
        _ => None,
    }
}

fn trim_leading_dot(p: &str) -> &str {
    p.strip_prefix("./").unwrap_or(p)
}

fn display_name(p: &str) -> String {
    trim_leading_dot(p).to_string()
}

#[cfg(unix)]
fn set_executable(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(p)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(p, perms)
}

#[cfg(not(unix))]
fn set_executable(_p: &Path) -> std::io::Result<()> {
    Ok(())
}
