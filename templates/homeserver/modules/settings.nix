{...}: {
  flake.modules.nixos.settings = {...}: let
    settingsPath = ../settings.toml;
    tomlSettings =
      if builtins.pathExists settingsPath
      then builtins.fromTOML (builtins.readFile settingsPath)
      else {};
  in {
    neo = tomlSettings;
  };
}
