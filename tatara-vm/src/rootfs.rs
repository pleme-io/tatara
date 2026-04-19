//! Initrd builder — assembles tatara-init + config + kernel modules into a
//! bootable cpio-gzip archive that Linux can use as its rootfs.
//!
//! The output derivation uses `runCommand` from nixpkgs so cpio + gzip come
//! from the store (no host-package dependency). Three inputs:
//!
//!   - `init_binary` — /nix/store path to a `bin/tatara-init` executable.
//!     When realizing with `NixStoreRealizer`, pass the path returned from
//!     realizing `tatara.packages.${system}.init`.
//!   - `init_config` — the `init.lisp` contents; placed at
//!     `/etc/tatara/init.lisp` inside the archive.
//!   - `extra_files` — arbitrary `(path, content)` pairs.
//!
//! The archive also:
//!   - symlinks `/sbin/init` → `/bin/tatara-init` (kernel-default init path)
//!   - creates `/dev`, `/proc`, `/sys`, `/run`, `/tmp` mount points
//!   - includes `busybox` from nixpkgs at `/bin/busybox` plus common applets
//!     via symlink, so the guest has `sh`, `mount`, `mkdir`, etc. available
//!     before tatara-init starts its services
//!
//! Boot sequence:
//!   1. vfkit → kernel + initrd
//!   2. Linux decompresses the initrd into a tmpfs rootfs
//!   3. Kernel execs `/sbin/init` → tatara-init (PID 1)
//!   4. tatara-init reads `/etc/tatara/init.lisp`, spawns declared services
//!   5. Guest is running — tatara is init, userspace, everything.

use tatara_nix::derivation::{Derivation, Outputs, Source};
use tatara_os::SshdSpec;

/// One file that should land in the initrd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitrdFile {
    /// Absolute path inside the guest (e.g., `/etc/hosts`).
    pub path: String,
    /// Inline content or a `/nix/store` source path to copy.
    pub content: InitrdContent,
    /// POSIX mode bits (default 0644, or 0755 for Executable).
    pub mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitrdContent {
    Inline(String),
    StorePath(String),
}

/// One nixpkgs-attr closure to bake into the initrd. Each package's full
/// runtime closure is copied into `root/nix/store/` and its `bin/*` entries
/// (or the subset in `bin_names`, if non-empty) get symlinked into
/// `/bin/` — so `/bin/htop`, `/bin/curl`, etc. just work inside the guest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestPackage {
    /// `pkgs.<attr_path>` — e.g. `"htop"`, `"curl"`, `"python3"`.
    pub attr_path: String,
    /// When empty, all `bin/*` entries are linked. Populate to whitelist.
    pub bin_names: Vec<String>,
}

impl GuestPackage {
    pub fn new(attr_path: impl Into<String>) -> Self {
        Self {
            attr_path: attr_path.into(),
            bin_names: vec![],
        }
    }
}

/// The full recipe for a bootable initrd.
pub struct LinuxRootfs {
    pub init_binary: String,
    pub init_config: String,
    pub extra_files: Vec<InitrdFile>,
    /// Bridge the busybox binary into the guest. Default: `busybox` from nixpkgs.
    pub busybox: Option<String>,
    /// Optional sshd setup. When `Some`, the emitted `runCommand` pulls the
    /// full `pkgs.openssh` closure into `root/nix/store/`, symlinks
    /// `/bin/sshd` + `/bin/ssh-keygen`, generates a host key at build time,
    /// and writes `/etc/ssh/{sshd_config, authorized_keys}`. The caller is
    /// responsible for adding the sshd service to `init_config`.
    pub sshd: Option<SshdSpec>,
    /// Userspace packages to install in the guest. Each package's closure
    /// is copied into `root/nix/store/`; bin entries land in `/bin/`.
    pub packages: Vec<GuestPackage>,
    /// Name baked into the output derivation.
    pub name: String,
}

impl Default for LinuxRootfs {
    fn default() -> Self {
        Self {
            init_binary: String::new(),
            init_config: String::new(),
            extra_files: vec![],
            busybox: Some("busybox".into()),
            sshd: None,
            packages: vec![],
            name: "tatara-rootfs".into(),
        }
    }
}

impl LinuxRootfs {
    pub fn new(init_binary: impl Into<String>, init_config: impl Into<String>) -> Self {
        Self {
            init_binary: init_binary.into(),
            init_config: init_config.into(),
            ..Default::default()
        }
    }

    pub fn with_name(mut self, n: impl Into<String>) -> Self {
        self.name = n.into();
        self
    }

