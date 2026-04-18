//! Realizer — take a typed `Derivation` and produce an actual on-disk artifact.
//!
//! Two backends ship here:
//!
//!   - `InProcessRealizer` — self-contained hermetic builder. Writes outputs
//!     into a content-addressed store directory (default `$XDG_DATA_HOME/tatara/store`).
//!     Useful for offline tests, sealed environments, and machines without Nix.
//!
//!   - `NixStoreRealizer` — binds to an existing Nix on disk. Emits a minimal
//!     `derivation { ... }` Nix expression, shells out to `nix build --impure
//!     --expr`, and records the resulting `/nix/store/...` path. You own the
//!     linguistic layer (tatara-lisp + tatara-eval); Nix owns the store and the
//!     build machinery — exactly the division the user asked for.
//!
//! Both impls share one trait so callers can swap them without changing code.
//! Both respect `Derivation::store_path()` for our own content addressing,
//! which stays stable regardless of backend choice.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

use crate::derivation::{BuilderPhase, Derivation, Source};
use crate::evaluator::EvaluationResult;
use crate::store::StorePath;

#[derive(Debug, Error)]
pub enum RealizeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("build failed for {name}: {reason}")]
    Build { name: String, reason: String },

    #[error("unsupported source kind for realization: {0:?}")]
    UnsupportedSource(String),

    #[error("nix invocation failed: {0}")]
    Nix(String),

    #[error("utf8 in build log: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

pub type Result<T> = std::result::Result<T, RealizeError>;

/// Result of realizing a derivation — the typed record of what was built, what
/// path it lives at, and what the build log looked like.
#[derive(Clone, Debug)]
pub struct RealizedArtifact {
    /// Our content-addressed `StorePath` (stable across backends).
    pub store_path: StorePath,
    /// Where this artifact actually lives on this filesystem.
    pub path: PathBuf,
    /// Per-output filesystem paths. Primary output is under key `primary`.
    pub outputs: BTreeMap<String, PathBuf>,
    /// Build log (may be empty for cached hits).
    pub log: Vec<String>,
    /// Whether we built it or hit the cache.
    pub cached: bool,
}

impl RealizedArtifact {
    pub fn as_evaluation_result(&self) -> EvaluationResult {
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "out".to_string(),
            self.store_path.clone(),
        );
        EvaluationResult {
            store_path: self.store_path.clone(),
            outputs,
            log: self.log.clone(),
        }
    }
}

/// Backend-agnostic realizer.
pub trait Realizer {
    fn realize(&self, d: &Derivation) -> Result<RealizedArtifact>;

    /// Build in topological order. Default impl just maps realize; backends
    /// that know a build DAG may override.
    fn realize_many(&self, ds: &[Derivation]) -> Result<Vec<RealizedArtifact>> {
        ds.iter().map(|d| self.realize(d)).collect()
    }
}

// ── InProcess — self-contained hermetic builder ──────────────────────────

/// Builds derivations into a content-addressed directory.
///
/// No Nix required. Uses `/bin/sh` for the build script. One builder-specified
/// temp directory per build; outputs are atomically moved into the store.
pub struct InProcessRealizer {
    store_dir: PathBuf,
}

impl InProcessRealizer {
    pub fn new(store_dir: impl Into<PathBuf>) -> Self {
        Self {
            store_dir: store_dir.into(),
        }
    }

    /// Default store — honors `$TATARA_STORE_DIR`, then `$XDG_DATA_HOME/tatara/store`,
    /// then `$HOME/.local/share/tatara/store`, then `/tmp/tatara-store`.
    pub fn default_store() -> Self {
        let path = std::env::var_os("TATARA_STORE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .map(|p| p.join("tatara/store"))
            })
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|p| p.join(".local/share/tatara/store"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/tatara-store"));
        Self::new(path)
    }

    pub fn store_dir(&self) -> &Path {
        &self.store_dir
    }

    fn output_path_for(&self, d: &Derivation) -> PathBuf {
        self.store_dir.join(d.store_path().render())
    }
}

