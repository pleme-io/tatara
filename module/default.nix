{ hmHelpers }:
{ config, lib, pkgs, ... }:

{
  imports = [
    (import ./tatara-service.nix { inherit hmHelpers; })
    ./ro.nix
  ];
}
