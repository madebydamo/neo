{...}: {
  flake.modules.nixos.settings = {lib, ...}: let
    settingsPath = ../settings.toml;
    tomlSettings =
      if builtins.pathExists settingsPath
      then builtins.fromTOML (builtins.readFile settingsPath)
      else {};
    hasSettings = builtins.pathExists settingsPath;
    # Secrets kept out of settings.toml (docs/design/secrets.md). Guarded so a
    # template refreshed ahead of its neo input still evaluates.
    secrets =
      if lib.neo ? secrets.load
      then
        lib.neo.secrets.load {
          plainFile = ../secrets.yaml;
          sopsFile = ../secrets.sops.yaml;
        }
      else {
        values = {};
        settings = {};
      };
  in {
    neo = lib.foldl' lib.recursiveUpdate tomlSettings [
      secrets.values
      secrets.settings
    ];
    environment.etc."neo/settings.toml" = lib.mkIf hasSettings {
      source = settingsPath;
      mode = "0600";
      user = "homeserver";
      group = "homeserver";
    };
  };
}
