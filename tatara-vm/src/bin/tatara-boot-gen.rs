//! tatara-boot-gen — read a tatara-lisp boot file, emit every artifact
//! needed to run a tatara-os guest on Darwin (or any host with vfkit).
//!
//! Usage:
//!   tatara-boot-gen <in.lisp> <out-dir> [--init-path /nix/store/.../bin/tatara-init]
//!
//! The input file should contain at least one `(defsystem …)` form. Any
//! `(defvm …)` form present is used as the VM-shape override; otherwise we
//! fall back to `VmSpec::plex_default`.
//!
//! Outputs dropped into `<out-dir>/`:
//!   - system.json, init.lisp, kernel.nix, initrd.nix, vm.json, boot.sh, README.md

use std::path::{Path, PathBuf};

use tatara_lisp::{domain::TataraDomain, read, Sexp};
use tatara_nix::MultiSynthesizer;
use tatara_os::SystemConfig;
use tatara_vm::{boot::BootSynthesizer, VmSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: tatara-boot-gen <in.lisp> <out-dir> [--init-path <path>]"
        );
        std::process::exit(2);
    }
    let in_path = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    let mut init_path: Option<String> = None;
    let mut busybox = true;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--init-path" => {
                init_path = Some(
                    args.get(i + 1)
                        .cloned()
                        .ok_or("--init-path needs an argument")?,
                );
                i += 2;
            }
            "--no-busybox" => {
                busybox = false;
                i += 1;
            }
            other => return Err(format!("unknown arg: {other}").into()),
        }
    }

    let src = std::fs::read_to_string(&in_path)?;
    let forms = read(&src)?;

    let mut system: Option<SystemConfig> = None;
    let mut vm: Option<VmSpec> = None;
    for f in &forms {
        match head_keyword(f).as_deref() {
            Some("defsystem") => {
                system = Some(SystemConfig::compile_from_sexp(f)?);
            }
            Some("defvm") => {
                vm = Some(VmSpec::compile_from_sexp(f)?);
            }
            _ => {}
        }
    }
    let system = system.ok_or("no (defsystem …) form found in input")?;

    let synth = BootSynthesizer::new()
        .with_out_prefix(".")
        .with_busybox(busybox)
        .with_init_binary_path(init_path.unwrap_or_else(|| {
            // Reasonable default: the store path the user will realize once
            // `tatara.packages.aarch64-linux.init` completes. Placeholder
            // otherwise — our VfkitEmitter prints nothing path-sensitive
            // in that branch.
            "${pkgs.hello}/bin/hello".into()
        }));
    let synth = match vm {
        Some(v) => synth.with_vm_override(v),
        None => synth,
    };

    let arts = synth.generate_all(&system);

    std::fs::create_dir_all(&out_dir)?;
    for a in &arts {
        let dst = out_dir.join(trim_leading_dot(&a.path));
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dst, &a.content)?;
        // boot.sh gets +x.
        if dst.file_name().is_some_and(|n| n == "boot.sh") {
            set_executable(&dst)?;
        }
    }
    eprintln!(
        "[tatara-boot-gen] wrote {} artifact(s) to {}",
        arts.len(),
        out_dir.display()
    );
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
