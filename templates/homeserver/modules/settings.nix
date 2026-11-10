{...}: {
  flake.modules.nixos.settings = {
    config,
    lib,
    ...
  }: let
    # neo-web evaluates edited settings without reloading this flake: it points
    # neoSettingsDir at a copy of settings.toml and the secrets files and
    # re-evaluates the configuration (docs/design/eval-latency.md). Builds
    # leave it unset and read the files next to this flake.
    dir = config.neoSettingsDir;
    file = name:
      if dir == null
      then ../. + "/${name}"
      else /. + "${dir}/${name}";
    settingsPath = file "settings.toml";
    tomlSettings =
      if builtins.pathExists settingsPath
      then builtins.fromTOML (builtins.readFile settingsPath)
      else {};
    hasSettings = builtins.pathExists ../settings.toml;
    # Secrets kept out of settings.toml (docs/design/secrets.md). Guarded so a
    # template refreshed ahead of its neo input still evaluates.
    secrets =
      if lib.neo ? secrets.load
      then
        lib.neo.secrets.load {
          plainFile = file "secrets.yaml";
          sopsFile = file "secrets.sops.yaml";
        }
      else {
        values = {};
        settings = {};
      };
  in {
    options.neoSettingsDir = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      internal = true;
      description = "Directory to read settings.toml and the secrets files from instead of this flake (set by the neo web UI while you edit).";
    };
    config = {
      neo = lib.foldl' lib.recursiveUpdate tomlSettings [
        (secrets.values or {})
        (secrets.settings or {})
      ];
      environment.etc."neo/settings.toml" = lib.mkIf hasSettings {
        source = ../settings.toml;
        mode = "0600";
        user = "homeserver";
        group = "homeserver";
      };
    };
  };
}