impl Realizer for InProcessRealizer {
    fn realize(&self, d: &Derivation) -> Result<RealizedArtifact> {
        std::fs::create_dir_all(&self.store_dir)?;
        let out_path = self.output_path_for(d);
        let store_path = d.store_path();

        // Cache hit: the output already exists at its content-addressed
        // location, so there's nothing to build. Just report it.
        if out_path.exists() {
            let mut outputs = BTreeMap::new();
            outputs.insert("primary".to_string(), out_path.clone());
            return Ok(RealizedArtifact {
                store_path,
                path: out_path,
                outputs,
                log: vec![format!("[cache] {}", d.name)],
                cached: true,
            });
        }

        let work = tempfile::tempdir()?;
        let work_path = work.path();

        // Materialize source — `$src` is either a single file or a directory.
        let src_path = materialize_source(&d.source, work_path)?;

        // Per-build directory for the primary output (we move it into the
        // store at the end, atomically).
        let scratch_out = work_path.join(".tatara-out");
        std::fs::create_dir_all(&scratch_out)?;
        let scratch_out_file = work_path.join(".tatara-out-file");

        let mut log: Vec<String> = Vec::new();
        let phases = if d.builder.phases.is_empty() {
            default_phases(&d.builder.commands)
        } else {
            d.builder.phases.clone()
        };

        for phase in &phases {
            let phase_name = phase_display(*phase);
            let commands = d
                .builder
                .commands
                .get(&phase_name)
                .cloned()
                .unwrap_or_default();
            if commands.is_empty() {
                continue;
            }
            let joined = commands.join("\n");
            log.push(format!("[{phase_name}] $ {joined}"));

            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-eu").arg("-c").arg(&joined);
            cmd.current_dir(work_path);
            // Standard build environment — mirrors Nix's env conventions.
            cmd.env_clear();
            cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
            cmd.env("HOME", work_path);
            cmd.env("TMPDIR", work_path);
            cmd.env("src", &src_path);
            // Expose both "$out as directory" (for multi-file installs) and
            // "$out as file" (for a single artifact). We post-process below.
            cmd.env("out", &scratch_out);
            cmd.env("out_file", &scratch_out_file);
            // User env
            for kv in &d.env {
                cmd.env(&kv.name, &kv.value);
            }

            let output = cmd.output()?;
            if !output.status.success() {
                return Err(RealizeError::Build {
                    name: d.name.clone(),
                    reason: format!(
                        "phase {phase_name} exited with {}:\nstdout:\n{}\nstderr:\n{}",
                        output.status,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    ),
                });
            }
            if !output.stdout.is_empty() {
                log.push(String::from_utf8(output.stdout)?);
            }
            if !output.stderr.is_empty() {
                log.push(String::from_utf8(output.stderr)?);
            }
        }

        // Decide whether the build produced a single file (via $out_file) or a
        // directory (via $out). File wins if present.
        let final_path;
        if scratch_out_file.exists() {
            final_path = out_path.clone();
            if let Some(parent) = final_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&scratch_out_file, &final_path)
                .or_else(|_| copy_then_remove(&scratch_out_file, &final_path))?;
        } else {
            final_path = out_path.clone();
            if final_path.exists() {
                // race: someone else got there first. Accept their result.
            } else {
                std::fs::rename(&scratch_out, &final_path)
                    .or_else(|_| copy_dir_then_remove(&scratch_out, &final_path))?;
            }
        }

        let mut outputs = BTreeMap::new();
        outputs.insert("primary".to_string(), final_path.clone());
        Ok(RealizedArtifact {
            store_path,
            path: final_path,
            outputs,
            log,
            cached: false,
        })
    }
}

fn materialize_source(src: &Source, work: &Path) -> Result<PathBuf> {
    match src {
        Source::Inline { content } => {
            let p = work.join("src");
            std::fs::write(&p, content)?;
            Ok(p)
        }
        Source::Path { path } => Ok(PathBuf::from(path)),
        Source::Git { .. } | Source::Tarball { .. } | Source::Derivation { .. } => {
            Err(RealizeError::UnsupportedSource(format!("{src:?}")))
        }
    }
}

