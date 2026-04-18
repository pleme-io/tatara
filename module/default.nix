{ hmHelpers }:
{ config, lib, pkgs, ... }:

{
  imports = [
    (import ./tatara-service.nix { inherit hmHelpers; })
    (import ./ro.nix { inherit hmHelpers; })
    (import ./tatara-os-vm.nix { inherit hmHelpers; })
  ];
}
