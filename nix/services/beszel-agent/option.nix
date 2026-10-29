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
              token = lib.neo.mkSecretOption {
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
              icon = ./icon.svg;
              tagline = "Reports this server's health to Beszel";
              description = ''
                A small helper that sends this machine's numbers (processor, memory, disks, temperatures and apps) to your Beszel dashboard.
                Turn it on together with Beszel.
              '';
              notes = ''
                Reports CPU, RAM, disk, network, temperatures, S.M.A.R.T., GPUs and Docker containers to the hub over a websocket; needs no inbound ports.
                Copy the key and token from the hub's Add System dialog.
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