fn default_phases(
    commands: &std::collections::BTreeMap<String, Vec<String>>,
) -> Vec<BuilderPhase> {
    // If the user named specific phases in `commands` but gave no `phases`
    // list, respect the names.
    let mut out = Vec::new();
    for phase in [
        BuilderPhase::Unpack,
        BuilderPhase::Patch,
        BuilderPhase::Configure,
        BuilderPhase::Build,
        BuilderPhase::Check,
        BuilderPhase::Install,
        BuilderPhase::Fixup,
        BuilderPhase::InstallCheck,
        BuilderPhase::Dist,
    ] {
        if commands.contains_key(&phase_display(phase)) {
            out.push(phase);
        }
    }
    out
}

fn phase_display(p: BuilderPhase) -> String {
    match p {
        BuilderPhase::Unpack => "Unpack",
        BuilderPhase::Patch => "Patch",
        BuilderPhase::Configure => "Configure",
        BuilderPhase::Build => "Build",
        BuilderPhase::Check => "Check",
        BuilderPhase::Install => "Install",
        BuilderPhase::Fixup => "Fixup",
        BuilderPhase::InstallCheck => "InstallCheck",
        BuilderPhase::Dist => "Dist",
    }
    .to_string()
}

fn copy_then_remove(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)?;
    Ok(())
}

fn copy_dir_then_remove(from: &Path, to: &Path) -> std::io::Result<()> {
    copy_dir_recursive(from, to)?;
    std::fs::remove_dir_all(from)?;
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

// ── NixStoreRealizer — bind to the live `/nix/store` ─────────────────────

/// Realizer that hands the build off to an already-installed Nix on disk.
///
/// We own the linguistic layer (Lisp + tatara-eval); Nix owns the store and
/// the build machinery. The bridge is a minimal `derivation { ... }`
/// expression we pipe through `nix build --impure --expr`.
pub struct NixStoreRealizer {
    /// Which Nix binary to invoke. Defaults to `nix` from PATH.
    pub nix_binary: PathBuf,
    /// Explicit system string (e.g., "x86_64-linux"). Default: builtins.currentSystem.
    pub system: Option<String>,
    /// When true (default), pull a minimal stdenv (coreutils + bash) from
    /// `<nixpkgs>` and expose its bin on `PATH` inside the builder. Set false
    /// if the caller's script provides its own inputs.
    pub stdenv_path: bool,
}

impl Default for NixStoreRealizer {
    fn default() -> Self {
        Self {
            nix_binary: PathBuf::from("nix"),
            system: None,
            stdenv_path: true,
        }
    }
}

impl NixStoreRealizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_binary(mut self, p: impl Into<PathBuf>) -> Self {
        self.nix_binary = p.into();
        self
    }

    pub fn with_system(mut self, s: impl Into<String>) -> Self {
        self.system = Some(s.into());
        self
    }

    pub fn with_stdenv_path(mut self, on: bool) -> Self {
        self.stdenv_path = on;
        self
    }

    /// Convert a tatara `Derivation` into a minimal Nix expression string.
    /// Keeps the same build contract (phases, env, source, outputs) but drops
    /// the contract onto Nix's own derivation primitive.
    pub fn to_nix_expr(&self, d: &Derivation) -> Result<String> {
        // Source: materialize Inline into a builtins.toFile; pass Path through.
        let src_expr = match &d.source {
            Source::Inline { content } => {
                format!(
                    "builtins.toFile \"src\" {}",
                    nix_string(content)
                )
            }
            Source::Path { path } => format!("{}", path), // unquoted path literal
            other => {
                return Err(RealizeError::UnsupportedSource(format!("{other:?}")))
            }
        };
        // Build a shell script from the phase commands.
        let phases = if d.builder.phases.is_empty() {
            default_phases(&d.builder.commands)
        } else {
            d.builder.phases.clone()
        };
        let mut script = String::from("set -eu\n");
        for phase in &phases {
            let name = phase_display(*phase);
            if let Some(cmds) = d.builder.commands.get(&name) {
                script.push_str(&format!("# phase: {name}\n"));
                for c in cmds {
                    script.push_str(c);
                    script.push('\n');
                }
            }
        }

        let system_expr = match &self.system {
            Some(s) => format!("\"{s}\""),
            None => "builtins.currentSystem".to_string(),
        };

        let mut env_lines = String::new();
        for kv in &d.env {
            env_lines.push_str(&format!(
                "    {} = {};\n",
                nix_identifier(&kv.name),
                nix_string(&kv.value),
            ));
        }

        let version = match &d.version {
            Some(v) => format!("-{v}"),
            None => String::new(),
        };

        // When stdenv_path is on, wrap the derivation in a `let` that imports
        // <nixpkgs> and exposes a PATH containing coreutils + bash. Most
        // tatara derivations assume basic Unix utilities exist in the builder;
        // Nix's empty sandbox PATH trips up naive scripts otherwise.
        let (prelude, builder_expr, path_attr) = if self.stdenv_path {
            (
                "let pkgs = import <nixpkgs> {}; in\n".to_string(),
                "\"${pkgs.bash}/bin/sh\"".to_string(),
                "    PATH = \"${pkgs.coreutils}/bin:${pkgs.bash}/bin\";\n"
                    .to_string(),
            )
        } else {
            (String::new(), "\"/bin/sh\"".to_string(), String::new())
        };

        let expr = format!(
            "{prelude}(derivation {{\n\
             \x20   name = \"{name}{version}\";\n\
             \x20   system = {system_expr};\n\
             \x20   builder = {builder_expr};\n\
             \x20   args = [ \"-c\" {script} ];\n\
             \x20   src = {src};\n\
             {path_attr}\
             {env}}})",
            prelude = prelude,
            name = d.name,
            version = version,
            system_expr = system_expr,
            builder_expr = builder_expr,
            script = nix_string(&script),
            src = src_expr,
            path_attr = path_attr,
            env = env_lines,
        );
        Ok(expr)
    }
}

