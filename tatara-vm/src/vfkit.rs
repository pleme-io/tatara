//! `vfkit` config emitter.
//!
//! vfkit is a minimal CLI over Apple Virtualization.framework. It reads a JSON
//! config (via `--config`) describing CPUs, memory, kernel, initrd, devices.
//! We emit that JSON from a typed `VmSpec` so the whole guest definition
//! survives round-trip from tatara-lisp to a bootable VM.
//!
//! This emitter is Darwin-friendly but the Rust build is host-agnostic — we
//! don't link against Virtualization.framework directly. vfkit does.

use serde::{Deserialize, Serialize};

use tatara_nix::{Artifact, MultiSynthesizer, Synthesizer};

use crate::config::{GuestKernel, GuestRootfs, Hypervisor, NetworkKind, VmSpec};

/// JSON shape vfkit expects. Keep it narrow — vfkit has many optional fields
/// we don't use yet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct VfkitJson {
    pub cpus: u32,
    pub memory_mib: u32,
    pub kernel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initrd: Option<String>,
    pub cmdline: String,
    pub devices: Vec<VfkitDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "device", rename_all = "kebab-case")]
pub enum VfkitDevice {
    VirtioBlk {
        image: String,
    },
    VirtioNet {
        mode: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subnet: Option<String>,
        /// Stable MAC address (form `xx:xx:xx:xx:xx:xx`). Derived from the
        /// guest hostname by default so `arp -a` can find the guest between
        /// boots without racing DHCP.
        #[serde(skip_serializing_if = "Option::is_none", rename = "mac-address")]
        mac_address: Option<String>,
    },
    VirtioFs {
        host: String,
        guest: String,
        read_only: bool,
    },
    VirtioConsole,
    VirtioRng,
}

/// JSON shape for native-arch Darwin guests (Apple Virtualization.framework
/// macOS mode). Completely different top-level from the Linux path — Darwin
/// boots via a bootloader descriptor pointing at an IPSW + auxiliary image,
/// not a kernel + initrd pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct VfkitDarwinJson {
    pub cpus: u32,
    pub memory_mib: u32,
    pub bootloader: VfkitDarwinBootloader,
    pub devices: Vec<VfkitDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct VfkitDarwinBootloader {
    /// Always `"macos"` today; reserved for future Darwin variants.
    #[serde(rename = "type")]
    pub kind: String,
    /// Filesystem path (or placeholder) to the Apple restore image (`.ipsw`).
    /// Used on first boot to install macOS into the root disk.
    pub restore_image: String,
    /// 16-byte machine identifier, base64 (unique per guest, persisted).
    pub machine_identifier: String,
    /// Auxiliary storage image path — 64 MiB persistent boot state slot.
    pub auxiliary_storage: String,
}

/// Emits a `VfkitJson` from a `VmSpec`. Resolves bridged paths *lazily*: a
/// caller who realizes the kernel/rootfs derivations separately can fill in
/// the resulting paths via `with_kernel_path` / `with_rootfs_path`.
pub struct VfkitEmitter {
    pub kernel_path: Option<String>,
    pub rootfs_path: Option<String>,
    pub initrd_path: Option<String>,
    // Darwin-guest extras — ignored for Linux guests.
    pub darwin_aux_path: Option<String>,
    pub darwin_machine_identifier_b64: Option<String>,
}

impl Default for VfkitEmitter {
    fn default() -> Self {
        Self {
            kernel_path: None,
            rootfs_path: None,
            initrd_path: None,
            darwin_aux_path: None,
            darwin_machine_identifier_b64: None,
        }
    }
}

