//! Native-arch Darwin guest support.
//!
//! Apple Virtualization.framework can run macOS-in-macOS on Apple Silicon
//! hosts. Unlike Linux guests, there is no kernel/initrd to hand-assemble;
//! the guest boots via:
//!
//!   1. **IPSW** — Apple restore image (`.ipsw`). Contains the kernel, the
//!      root filesystem, and boot assets. On first boot, vfkit uses
//!      `--restore-image=<ipsw>` to install macOS onto a disk image.
//!   2. **Auxiliary storage** — a small file (typically 64 MB) that holds
//!      boot state the guest writes between runs.
//!   3. **Root disk image** — raw disk file, sized for the guest's workload.
//!   4. **Machine identifier** — 16-byte blob exposed to the guest as its
//!      hardware identity (serial-number-shaped). Stable across boots.
//!   5. **Hardware model** — Apple-signed blob describing the emulated
//!      machine class (Mac Studio M1 Max, etc.). Pulled from the IPSW.
//!
//! The output of `DarwinRootfs::derivation()` is a Nix derivation that
//! produces a directory containing:
//!
//!   - `disk.img`       — zeroed raw image, `disk_size_gib` GiB
//!   - `aux.img`        — empty auxiliary storage (64 MiB)
//!   - `machine-id.bin` — 16 random bytes (caller may override)
//!   - `restore.ipsw`   — symlink to the caller-provided IPSW path
//!   - `README.md`      — boot instructions, per-build
//!
//! The hardware-model blob is NOT emitted by us — it's lifted from the
//! IPSW at runtime by vfkit / kasou. We don't re-implement that.
//!
//! ## Where this fits
//!
//! - Input: `(defvm :hypervisor (:kind "VfkitDarwin") :kernel (:kind
//!   "DarwinIpsw" :ipsw_path "/path/to/macos.ipsw") …)`
//! - Output: a tatara `Derivation` whose nix_expr realizes the bundle above.
//! - Boot: vfkit --config vm-darwin.json first-boots the IPSW installer,
//!   subsequent boots just run the installed disk.

use tatara_nix::derivation::{Derivation, Outputs, Source};

/// Recipe for a native-arch Darwin guest bundle.
pub struct DarwinRootfs {
    /// Path to the Apple IPSW restore image. Caller-owned; may be a
    /// `/nix/store/...` path (bridged via `nixpkgs.macos-ipsw` when
    /// available), a local file, or a `${…}/restore.ipsw` Nix antiquotation
    /// — we pass it through verbatim into the emitted expression.
    pub ipsw_path: String,
    /// Root disk size in GiB. Default 64.
    pub disk_size_gib: u32,
    /// 16-byte machine identifier, base64-encoded. When `None`, we generate
    /// 16 random bytes at build time via `head -c 16 /dev/urandom`.
    pub machine_identifier_b64: Option<String>,
    /// Name baked into the output derivation.
    pub name: String,
}

impl Default for DarwinRootfs {
    fn default() -> Self {
        Self {
            ipsw_path: "/path/to/macos.ipsw".into(),
            disk_size_gib: 64,
            machine_identifier_b64: None,
            name: "tatara-darwin-rootfs".into(),
        }
    }
}

impl DarwinRootfs {
    pub fn new(ipsw_path: impl Into<String>) -> Self {
        Self {
            ipsw_path: ipsw_path.into(),
            ..Default::default()
        }
    }

    pub fn with_name(mut self, n: impl Into<String>) -> Self {
        self.name = n.into();
        self
    }

    pub fn with_disk_size_gib(mut self, n: u32) -> Self {
        self.disk_size_gib = n;
        self
    }

    pub fn with_machine_identifier_b64(mut self, id: impl Into<String>) -> Self {
        self.machine_identifier_b64 = Some(id.into());
        self
    }

    /// Produce the tatara `Derivation` whose realization is the guest bundle.
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

    pub fn to_nix_expr(&self) -> String {
        let id_cmd = match &self.machine_identifier_b64 {
            Some(b64) => format!("printf %s '{b64}' > $out/machine-id.b64\n"),
            None => "head -c 16 /dev/urandom | base64 > $out/machine-id.b64\n".into(),
        };

        format!(
            r#"let pkgs = import <nixpkgs> {{}}; in
pkgs.runCommand "{name}" {{
  buildInputs = [ pkgs.coreutils pkgs.qemu ];
}} ''
  mkdir -p $out
  # Root disk — sparse raw image sized for the guest's workload.
  qemu-img create -f raw "$out/disk.img" {disk_size}G
  # Auxiliary storage — 64 MiB persistent boot state slot.
  qemu-img create -f raw "$out/aux.img" 64M
  # Machine identifier (16 bytes, base64). Regenerated each build unless
  # the caller pinned it via `:machine_identifier_b64` in defvm.
  {id_cmd}
  # Symlink the caller-provided IPSW so the whole bundle is in one place.
  ln -sf {ipsw} "$out/restore.ipsw"
  cat > $out/README.md <<'TATARA_README_EOF'
  # tatara-os Darwin guest — {name}
  #
  # Artifacts:
  #   disk.img         sparse root disk ({disk_size} GiB)
  #   aux.img          64 MiB auxiliary storage
  #   machine-id.b64   16-byte machine identifier (base64)
  #   restore.ipsw     symlink to the caller-provided Apple restore image
  #
  # Boot (first time, installs macOS into disk.img):
  #   vfkit --config vm-darwin.json --restore-image=$out/restore.ipsw
  #
  # Subsequent boots:
  #   vfkit --config vm-darwin.json
  TATARA_README_EOF
''"#,
            name = self.name,
            disk_size = self.disk_size_gib,
            id_cmd = id_cmd,
            ipsw = self.ipsw_path,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_darwin_rootfs_expr_shape() {
        let r = DarwinRootfs::new("/nix/store/xxx-macos-14.ipsw");
        let expr = r.derivation().nix_expr.unwrap();
        assert!(expr.contains("qemu-img create -f raw \"$out/disk.img\" 64G"));
        assert!(expr.contains("qemu-img create -f raw \"$out/aux.img\" 64M"));
        assert!(expr.contains("ln -sf /nix/store/xxx-macos-14.ipsw \"$out/restore.ipsw\""));
        // default: generated identifier
        assert!(expr.contains("head -c 16 /dev/urandom"));
    }

    #[test]
    fn disk_size_override_flows_into_expr() {
        let r = DarwinRootfs::new("/x.ipsw").with_disk_size_gib(256);
        let expr = r.derivation().nix_expr.unwrap();
        assert!(expr.contains("\"$out/disk.img\" 256G"));
    }

    #[test]
    fn pinned_machine_identifier_is_written_literally() {
        let r = DarwinRootfs::new("/x.ipsw")
            .with_machine_identifier_b64("YWJjMTIzNDU2Nzg5MGFiYw==");
        let expr = r.derivation().nix_expr.unwrap();
        assert!(expr.contains("printf %s 'YWJjMTIzNDU2Nzg5MGFiYw=='"));
        assert!(!expr.contains("/dev/urandom"));
    }

    #[test]
    fn custom_name_propagates_to_derivation() {
        let r = DarwinRootfs::new("/x.ipsw").with_name("plex-darwin-guest");
        let d = r.derivation();
        assert_eq!(d.name, "plex-darwin-guest");
        assert!(d.nix_expr.unwrap().contains(r#"runCommand "plex-darwin-guest""#));
    }
}
