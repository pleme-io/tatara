//! # tatara-os — our NixOS, expressed in Rust + tatara-lisp
//!
//! A full Linux operating system authored as a single `(defsystem …)` form.
//! Runs on the Linux kernel; reuses nixpkgs for userspace today (via
//! `tatara-pkgs::NixpkgsBridge`); transliterates to pure tatara-lisp over
//! time.
//!
//! ## Type surface
//!
//! ```text
//! SystemConfig
//!  ├── hostname, system
//!  ├── kernel        : KernelSpec      (bridged by default → linuxPackages.kernel)
//!  ├── bootloader    : BootloaderSpec  (Grub | SystemdBoot | Uboot | None)
//!  ├── init          : InitSystem      (Systemd | S6 | OpenRC)
//!  ├── services      : Vec<ServiceSpec>
//!  ├── users         : Vec<UserSpec>
//!  ├── filesystems   : Vec<FilesystemSpec>
//!  └── environment   : EnvSpec         (etc files, path, locale, timezone)
//! ```
//!
//! ## Synthesis
//!
//! `SystemSynthesizer` (impl of `tatara_nix::Synthesizer`) takes a
//! `SystemConfig`, walks it, and produces a `SystemClosure`:
//!
//! 1. The kernel derivation
//! 2. An `/etc` derivation (hostname file, os-release, passwd, group)
//! 3. A systemd-unit derivation per service
//! 4. A bootloader-config derivation
//! 5. An **activation script** — the shell script `switch-to-configuration`
//!    calls to move the running system to the configured state.
//! 6. A top-level **profile** derivation that composes all of the above into
//!    one content-addressed root.
//!
//! ## Lisp authoring
//!
//! ```lisp
//! (defsystem my-host
//!   :hostname   "plex"
//!   :system     "x86_64-linux"
//!   :kernel     (:bridge "linuxPackages.kernel")
//!   :bootloader (:kind SystemdBoot :device "/boot")
//!   :init       Systemd
//!   :services   ((:name "nginx"  :exec "nginx -g 'daemon off;'" :enable #t)
//!                (:name "fumi"   :exec "/run/current-system/sw/bin/fumi" :enable #t))
//!   :users      ((:name "drzzln" :uid 1000 :home "/home/drzzln" :shell "/run/current-system/sw/bin/zsh"))
//!   :filesystems (((:mount "/"   :device "/dev/sda2" :fs-type "ext4"))
//!                 ((:mount "/boot" :device "/dev/sda1" :fs-type "vfat")))
//!   :environment (:timezone "America/Sao_Paulo" :locale "en_US.UTF-8"))
//! ```

pub mod activation;
pub mod closure;
pub mod config;
pub mod synth;

pub use activation::ActivationScript;
pub use closure::{SystemClosure, SystemClosureError};
pub use config::{
    BootloaderKind, BootloaderSpec, EnvSpec, FilesystemSpec, InitSystem, KernelSpec, ServiceSpec,
    SystemConfig, UserSpec,
};
pub use synth::SystemSynthesizer;
