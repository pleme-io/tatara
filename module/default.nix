{ hmHelpers }:
{ config, lib, pkgs, ... }:

{
  imports = [
    (import ./tatara-service.nix { inherit hmHelpers; })
    (import ./ro.nix { inherit hmHelpers; })
  ];
}
