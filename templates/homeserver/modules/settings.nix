{...}: {
  flake.modules.nixos.settings = {lib, ...}: let
    settingsPath = ../settings.toml;
    tomlSettings =
      if builtins.pathExists settingsPath
      then builtins.fromTOML (builtins.readFile settingsPath)
      else {};
    hasSettings = builtins.pathExists settingsPath;
  in {
    neo = tomlSettings;
    environment.etc."neo/settings.toml" = lib.mkIf hasSettings {
      source = settingsPath;
      mode = "0600";
      user = "homeserver";
      group = "homeserver";
    };
  };
}