impl Realizer for NixStoreRealizer {
    fn realize(&self, d: &Derivation) -> Result<RealizedArtifact> {
        let expr = self.to_nix_expr(d)?;
        let mut cmd = Command::new(&self.nix_binary);
        cmd.arg("build")
            .arg("--impure")
            .arg("--no-link")
            .arg("--print-out-paths")
            .arg("--expr")
            .arg(&expr);
        let out = cmd.output()?;
        if !out.status.success() {
            return Err(RealizeError::Nix(format!(
                "`nix build` for {} exited with {}:\nexpr:\n{expr}\nstderr:\n{}",
                d.name,
                out.status,
                String::from_utf8_lossy(&out.stderr),
            )));
        }
        let path = String::from_utf8(out.stdout)?
            .trim()
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        if path.is_empty() {
            return Err(RealizeError::Nix(
                "`nix build` returned no output path".into(),
            ));
        }
        let pbuf = PathBuf::from(&path);
        let mut outputs = BTreeMap::new();
        outputs.insert("primary".to_string(), pbuf.clone());
        Ok(RealizedArtifact {
            store_path: d.store_path(),
            path: pbuf,
            outputs,
            log: vec![expr],
            cached: false,
        })
    }
}

fn nix_string(s: &str) -> String {
    // Always use indented-string ''...'' if there's a newline; else plain.
    if s.contains('\n') {
        format!("''\n{}''", s.replace("''", "''''"))
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn nix_identifier(s: &str) -> String {
    // Conservative: quote anything non-trivial.
    if s.chars().all(|c| c.is_alphanumeric() || c == '_') && !s.is_empty() {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('"', "\\\""))
    }
}

// ── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivation::{BuilderPhase, BuilderPhases};
    use tempfile::TempDir;

    fn mk_hello(src: &str) -> Derivation {
        let mut commands = BTreeMap::new();
        commands.insert(
            "Install".to_string(),
            vec!["cat \"$src\" > \"$out_file\"".to_string()],
        );
        Derivation {
            name: "hello-inline".into(),
            version: Some("1.0".into()),
            inputs: vec![],
            source: Source::Inline {
                content: src.into(),
            },
            builder: BuilderPhases {
                phases: vec![BuilderPhase::Install],
                commands,
            },
            outputs: Default::default(),
            env: vec![],
            sandbox: Default::default(),
        }
    }

    #[test]
    fn in_process_realizes_inline_source() {
        let td = TempDir::new().unwrap();
        let r = InProcessRealizer::new(td.path());
        let d = mk_hello("hello world\n");
        let art = r.realize(&d).unwrap();
        assert!(!art.cached);
        assert!(art.path.exists(), "output file should exist: {:?}", art.path);
        let contents = std::fs::read_to_string(&art.path).unwrap();
        assert_eq!(contents, "hello world\n");
    }

    #[test]
    fn in_process_is_content_addressed_and_cached() {
        let td = TempDir::new().unwrap();
        let r = InProcessRealizer::new(td.path());
        let d = mk_hello("deterministic\n");
        let a1 = r.realize(&d).unwrap();
        let a2 = r.realize(&d).unwrap();
        assert_eq!(a1.store_path, a2.store_path);
        assert_eq!(a1.path, a2.path);
        assert!(!a1.cached);
        assert!(a2.cached);
    }

    #[test]
    fn nix_expr_embeds_phase_script_and_inline_source() {
        // Disable stdenv_path for this assertion so the builder line is
        // predictable; the default includes a <nixpkgs> `let` prelude.
        let r = NixStoreRealizer::new()
            .with_system("x86_64-linux")
            .with_stdenv_path(false);
        let d = mk_hello("payload\n");
        let expr = r.to_nix_expr(&d).unwrap();
        assert!(expr.contains("name = \"hello-inline-1.0\""));
        assert!(expr.contains("system = \"x86_64-linux\""));
        assert!(expr.contains("builder = \"/bin/sh\""));
        // The script is embedded; indented-string form because it has newlines.
        assert!(expr.contains("phase: Install"));
        assert!(expr.contains("cat \"$src\" > \"$out_file\""));
        // Inline source is turned into builtins.toFile.
        assert!(expr.contains("builtins.toFile \"src\""));
    }

    #[test]
    fn nix_expr_default_pulls_stdenv_from_nixpkgs() {
        let r = NixStoreRealizer::new();
        let d = mk_hello("payload\n");
        let expr = r.to_nix_expr(&d).unwrap();
        assert!(expr.starts_with("let pkgs = import <nixpkgs>"));
        assert!(expr.contains("builder = \"${pkgs.bash}/bin/sh\""));
        assert!(expr.contains("PATH = \"${pkgs.coreutils}/bin:${pkgs.bash}/bin\""));
    }

    /// Proves the NixStoreRealizer actually binds to a live `/nix/store`.
    /// Marked `#[ignore]` so CI stays hermetic — run explicitly with:
    ///   cargo test -p tatara-nix -- --ignored nix_store_realizer_binds_to_live_nix
    #[test]
    #[ignore]
    fn nix_store_realizer_binds_to_live_nix() {
        let mut commands = BTreeMap::new();
        commands.insert(
            "Install".to_string(),
            vec!["mkdir -p $out && cp \"$src\" $out/message.txt".to_string()],
        );
        let d = Derivation {
            name: "tatara-hello".into(),
            version: Some("1.0".into()),
            inputs: vec![],
            source: Source::Inline {
                content: "hello from tatara via live nix\n".into(),
            },
            builder: BuilderPhases {
                phases: vec![BuilderPhase::Install],
                commands,
            },
            outputs: Default::default(),
            env: vec![],
            sandbox: Default::default(),
        };
        let r = NixStoreRealizer::new();
        let art = r.realize(&d).expect("nix build should succeed");
        assert!(art.path.starts_with("/nix/store"));
        let msg = std::fs::read_to_string(art.path.join("message.txt")).unwrap();
        assert_eq!(msg, "hello from tatara via live nix\n");
    }

    #[test]
    fn default_store_honors_env_var() {
        let saved = std::env::var_os("TATARA_STORE_DIR");
        // SAFETY: test accessed in-proc only, env is process-global; tests run
        // with --test-threads settable by caller. We restore after.
        unsafe { std::env::set_var("TATARA_STORE_DIR", "/tmp/tatara-test-override"); }
        let r = InProcessRealizer::default_store();
        assert_eq!(r.store_dir(), std::path::Path::new("/tmp/tatara-test-override"));
        match saved {
            Some(v) => unsafe { std::env::set_var("TATARA_STORE_DIR", v) },
            None => unsafe { std::env::remove_var("TATARA_STORE_DIR") },
        }
    }
}