impl VfkitEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_kernel_path(mut self, p: impl Into<String>) -> Self {
        self.kernel_path = Some(p.into());
        self
    }

    pub fn with_rootfs_path(mut self, p: impl Into<String>) -> Self {
        self.rootfs_path = Some(p.into());
        self
    }

    pub fn with_initrd_path(mut self, p: impl Into<String>) -> Self {
        self.initrd_path = Some(p.into());
        self
    }

    pub fn with_darwin_aux_path(mut self, p: impl Into<String>) -> Self {
        self.darwin_aux_path = Some(p.into());
        self
    }

    pub fn with_darwin_machine_identifier(mut self, b64: impl Into<String>) -> Self {
        self.darwin_machine_identifier_b64 = Some(b64.into());
        self
    }

    /// Render the Darwin guest shape. Only meaningful when
    /// `vm.hypervisor == Hypervisor::VfkitDarwin` and
    /// `vm.kernel == GuestKernel::DarwinIpsw { .. }`.
    pub fn synthesize_darwin(&self, vm: &VmSpec) -> VfkitDarwinJson {
        let ipsw = match &vm.kernel {
            GuestKernel::DarwinIpsw { ipsw_path } => ipsw_path.clone(),
            _ => self
                .kernel_path
                .clone()
                .unwrap_or_else(|| "<unset-ipsw>".into()),
        };
        let aux = self
            .darwin_aux_path
            .clone()
            .unwrap_or_else(|| "<bundle>/aux.img".into());
        let machine_id = self
            .darwin_machine_identifier_b64
            .clone()
            .unwrap_or_else(|| "<bundle>/machine-id.b64".into());
        let rootfs = self
            .rootfs_path
            .clone()
            .unwrap_or_else(|| "<bundle>/disk.img".into());

        let mut devs = vec![VfkitDevice::VirtioBlk { image: rootfs }];
        if !matches!(vm.network.kind, NetworkKind::None) {
            devs.push(VfkitDevice::VirtioNet {
                mode: match vm.network.kind {
                    NetworkKind::Nat => "nat".into(),
                    NetworkKind::Bridge => "bridge".into(),
                    NetworkKind::None => unreachable!(),
                },
                subnet: vm.network.subnet.clone(),
                mac_address: Some(deterministic_mac(&vm.name)),
            });
        }
        for s in &vm.shares {
            devs.push(VfkitDevice::VirtioFs {
                host: s.host.clone(),
                guest: s.guest.clone(),
                read_only: s.read_only,
            });
        }
        devs.push(VfkitDevice::VirtioConsole);
        devs.push(VfkitDevice::VirtioRng);

        VfkitDarwinJson {
            cpus: vm.cpus,
            memory_mib: vm.memory_mib,
            bootloader: VfkitDarwinBootloader {
                kind: "macos".into(),
                restore_image: ipsw,
                machine_identifier: machine_id,
                auxiliary_storage: aux,
            },
            devices: devs,
        }
    }

    /// Pretty-printed JSON for the Darwin guest shape.
    pub fn render_darwin(&self, vm: &VmSpec) -> String {
        serde_json::to_string_pretty(&self.synthesize_darwin(vm)).unwrap_or_default()
    }

    fn kernel_placeholder(vm: &VmSpec) -> String {
        match &vm.kernel {
            GuestKernel::Bridge { attr_path } => {
                format!("<bridge:{attr_path}/bzImage>")
            }
            GuestKernel::Custom { derivation } => {
                format!("<custom:{}/bzImage>", derivation.name)
            }
            // Darwin guests don't use `bzImage`; the IPSW is the kernel blob.
            // Emit the path so boot.sh can drive the Darwin-guest flavor.
            GuestKernel::DarwinIpsw { ipsw_path } => ipsw_path.clone(),
        }
    }

    fn rootfs_placeholder(vm: &VmSpec) -> String {
        match &vm.rootfs {
            GuestRootfs::System { name } => format!("<system:{name}/rootfs.img>"),
            GuestRootfs::Image { derivation } => {
                format!("<image:{}/rootfs.img>", derivation.name)
            }
            GuestRootfs::Bridge { attr_path } => {
                format!("<bridge:{attr_path}/rootfs.img>")
            }
        }
    }

    fn emit_devices(&self, vm: &VmSpec) -> Vec<VfkitDevice> {
        let mut devs = Vec::new();
        // Root disk
        let rootfs = self
            .rootfs_path
            .clone()
            .unwrap_or_else(|| Self::rootfs_placeholder(vm));
        devs.push(VfkitDevice::VirtioBlk { image: rootfs });
        // Network
        if !matches!(vm.network.kind, NetworkKind::None) {
            devs.push(VfkitDevice::VirtioNet {
                mode: match vm.network.kind {
                    NetworkKind::Nat => "nat".into(),
                    NetworkKind::Bridge => "bridge".into(),
                    NetworkKind::None => unreachable!(),
                },
                subnet: vm.network.subnet.clone(),
                mac_address: Some(deterministic_mac(&vm.name)),
            });
        }
        // Shared folders via virtiofs
        for s in &vm.shares {
            devs.push(VfkitDevice::VirtioFs {
                host: s.host.clone(),
                guest: s.guest.clone(),
                read_only: s.read_only,
            });
        }
        // Console + RNG — essential for a usable Linux guest.
        devs.push(VfkitDevice::VirtioConsole);
        devs.push(VfkitDevice::VirtioRng);
        devs
    }
}

/// Derive a stable MAC address from the VM name via BLAKE3. First byte is
/// masked `0x02` (locally-administered, unicast) to stay out of the IANA
/// space; `arp -a` then shows the guest reliably across boots.
fn deterministic_mac(name: &str) -> String {
    let h = blake3::hash(name.as_bytes());
    let b = h.as_bytes();
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        (b[0] & 0xfe) | 0x02,
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
    )
}

impl Synthesizer for VfkitEmitter {
    type Input = VmSpec;
    type Ast = VfkitJson;
    type Output = String;

    fn synthesize(&self, vm: &VmSpec) -> VfkitJson {
        let kernel = self
            .kernel_path
            .clone()
            .unwrap_or_else(|| Self::kernel_placeholder(vm));
        let initrd = self.initrd_path.clone().or_else(|| {
            vm.initrd
                .as_ref()
                .map(|d| format!("<initrd:{}/initrd>", d.name))
        });
        let cmdline = vm.cmdline.join(" ");
        let devices = self.emit_devices(vm);
        VfkitJson {
            cpus: vm.cpus,
            memory_mib: vm.memory_mib,
            kernel,
            initrd,
            cmdline,
            devices,
        }
    }

