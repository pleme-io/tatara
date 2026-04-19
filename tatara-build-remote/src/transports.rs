//! Concrete `BuildTransport` implementations.
//!
//! Every transport shells out to already-installed Nix / Attic / git CLIs.
//! We're not re-implementing the Nix wire protocol — the point is to
//! declaratively compose existing tools into a priority-ordered chain
//! driven by `(defguest …)`. If a tool isn't on PATH, the transport
//! returns a typed `BuildError` and the layered chain advances.
//!
//! The trio:
//!
//! - `AtticTransport` — `attic get <cache> <store-path>` (or push / pull).
//!   Cache hits are fast; misses return `BuildError::Attic`.
//! - `SshRemoteTransport` — `nix copy --from ssh-ng://<host> <ref>` +
//!   `nix build --builders 'ssh-ng://<host>'`. Delegates build work to
//!   a remote builder.
//! - `LocalTransport` — `nix build` on the host. Works for any BuildRef.
//!
//! All three honor `BuildRef::StorePath` as a fast path: if the path is
//! already in the store, we return it without shelling out.

use std::path::Path;
use std::process::Command;

use crate::{BuildError, BuildRef, BuildTransport, StorePath};

/// `attic` CLI transport against a named cache (e.g. `"quero.lol"`).
pub struct AtticTransport {
    pub cache: String,
}

impl AtticTransport {
    #[must_use]
    pub fn new(cache: impl Into<String>) -> Self {
        Self {
            cache: cache.into(),
        }
    }
}

impl BuildTransport for AtticTransport {
    fn fetch(&self, reference: &BuildRef) -> Result<StorePath, BuildError> {
        if !tool_on_path("attic") {
            return Err(BuildError::Attic("attic CLI not on PATH".into()));
        }

        match reference {
            BuildRef::StorePath(p) => {
                // Already fully-qualified — just make sure it's in the
                // store locally. Attic has a pull-by-store-path mode.
                if Path::new(p).exists() {
                    return Ok(StorePath(p.clone()));
                }
                let status = Command::new("attic")
                    .args(["get", &self.cache, p])
                    .status()
                    .map_err(|e| BuildError::Attic(format!("spawn attic: {e}")))?;
                if status.success() {
                    Ok(StorePath(p.clone()))
                } else {
                    Err(BuildError::Attic(format!(
                        "attic get {}/{p} exited {status}",
                        self.cache
                    )))
                }
            }
            // For Flake / Nix / Oci, Attic alone doesn't know how to
            // evaluate. The layered chain's next transport handles it.
            BuildRef::Flake { .. } | BuildRef::Nix { .. } | BuildRef::Oci { .. } => {
                Err(BuildError::Attic(
                    "attic transport only satisfies StorePath; defer to ssh-ng/local".into(),
                ))
            }
        }
    }
}

/// Remote Nix builder over ssh-ng (e.g. `ssh://builder.quero.lol`).
pub struct SshRemoteTransport {
    pub ssh_uri: String,
}

impl SshRemoteTransport {
    #[must_use]
    pub fn new(ssh_uri: impl Into<String>) -> Self {
        Self {
            ssh_uri: ssh_uri.into(),
        }
    }

    /// Normalize `ssh://host` → `ssh-ng://host` because Nix's native
    /// builder protocol prefers the `-ng` scheme.
    fn nix_builder_uri(&self) -> String {
        if let Some(rest) = self.ssh_uri.strip_prefix("ssh://") {
            format!("ssh-ng://{rest}")
        } else {
            self.ssh_uri.clone()
        }
    }
}

impl BuildTransport for SshRemoteTransport {
    fn fetch(&self, reference: &BuildRef) -> Result<StorePath, BuildError> {
        if !tool_on_path("nix") {
            return Err(BuildError::Remote("nix CLI not on PATH".into()));
        }
        let builder = self.nix_builder_uri();
        match reference {
            BuildRef::Flake { url, attr } => run_nix_build(
                &[
                    "build",
                    &format!("{url}#{attr}"),
                    "--print-out-paths",
                    "--no-link",
                    "--builders",
                    &builder,
                ],
                BuildError::Remote,
            ),
            BuildRef::Nix { expr } => run_nix_build(
                &[
                    "build",
                    "--impure",
                    "--expr",
                    expr,
                    "--print-out-paths",
                    "--no-link",
                    "--builders",
                    &builder,
                ],
                BuildError::Remote,
            ),
            BuildRef::StorePath(p) => {
                // Ask the remote to ensure this path is realized + copy
                // it here. `nix copy --from` pulls from the remote.
                let status = Command::new("nix")
                    .args([
                        "copy",
                        "--from",
                        &builder,
                        p,
                        "--no-check-sigs",
                    ])
                    .status()
                    .map_err(|e| BuildError::Remote(format!("spawn nix copy: {e}")))?;
                if status.success() {
                    Ok(StorePath(p.clone()))
                } else {
                    Err(BuildError::Remote(format!(
                        "nix copy --from {builder} {p} exited {status}"
                    )))
                }
            }
            BuildRef::Oci { image, tag } => Err(BuildError::Remote(format!(
                "OCI transport not yet wired: {image}:{tag}"
            ))),
        }
    }
}

