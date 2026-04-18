//! tatara-terreiro — CLI for sealed Lisp virtual environments.
//!
//! Commands:
//!
//!   new         <spec.lisp>                        — parse a (defcompiler …) form, print identity
//!   seal        <spec.lisp> <snapshot.json>        — seal + write snapshot
//!   load        <snapshot.json>                    — restore + print identity
//!   compile     <spec.lisp|snapshot.json> <src.lisp> — macro-expand through the terreiro
//!   eval        <spec.lisp|snapshot.json> <src.lisp> — evaluate via tatara-eval (needs :with-interpreter)
//!   realize     <spec.lisp|snapshot.json> <src.lisp> <out-dir>
//!               evaluate to a Derivation + build it into out-dir
//!   id          <spec.lisp|snapshot.json>          — print only the BLAKE3 content id
//!
//! Output is Nord-themed via tatara-ui when stderr is a tty. `NO_COLOR`
//! is honored. Snapshot files round-trip byte-identically (identity hash
//! is stable across saves).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tatara_terreiro::{Terreiro, TerreiroError};
use tatara_ui::{
    ArtifactState, EventStream, LogLevel, Renderer, ShortHash, ThemeSpec, UiEvent,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return ExitCode::from(2);
    }
    let cmd = args[1].as_str();
    let rest = &args[2..];

    let theme = ThemeSpec::nord_arctic();
    let renderer = Renderer::new(theme.to_role_map());
    let mut stderr = std::io::stderr();
    let mut stream = EventStream::new();

    let res = match cmd {
        "new" | "id" => cmd_id(rest, &renderer, &mut stream, &mut stderr),
        "seal" => cmd_seal(rest, &renderer, &mut stream, &mut stderr),
        "load" => cmd_load(rest, &renderer, &mut stream, &mut stderr),
        "compile" => cmd_compile(rest, &renderer, &mut stream, &mut stderr),
        "eval" => cmd_eval(rest, &renderer, &mut stream, &mut stderr),
        "realize" => cmd_realize(rest, &renderer, &mut stream, &mut stderr),
        "help" | "--help" | "-h" => {
            usage();
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            usage();
            return ExitCode::from(2);
        }
    };

    match res {
        Ok(code) => code,
        Err(e) => {
            let _ = renderer.render_one(
                &UiEvent::Log {
                    level: LogLevel::Error,
                    message: format!("{e}"),
                },
                &mut stderr,
            );
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!(
        "\
tatara-terreiro — sealed Lisp virtual environments

Usage:
  tatara-terreiro new      <spec.lisp>                           parse + print id
  tatara-terreiro seal     <spec.lisp> <snapshot.json>           seal + write snapshot
  tatara-terreiro load     <snapshot.json>                       restore + print id
  tatara-terreiro compile  <spec|snapshot> <src.lisp>            macro-expand through the terreiro
  tatara-terreiro eval     <spec|snapshot> <src.lisp>            evaluate via tatara-eval
  tatara-terreiro realize  <spec|snapshot> <src.lisp> <out-dir>  evaluate + build the resulting derivation
  tatara-terreiro id       <spec|snapshot>                       print only the BLAKE3 id
  tatara-terreiro help                                           this message

Flags:
  NO_COLOR env var disables ANSI output.
"
    );
}

/// Load a Terreiro from either a raw `(defcompiler …)` Lisp source file
/// or a previously-serialized snapshot JSON.
fn load_terreiro(path: &Path) -> Result<Terreiro, TerreiroError> {
    let bytes = std::fs::read(path)?;
    // Heuristic: JSON starts with `{` (possibly after whitespace).
    let first_nonws = bytes.iter().find(|b| !b.is_ascii_whitespace()).copied();
    if first_nonws == Some(b'{') {
        return Terreiro::load_from(path);
    }
    let src = String::from_utf8(bytes).map_err(|e| TerreiroError::Io(std::io::Error::other(e)))?;
    Terreiro::from_spec_lisp(&src)
}

fn banner(r: &Renderer, s: &mut EventStream, w: &mut impl Write, title: &str) -> std::io::Result<()> {
    let e = UiEvent::Banner {
        title: title.into(),
        subtitle: None,
    };
    r.render_one(&e, w)?;
    s.push(e);
    Ok(())
}

fn emit(r: &Renderer, s: &mut EventStream, w: &mut impl Write, e: UiEvent) -> std::io::Result<()> {
    r.render_one(&e, w)?;
    s.push(e);
    Ok(())
}

// ── commands ────────────────────────────────────────────────────────────

fn cmd_id(
    args: &[String],
    r: &Renderer,
    s: &mut EventStream,
    w: &mut impl Write,
) -> Result<ExitCode, TerreiroError> {
    if args.is_empty() {
        usage();
        return Ok(ExitCode::from(2));
    }
    let mut t = load_terreiro(Path::new(&args[0]))?;
    let id = t.seal().clone();
    banner(r, s, w, "tatara-terreiro · id")?;
    emit(
        r,
        s,
        w,
        UiEvent::Log {
            level: LogLevel::Success,
            message: format!("terreiro:{}", id.short()),
        },
    )?;
    // stdout: just the hex id, pipeable.
    println!("{}", id.full());
    Ok(ExitCode::SUCCESS)
}

fn cmd_seal(
    args: &[String],
    r: &Renderer,
    s: &mut EventStream,
    w: &mut impl Write,
) -> Result<ExitCode, TerreiroError> {
    if args.len() < 2 {
        usage();
        return Ok(ExitCode::from(2));
    }
    banner(r, s, w, "tatara-terreiro · seal")?;
    let mut t = load_terreiro(Path::new(&args[0]))?;
    let id = t.seal().clone();
    t.write_to(&args[1])?;
    emit(
        r,
        s,
        w,
        UiEvent::Artifact {
            name: args[1].clone(),
            hash: ShortHash::from_blake3_hex(id.full()),
            state: ArtifactState::Built { elapsed_ms: 0 },
        },
    )?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_load(
    args: &[String],
    r: &Renderer,
    s: &mut EventStream,
    w: &mut impl Write,
) -> Result<ExitCode, TerreiroError> {
    if args.is_empty() {
        usage();
        return Ok(ExitCode::from(2));
    }
    banner(r, s, w, "tatara-terreiro · load")?;
    let t = Terreiro::load_from(&args[0])?;
    let id = t.id().cloned().ok_or(TerreiroError::NotSealed)?;
    emit(
        r,
        s,
        w,
        UiEvent::Log {
            level: LogLevel::Success,
            message: format!("loaded terreiro:{}  ({} macros)", id.short(), t.macro_count()),
        },
    )?;
    println!("{}", id.full());
    Ok(ExitCode::SUCCESS)
}

fn cmd_compile(
    args: &[String],
    r: &Renderer,
    s: &mut EventStream,
    w: &mut impl Write,
) -> Result<ExitCode, TerreiroError> {
    if args.len() < 2 {
        usage();
        return Ok(ExitCode::from(2));
    }
    banner(r, s, w, "tatara-terreiro · compile")?;
    let t = load_terreiro(Path::new(&args[0]))?;
    let src = std::fs::read_to_string(&args[1])?;
    let forms = t.compile(&src)?;
    emit(
        r,
        s,
        w,
        UiEvent::Log {
            level: LogLevel::Success,
            message: format!("expanded {} form(s)", forms.len()),
        },
    )?;
    for f in &forms {
        println!("{f}");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_eval(
    args: &[String],
    r: &Renderer,
    s: &mut EventStream,
    w: &mut impl Write,
) -> Result<ExitCode, TerreiroError> {
    if args.len() < 2 {
        usage();
        return Ok(ExitCode::from(2));
    }
    banner(r, s, w, "tatara-terreiro · eval")?;
    let t = load_terreiro(Path::new(&args[0]))?.with_interpreter();
    let src = std::fs::read_to_string(&args[1])?;
    let v = t.eval(&src)?;
    emit(
        r,
        s,
        w,
        UiEvent::Log {
            level: LogLevel::Success,
            message: format!("evaluated to <{}>", v.type_name()),
        },
    )?;
    println!("{v:?}");
    Ok(ExitCode::SUCCESS)
}

fn cmd_realize(
    args: &[String],
    r: &Renderer,
    s: &mut EventStream,
    w: &mut impl Write,
) -> Result<ExitCode, TerreiroError> {
    if args.len() < 3 {
        usage();
        return Ok(ExitCode::from(2));
    }
    banner(r, s, w, "tatara-terreiro · realize")?;
    let store_dir = PathBuf::from(&args[2]);
    let t = load_terreiro(Path::new(&args[0]))?
        .with_interpreter()
        .with_in_process_realizer(&store_dir);
    let src = std::fs::read_to_string(&args[1])?;
    let art = t.realize(&src)?;
    let state = if art.cached {
        ArtifactState::Cached
    } else {
        ArtifactState::Built { elapsed_ms: 0 }
    };
    emit(
        r,
        s,
        w,
        UiEvent::Artifact {
            name: art.path.display().to_string(),
            hash: ShortHash::from_blake3_hex(&art.store_path.hash.0),
            state,
        },
    )?;
    println!("{}", art.path.display());
    Ok(ExitCode::SUCCESS)
}
