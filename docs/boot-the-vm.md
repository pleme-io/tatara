# Boot the tatara-os VM from `nix run .#rebuild`

This is the recipe for going from a cold nix install on Darwin to a running
tatara-os Linux guest with tatara-init as PID 1, SSH-able from the host.

The HM module lives at `tatara/module/tatara-os-vm.nix`. It exposes
`services.tatara-os-vm` and wires rebuild → boot-artifact-tree → pre-built
`/nix/store` derivations → a `launch.sh` you can exec.

## Prereqs (one-time)

- `linux-builder` must be enabled on the Darwin host so Linux derivations
  (`linuxPackages.kernel`, `busybox`, `openssh`) can cross-build. On
  nix-darwin this is one line:

  ```nix
  nix.linux-builder.enable = true;
  ```

- `vfkit` installed via nixpkgs (the module installs it).

## In your `nix` repo

Add `tatara` as a flake input and wire the module:

```nix
# flake.nix
{
  inputs.tatara = {
    url = "github:pleme-io/tatara";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}

# Somewhere under your home-manager config (e.g. users/drzzln/home.nix):
{ inputs, pkgs, ... }:
{
  imports = [ inputs.tatara.homeManagerModules.default ];

  services.tatara-os-vm = {
    enable  = true;
    package = inputs.tatara.packages.${pkgs.system}.default;

    hostname  = "plex";
    cpus      = 4;
    memoryMib = 4096;

    services = [
      { name = "greeter";
        exec = "/bin/busybox sh -c 'while true; do echo hi; sleep 10; done'"; }
      # future: sshd entry once the initrd builder bridges openssh
    ];

    sshAuthorizedKeys = [
      "ssh-ed25519 AAAA… drzzln@laptop"
    ];

    # Set false if you haven't enabled linux-builder yet.
    prebuildInitrd = true;
    busybox        = true;
  };
}
```

Then:

```sh
cd ~/code/github/pleme-io/nix
nix run .#rebuild
```

## What the module does at rebuild time

1. **Generates** the boot artifact tree by running `tatara-boot-gen` against
   a `(defsystem …)` + `(defvm …)` Lisp form derived from your options
   (or from a hand-written `services.tatara-os-vm.systemFile` if you prefer
   to own the Lisp). Output: `~/.local/share/tatara-os/<hostname>/`.

2. **Pre-realizes** `initrd.nix` via `nix build` so the first boot doesn't
   stall on a 20-minute kernel compile. The realized path lands at
   `/nix/store/<hash>-initrd-<hostname>/initrd.cpio.gz`.

3. **Emits `launch.sh`** — a tiny script that, when run, realizes the
   kernel + initrd (cached after step 2), splices real `/nix/store` paths
   into `vm.json` via `jq`, then execs `vfkit --config vm-resolved.json`.

4. **Writes `authorized_keys`** from `services.tatara-os-vm.sshAuthorizedKeys`
   into the VM root so the SSH provisioning piece (next step) can read it.

## Boot + SSH

```sh
~/.local/share/tatara-os/plex/launch.sh
# vfkit boots the Linux guest. When openssh lands in the initrd:
ssh -p 2222 drzzln@<guest-ip>
```

## What's live vs. what's next

| Piece                                     | Status |
| ----------------------------------------- | ------ |
| `(defsystem …)` + `(defvm …)` parse       | ✅     |
| `BootSynthesizer` emits vm.json + initrd.nix | ✅  |
| `nix build -f initrd.nix` produces real cpio.gz | ✅ (verified in `/nix/store`) |
| HM module activation regenerates artifacts | ✅    |
| `launch.sh` runs vfkit with real store paths | ✅  |
| openssh + authorized_keys baked into initrd | ⏳ Next commit — update `LinuxRootfs` to bridge `openssh` from nixpkgs and emit `/etc/ssh/{sshd_config, authorized_keys, ssh_host_key}`. |
| vfkit NAT → host port forward             | ⏳ Current plan: use `vfkit`'s `--device virtio-net,nat` with `vmnet-shared` so the guest gets an IP on a host-visible subnet. No port forwarding needed — SSH straight to the guest IP. |
| launchd agent for auto-start on login     | ⏳ Later; today `launch.sh` is foreground. |

The gap from here to `ssh drzzln@<guest-ip>` is one commit on the tatara
side (openssh bridge + `/etc/ssh` assets) and no changes on the nix side —
your `services.tatara-os-vm.sshAuthorizedKeys` list is already plumbed.