    pub fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.extra_files.push(InitrdFile {
            path: path.into(),
            content: InitrdContent::Inline(content.into()),
            mode: 0o644,
        });
        self
    }

    pub fn with_file_from_store(
        mut self,
        path: impl Into<String>,
        store_path: impl Into<String>,
    ) -> Self {
        self.extra_files.push(InitrdFile {
            path: path.into(),
            content: InitrdContent::StorePath(store_path.into()),
            mode: 0o644,
        });
        self
    }

    pub fn without_busybox(mut self) -> Self {
        self.busybox = None;
        self
    }

    /// Bake openssh + sshd_config + authorized_keys + a generated ed25519
    /// host key into the initrd.
    pub fn with_sshd(mut self, spec: SshdSpec) -> Self {
        self.sshd = Some(spec);
        self
    }

    /// Add one userspace package. `bin_names` empty means "symlink every
    /// `bin/*` entry into `/bin/`".
    pub fn with_package(mut self, attr_path: impl Into<String>) -> Self {
        self.packages.push(GuestPackage::new(attr_path));
        self
    }

    /// Add many packages at once.
    pub fn with_packages<I, S>(mut self, attrs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for a in attrs {
            self.packages.push(GuestPackage::new(a));
        }
        self
    }

    /// Produce the tatara `Derivation` whose realization is the initrd.cpio.gz.
    pub fn derivation(&self) -> Derivation {
        Derivation {
            name: self.name.clone(),
            version: None,
            inputs: vec![],
            source: Source::default(),
            builder: Default::default(),
            outputs: Outputs::default(),
            env: vec![],
            sandbox: Default::default(),
            bridge: None,
            nix_expr: Some(self.to_nix_expr()),
        }
    }

    /// Emit the full `runCommand` Nix expression.
    pub fn to_nix_expr(&self) -> String {
        let busybox_line = match &self.busybox {
            Some(attr) => format!(
                "  mkdir -p root/bin\n\
                 \x20 cp ${{pkgs.{attr}}}/bin/busybox root/bin/busybox\n\
                 \x20 # Install busybox applet symlinks so sh/mount/mkdir/… work\n\
                 \x20 for app in $(root/bin/busybox --list); do\n\
                 \x20   ln -sf /bin/busybox root/bin/$app\n\
                 \x20 done\n"
            ),
            None => String::new(),
        };

        // Userspace packages — copy each package's closure into the initrd
        // and symlink bin/* into /bin/.
        let (pkg_prelude, pkg_block) = if self.packages.is_empty() {
            (String::new(), String::new())
        } else {
            let mut prelude = String::from("  guestPackages = [\n");
            for p in &self.packages {
                prelude.push_str(&format!("    pkgs.{}\n", p.attr_path));
            }
            prelude.push_str("  ];\n");
            prelude.push_str("  guestClosure = pkgs.closureInfo { rootPaths = guestPackages; };\n");

            let mut block = String::from(
                "  # userspace packages: pull the full closure into root/nix/store\n\
                 \x20 mkdir -p root/nix/store root/bin\n\
                 \x20 while read -r store_path; do\n\
                 \x20   cp -r \"$store_path\" root/nix/store/\n\
                 \x20 done < ${guestClosure}/store-paths\n\
                 \x20 # Symlink each package's bin/* into /bin (best-effort — skip\n\
                 \x20 # packages with no bin/ dir).\n",
            );
            for p in &self.packages {
                if p.bin_names.is_empty() {
                    block.push_str(&format!(
                        "  if [ -d ${{pkgs.{attr}}}/bin ]; then\n\
                         \x20   for bin in ${{pkgs.{attr}}}/bin/*; do\n\
                         \x20     [ -e \"$bin\" ] && ln -sf \"$bin\" root/bin/$(basename \"$bin\")\n\
                         \x20   done\n\
                         \x20 fi\n",
                        attr = p.attr_path,
                    ));
                } else {
                    for name in &p.bin_names {
                        block.push_str(&format!(
                            "  [ -e ${{pkgs.{attr}}}/bin/{name} ] && ln -sf ${{pkgs.{attr}}}/bin/{name} root/bin/{name}\n",
                            attr = p.attr_path,
                            name = name,
                        ));
                    }
                }
            }
            (prelude, block)
        };

        // sshd integration: copy the openssh closure into root/nix/store,
        // symlink bin entries, write sshd_config + authorized_keys, and
        // generate an ed25519 host key at build time.
        let (sshd_prelude, sshd_block) = match &self.sshd {
            Some(s) => {
                let auth_keys = s.authorized_keys.join("\n");
                let permit_root = if s.permit_root { "yes" } else { "no" };
                let pass_auth = if s.password_authentication {
                    "yes"
                } else {
                    "no"
                };
                let cfg = format!(
                    "Port {port}\n\
                     HostKey /etc/ssh/ssh_host_ed25519_key\n\
                     PermitRootLogin {permit_root}\n\
                     PasswordAuthentication {pass_auth}\n\
                     PubkeyAuthentication yes\n\
                     AuthorizedKeysFile /etc/ssh/authorized_keys\n\
                     StrictModes no\n\
                     UsePAM no\n\
                     Subsystem sftp internal-sftp\n",
                    port = s.port,
                    permit_root = permit_root,
                    pass_auth = pass_auth,
                );
                // ── prelude injected before runCommand's args attrs ─────
                let prelude = "  openssh = pkgs.openssh;\n\
                               \x20 opensshClosure = pkgs.closureInfo { rootPaths = [ pkgs.openssh ]; };\n";
                // ── build-time commands (run inside runCommand) ─────────
                let block = format!(
                    "  # openssh: bring the closure into root/nix/store\n\
                     \x20 mkdir -p root/nix/store root/bin root/etc/ssh root/var/empty\n\
                     \x20 while read -r store_path; do\n\
                     \x20   cp -r \"$store_path\" root/nix/store/\n\
                     \x20 done < ${{opensshClosure}}/store-paths\n\
                     \x20 ln -sf ${{openssh}}/bin/sshd root/bin/sshd\n\
                     \x20 ln -sf ${{openssh}}/bin/ssh-keygen root/bin/ssh-keygen\n\
                     \x20 cat > root/etc/ssh/sshd_config <<'TATARA_SSHD_CFG_EOF'\n\
                     {cfg}TATARA_SSHD_CFG_EOF\n\
                     \x20 cat > root/etc/ssh/authorized_keys <<'TATARA_AUTH_KEYS_EOF'\n\
                     {auth_keys}\n\
                     TATARA_AUTH_KEYS_EOF\n\
                     \x20 chmod 0600 root/etc/ssh/authorized_keys\n\
                     \x20 # Deterministic(-ish) host key: generate at build.\n\
                     \x20 ${{openssh}}/bin/ssh-keygen -t ed25519 -N '' \\\n\
                     \x20   -f root/etc/ssh/ssh_host_ed25519_key \\\n\
                     \x20   -C \"tatara-os-{name}\"\n\
                     \x20 chmod 0600 root/etc/ssh/ssh_host_ed25519_key\n",
                    cfg = cfg,
                    auth_keys = auth_keys,
                    name = self.name,
                );
                (prelude, block)
            }
            None => ("", String::new()),
        };

        let mut file_cmds = String::new();
        for f in &self.extra_files {
            // Ensure the target directory exists, then write the file.
            let dir = match f.path.rsplit_once('/') {
                Some((d, _)) if !d.is_empty() => d.to_string(),
                _ => "".to_string(),
            };
            if !dir.is_empty() {
                file_cmds.push_str(&format!("  mkdir -p root{}\n", nix_path_escape(&dir)));
            }
            match &f.content {
                InitrdContent::Inline(body) => {
                    // The closing sentinel MUST sit on its own line, so
                    // guarantee a trailing newline before we emit it.
                    let body_nl = if body.ends_with('\n') {
                        body.clone()
                    } else {
                        format!("{body}\n")
                    };
                    file_cmds.push_str(&format!(
                        "  cat > root{} <<'TATARA_ROOTFS_EOF'\n{body_nl}TATARA_ROOTFS_EOF\n",
                        nix_path_escape(&f.path)
                    ));
                }
                InitrdContent::StorePath(sp) => {
                    file_cmds.push_str(&format!("  cp {sp} root{}\n", nix_path_escape(&f.path)));
                }
            }
            let mode = f.mode;
            file_cmds.push_str(&format!(
                "  chmod {mode:o} root{}\n",
                nix_path_escape(&f.path)
            ));
        }

        let init_config_escaped = {
            let s = heredoc_escape(&self.init_config);
            if s.ends_with('\n') {
                s
            } else {
                format!("{s}\n")
            }
        };

        format!(
            r#"let
  pkgs = import <nixpkgs> {{}};
{pkg_prelude}{sshd_prelude}in
pkgs.runCommand "{name}" {{
  buildInputs = [ pkgs.cpio pkgs.gzip pkgs.coreutils pkgs.findutils ];
}} ''
  mkdir -p $out
  mkdir -p root/bin root/sbin root/etc/tatara root/proc root/sys root/dev root/run root/tmp
  # tatara-init — the PID 1 supervisor
  cp {init_binary} root/bin/tatara-init
  chmod 0755 root/bin/tatara-init
  # Linux looks for /init at initramfs root before honoring kernel cmdline
  # `init=…`. Symlink both so either path works.
  ln -sf /bin/tatara-init root/init
  ln -sf /bin/tatara-init root/sbin/init
  # init.lisp — the service manifest
  cat > root/etc/tatara/init.lisp <<'TATARA_INIT_LISP_EOF'
{init_config_escaped}TATARA_INIT_LISP_EOF
  chmod 0644 root/etc/tatara/init.lisp
{busybox_line}{pkg_block}{sshd_block}{file_cmds}  # cpio + gzip into initrd
  ( cd root && find . -print0 | cpio -o -0 --format=newc ) | gzip -9 > $out/initrd.cpio.gz
  # Emit the top-level filesystem tree too, for anyone who wants ext4 later.
  cp -r root $out/rootfs
''"#,
            name = self.name,
            init_binary = self.init_binary,
            init_config_escaped = init_config_escaped,
            busybox_line = busybox_line,
            pkg_prelude = pkg_prelude,
            pkg_block = pkg_block,
            sshd_block = sshd_block,
            sshd_prelude = sshd_prelude,
            file_cmds = file_cmds,
        )
    }
}

