; ============================================================
;  plex — a minimal tatara-os guest, authored in tatara-lisp
;
;  Run:
;    cargo run -p tatara-vm --bin tatara-boot-gen -- \
;      examples/boot/plex.lisp /tmp/plex-boot/
;
;  Emits: system.json, init.lisp, kernel.nix, initrd.nix,
;         vm.json, boot.sh, README.md under /tmp/plex-boot/.
;
;  To actually boot (on Apple Silicon with vfkit installed):
;    KERNEL=$(nix build -f /tmp/plex-boot/kernel.nix \
;               --no-link --print-out-paths)/bzImage
;    INITRD=$(nix build -f /tmp/plex-boot/initrd.nix \
;               --no-link --print-out-paths)/initrd.cpio.gz
;    jq ".kernel=\"$KERNEL\"|.devices[0].image=\"$INITRD\"" \
;       /tmp/plex-boot/vm.json > /tmp/plex-boot/vm-resolved.json
;    vfkit --config /tmp/plex-boot/vm-resolved.json
; ============================================================

(defsystem
  :hostname "plex"
  :system   "aarch64-linux"
  :services ((:name "greeter"
              :exec "/bin/busybox sh -c 'while true; do echo \"plex says hi\"; sleep 10; done'"
              :enable #t)
             (:name "motd"
              :exec "/bin/busybox sh -c 'echo tatara-os > /etc/motd'"
              :enable #t)))

(defvm
  :name       "plex"
  :cpus       2
  :memory-mib 1024
  :hypervisor (:kind "Vfkit")
  :kernel     (:kind "Bridge" :attr_path "linuxPackages.kernel")
  :rootfs     (:kind "Bridge" :attr_path "placeholder")
  :network    (:kind "Nat")
  :cmdline    ("console=hvc0" "init=/bin/tatara-init"))
