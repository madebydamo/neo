# Collects all flake.libExtensions.* attr sets, deep-merges them,
# and exports a single extended lib as flake.lib.
{
  inputs,
  config,
  lib,
  ...
}: let
  nixpkgsLib = inputs.nixpkgs.lib;
  allLibExtensions = lib.attrValues config.libExtensions;
  merged = lib.foldl' lib.recursiveUpdate {} allLibExtensions;
in {
  options.libExtensions = lib.mkOption {
    type = lib.types.attrsOf lib.types.attrs;
    default = {};
    description = "Library extension attr sets. All are deep-merged and added to flake.lib.";
  };

  config.flake.lib = nixpkgsLib.extend (self: super: merged);
}
