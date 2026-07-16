# Beszel agent options (minimal, websocket only via HUB_URL + TOKEN, KEY still required by agent).
{...}: {
  flake.modules.nixos.beszel-agent-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.beszel-agent = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "beszel agent service" {rank = 0;};
              hubUrl = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Hub URL for websocket connection (e.g. http://beszel:8090)";
                rank = 10;
              };
              key = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Public key from hub (shown when adding system)";
                rank = 20;
              };
              token = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Universal or system token for websocket auth (from hub /settings/tokens)";
                rank = 30;
              };
            }
            // lib.neo.mkContainerDefinitions {
              "beszel-agent" = "henrygd/beszel-agent:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/beszel-agent"
            // lib.neo.mkServiceMeta {
              category = "Monitoring";
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/beszel.svg";
              description = ''
                Beszel agent is the lightweight companion to the Beszel monitoring hub.
                It runs on target hosts and reports detailed system and container metrics (CPU, RAM, disk, network, temps, SMART, GPUs, Docker/Podman) back to the hub over secure websocket.
                Extremely low resource usage; designed for always-on background operation with no requirement for inbound network access or public ports.
              '';
              projectUrl = "https://beszel.dev/";
              githubUrl = "https://github.com/henrygd/beszel";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Beszel agent service configuration";
      };
    };
}
