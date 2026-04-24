//! # tatara-init — tatara IS PID 1
//!
//! Drop-in replacement for systemd/s6/openrc inside a tatara-os Linux guest.
//! No daemon, no dbus, no cgroup choreography (the kernel already does that).
//! A single static Rust binary that:
//!
//!   1. Reads `/etc/tatara/init.lisp` (or `$TATARA_INIT_CONFIG`)
//!   2. Compiles it to a typed `InitConfig` via `tatara-lisp-derive`
//!   3. For each declared `Service`, forks + execs
//!   4. Reaps zombie children (the classic PID-1 duty)
//!   5. Honors `SIGTERM`/`SIGINT` → propagates to children, then exits
//!   6. On `SIGHUP` → re-reads the config, diff + apply (start new,
//!      stop removed, restart changed)
//!
//! The supervisor is abstract (`Supervisor` trait) so the control logic is
//! unit-testable without actually being PID 1. A concrete `LinuxSupervisor`
//! ships for the real boot path; a `MockSupervisor` ships for tests.
//!
//! Lisp authoring:
//!
//! ```lisp
//! (definit
//!   :name "plex-boot"
//!   :reap-zombies #t
//!   :services
//!     ((:name "sshd"  :exec "/run/current-system/sw/bin/sshd -D")
//!      (:name "tatara-reconciler" :exec "/bin/tatara-reconciler")
//!      (:name "kasou" :exec "/bin/kasou" :restart "always"
//!                     :env (("KASOU_SOCKET" "/run/kasou.sock")))))
//! ```

pub mod config;
pub mod mounts;
pub mod supervisor;

pub use config::{InitConfig, MountSpec, RestartPolicy, Service};
pub use mounts::{mount_early_filesystems, mount_extra, EarlyMount, EarlyMountError};
pub use supervisor::{LinuxSupervisor, MockSupervisor, Pid, Supervisor, SupervisorError};