fn nix_path_escape(s: &str) -> String {
    // Very conservative: pass through, assume valid POSIX paths.
    // Single quotes inside would break our heredoc; this is init-path code,
    // those cases aren't expected.
    s.to_string()
}

fn heredoc_escape(s: &str) -> String {
    // Ensure the sentinel doesn't appear in the content.
    if s.contains("TATARA_INIT_LISP_EOF") {
        s.replace("TATARA_INIT_LISP_EOF", "TATARA_INIT_LISP_ESC_EOF")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_rootfs_emits_expected_shape() {
        let r = LinuxRootfs::new(
            "/nix/store/xxx-tatara-init/bin/tatara-init",
            "(definit :name \"plex\")",
        );
        let d = r.derivation();
        assert_eq!(d.name, "tatara-rootfs");
        let expr = d.nix_expr.as_ref().unwrap();
        assert!(expr.contains("pkgs.cpio"));
        assert!(expr.contains("pkgs.gzip"));
        assert!(expr.contains("cp /nix/store/xxx-tatara-init/bin/tatara-init root/bin/tatara-init"));
        assert!(expr.contains("ln -sf /bin/tatara-init root/sbin/init"));
        assert!(expr.contains("cpio -o -0 --format=newc"));
        assert!(expr.contains("gzip -9 > $out/initrd.cpio.gz"));
        assert!(expr.contains("(definit :name \"plex\")"));
    }

    #[test]
    fn busybox_applets_get_symlinked_by_default() {
        let r = LinuxRootfs::new("/nix/store/x/bin/tatara-init", "");
        let expr = r.derivation().nix_expr.unwrap();
        assert!(expr.contains("cp ${pkgs.busybox}/bin/busybox root/bin/busybox"));
        assert!(expr.contains("for app in $(root/bin/busybox --list)"));
    }

    #[test]
    fn without_busybox_drops_applet_installation() {
        let r = LinuxRootfs::new("/nix/store/x/bin/tatara-init", "").without_busybox();
        let expr = r.derivation().nix_expr.unwrap();
        assert!(!expr.contains("busybox"));
    }

    #[test]
    fn extra_files_get_heredoc_blocks() {
        let r = LinuxRootfs::new("/nix/store/x/bin/tatara-init", "")
            .with_file("/etc/hosts", "127.0.0.1 localhost\n")
            .with_file("/etc/hostname", "plex-guest\n");
        let expr = r.derivation().nix_expr.unwrap();
        assert!(expr.contains("mkdir -p root/etc"));
        assert!(expr.contains("cat > root/etc/hosts <<'TATARA_ROOTFS_EOF'"));
        assert!(expr.contains("127.0.0.1 localhost"));
        assert!(expr.contains("cat > root/etc/hostname <<'TATARA_ROOTFS_EOF'"));
    }

    #[test]
    fn store_path_files_get_cp_commands() {
        let r = LinuxRootfs::new("/nix/store/x/bin/tatara-init", "").with_file_from_store(
            "/etc/ssl/certs/ca-cert.pem",
            "/nix/store/y-ca-bundle/cert.pem",
        );
        let expr = r.derivation().nix_expr.unwrap();
        assert!(expr.contains("cp /nix/store/y-ca-bundle/cert.pem root/etc/ssl/certs/ca-cert.pem"));
    }

    #[test]
    fn init_config_with_sentinel_is_escaped() {
        let r = LinuxRootfs::new(
            "/nix/store/x/bin/tatara-init",
            "line1\nTATARA_INIT_LISP_EOF\nline3",
        );
        let expr = r.derivation().nix_expr.unwrap();
        // The original sentinel should no longer appear as a standalone token
        // (it's renamed so the heredoc closes correctly).
        assert!(expr.contains("TATARA_INIT_LISP_ESC_EOF"));
    }

    #[test]
    fn custom_name_propagates_to_derivation() {
        let r = LinuxRootfs::new("/nix/store/x/bin/tatara-init", "").with_name("plex-guest-initrd");
        let d = r.derivation();
        assert_eq!(d.name, "plex-guest-initrd");
        let expr = d.nix_expr.unwrap();
        assert!(expr.contains(r#"runCommand "plex-guest-initrd""#));
    }
}
