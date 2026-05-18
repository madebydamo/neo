# Beszel agent options (minimal, websocket only via HUB_URL + TOKEN, KEY still required by agent).
{...}: {
  flake.modules.nixos.beszel-agent-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.beszel-agent = mkOption {
        type = types.submodule {
          options = {
            enabled = mkEnableOption "beszel agent service";
            hubUrl = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Hub URL for websocket connection (e.g. http://beszel:8090)";
            };
            key = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Public key from hub (shown when adding system)";
            };
            token = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Universal or system token for websocket auth (from hub /settings/tokens)";
            };
          };
        };
        default = {};
        description = "Beszel agent service configuration";
      };
    };
}
