# Activation writes into appdata (Hermes .env / config.yaml, secret copies such
# as SWAG snippets). A separate filesystem under the Neo volumes that is not
# mounted until after activation hides those files at boot (disko marks its own
# dataset neededForBoot). Warn for other layouts.
{...}: {
  flake.modules.nixos.volumes-boot-check = {
    config,
    lib,
    ...
  }: let
    appdata = config.neo.core.volumes.appdata;
    # Mount points the appdata path lives on (/var/neo, /var/neo/DATA, …).
    covering =
      lib.filterAttrs (mp: fs: mp != "/" && lib.hasPrefix "${lib.removeSuffix "/" mp}/" "${appdata}/" && !(fs.neededForBoot or false))
      config.fileSystems;
  in {
    warnings = lib.mapAttrsToList (mp: _: "fileSystems.\"${mp}\" holds Neo appdata (${appdata}) but is mounted after activation, so files activation writes there (Hermes .env and config.yaml, secret copies) are hidden at boot until the next live switch. Set fileSystems.\"${mp}\".neededForBoot = true.") covering;
  };
}