/// Local `nix build` — the last-resort transport.
#[derive(Debug, Default)]
pub struct LocalTransport;

impl BuildTransport for LocalTransport {
    fn fetch(&self, reference: &BuildRef) -> Result<StorePath, BuildError> {
        if !tool_on_path("nix") {
            return Err(BuildError::Local("nix CLI not on PATH".into()));
        }
        match reference {
            BuildRef::Flake { url, attr } => run_nix_build(
                &["build", &format!("{url}#{attr}"), "--print-out-paths", "--no-link"],
                BuildError::Local,
            ),
            BuildRef::Nix { expr } => run_nix_build(
                &["build", "--impure", "--expr", expr, "--print-out-paths", "--no-link"],
                BuildError::Local,
            ),
            BuildRef::StorePath(p) => {
                if Path::new(p).exists() {
                    Ok(StorePath(p.clone()))
                } else {
                    // Attempt realization — nix will build if the path's
                    // derivation is known.
                    run_nix_build(
                        &["store", "realise", p, "--print-out-paths"],
                        BuildError::Local,
                    )
                }
            }
            BuildRef::Oci { image, tag } => Err(BuildError::Local(format!(
                "OCI transport not yet wired: {image}:{tag}"
            ))),
        }
    }
}

// ── helpers ─────────────────────────────────────────────────────────

fn tool_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths).find_map(|dir| {
                let candidate = dir.join(name);
                candidate.exists().then_some(())
            })
        })
        .is_some()
}

fn run_nix_build(args: &[&str], err_wrap: fn(String) -> BuildError) -> Result<StorePath, BuildError> {
    let output = Command::new("nix")
        .args(args)
        .output()
        .map_err(|e| err_wrap(format!("spawn nix: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(err_wrap(format!(
            "nix {} exited {}: {}",
            args.join(" "),
            output.status,
            stderr.lines().last().unwrap_or_default()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout
        .lines()
        .last()
        .ok_or_else(|| err_wrap("nix produced no out paths".into()))?
        .trim()
        .to_owned();
    if path.is_empty() {
        return Err(err_wrap("empty store path from nix".into()));
    }
    Ok(StorePath(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attic_transport_holds_cache_name() {
        let t = AtticTransport::new("quero.lol");
        assert_eq!(t.cache, "quero.lol");
    }

    #[test]
    fn ssh_scheme_normalized_to_ng() {
        let t = SshRemoteTransport::new("ssh://builder.quero.lol");
        assert_eq!(t.nix_builder_uri(), "ssh-ng://builder.quero.lol");
    }

    #[test]
    fn ssh_ng_scheme_passes_through() {
        let t = SshRemoteTransport::new("ssh-ng://builder.quero.lol");
        assert_eq!(t.nix_builder_uri(), "ssh-ng://builder.quero.lol");
    }

    #[test]
    fn attic_rejects_flake_ref() {
        let t = AtticTransport::new("quero.lol");
        // Even without attic on PATH, the match arm orders so a Flake
        // ref never reaches the shell-out.
        let r = BuildRef::Flake {
            url: "github:x/y".into(),
            attr: "a".into(),
        };
        let err = t.fetch(&r).expect_err("flake via attic must fail");
        let msg = format!("{err:?}");
        // Either "only satisfies StorePath" or "not on PATH" — both are
        // "attic can't help with a flake here".
        assert!(
            msg.contains("StorePath") || msg.contains("PATH"),
            "got {msg}"
        );
    }

    #[test]
    fn store_path_already_in_store_is_returned_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("fake-store-entry");
        std::fs::write(&path, b"fake").unwrap();
        let r = BuildRef::StorePath(path.display().to_string());

        let t = LocalTransport;
        let out = t.fetch(&r).expect("local fetch of existing path");
        assert_eq!(out.0, path.display().to_string());
    }
}