    fn render(&self, ast: &VfkitJson) -> String {
        serde_json::to_string_pretty(ast).unwrap_or_default()
    }
}

/// Multi-file emission for a full `defvm`: writes `vm.json` + `boot.sh`
/// helper that drives vfkit with the right flags. Auto-selects the Linux
/// vs Darwin JSON shape based on `vm.hypervisor`.
impl MultiSynthesizer for VfkitEmitter {
    type Input = VmSpec;

    fn generate_all(&self, vm: &VmSpec) -> Vec<Artifact> {
        let prefix = format!("vm/{}", vm.name);
        match vm.hypervisor {
            Hypervisor::Vfkit => {
                let json = Synthesizer::generate(self, vm);
                let boot_sh = format!(
                    "#!/bin/sh\n# tatara-vm boot helper for '{name}' (Linux guest)\nset -eu\nexec vfkit --config \"$(dirname \"$0\")/vm.json\" \"$@\"\n",
                    name = vm.name,
                );
                vec![
                    Artifact::new(format!("{prefix}/vm.json"), json),
                    Artifact::new(format!("{prefix}/boot.sh"), boot_sh),
                ]
            }
            Hypervisor::VfkitDarwin => {
                let json = self.render_darwin(vm);
                let boot_sh = format!(
                    "#!/bin/sh\n# tatara-vm boot helper for '{name}' (native Darwin guest)\nset -eu\nexec vfkit --config \"$(dirname \"$0\")/vm-darwin.json\" \"$@\"\n",
                    name = vm.name,
                );
                vec![
                    Artifact::new(format!("{prefix}/vm-darwin.json"), json),
                    Artifact::new(format!("{prefix}/boot.sh"), boot_sh),
                ]
            }
            _ => vec![Artifact::new(
                "ERROR.txt".to_string(),
                format!(
                    "VfkitEmitter only renders Hypervisor::Vfkit / VfkitDarwin; got {:?}",
                    vm.hypervisor
                ),
            )],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> VmSpec {
        let mut v = VmSpec::plex_default("plex-guest");
        v.cpus = 4;
        v.memory_mib = 4096;
        v.shares = vec![crate::config::ShareSpec {
            host: "/Users/drzzln/code".into(),
            guest: "/mnt/code".into(),
            read_only: false,
        }];
        v
    }

    #[test]
    fn synthesize_maps_core_fields() {
        let emitter = VfkitEmitter::new();
        let json = emitter.synthesize(&vm());
        assert_eq!(json.cpus, 4);
        assert_eq!(json.memory_mib, 4096);
        assert!(json.cmdline.contains("init=/bin/tatara-init"));
        // devices include block + net + virtiofs + console + rng
        assert_eq!(json.devices.len(), 5);
    }

    #[test]
    fn placeholders_reflect_bridge_attrs() {
        let emitter = VfkitEmitter::new();
        let json = emitter.synthesize(&vm());
        assert!(json.kernel.contains("linuxPackages.kernel"));
        match &json.devices[0] {
            VfkitDevice::VirtioBlk { image } => {
                assert!(image.contains("minimal-rootfs"));
            }
            _ => panic!("expected VirtioBlk first"),
        }
    }

    #[test]
    fn realized_paths_replace_placeholders() {
        let emitter = VfkitEmitter::new()
            .with_kernel_path("/nix/store/xxx-kernel/bzImage")
            .with_rootfs_path("/nix/store/yyy-rootfs/rootfs.img");
        let json = emitter.synthesize(&vm());
        assert_eq!(json.kernel, "/nix/store/xxx-kernel/bzImage");
        match &json.devices[0] {
            VfkitDevice::VirtioBlk { image } => {
                assert_eq!(image, "/nix/store/yyy-rootfs/rootfs.img");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn render_emits_pretty_json() {
        let emitter = VfkitEmitter::new();
        let output = emitter.generate(&vm());
        assert!(output.contains("\"cpus\":"));
        assert!(output.contains("\"memory-mib\":"));
    }

    #[test]
    fn multi_synth_emits_vm_json_and_boot_sh() {
        let emitter = VfkitEmitter::new();
        let arts = emitter.generate_all(&vm());
        assert_eq!(arts.len(), 2);
        let paths: Vec<_> = arts.iter().map(|a| a.path.as_str()).collect();
        assert!(paths.contains(&"vm/plex-guest/vm.json"));
        assert!(paths.contains(&"vm/plex-guest/boot.sh"));
        let boot = arts.iter().find(|a| a.path.ends_with("boot.sh")).unwrap();
        assert!(boot.content.starts_with("#!/bin/sh"));
        assert!(boot.content.contains("exec vfkit --config"));
    }

    #[test]
    fn non_vfkit_hypervisor_produces_explicit_error() {
        let mut v = vm();
        v.hypervisor = Hypervisor::Qemu;
        let emitter = VfkitEmitter::new();
        let arts = emitter.generate_all(&v);
        assert_eq!(arts[0].path, "ERROR.txt");
        assert!(arts[0].content.contains("Qemu"));
    }
}
