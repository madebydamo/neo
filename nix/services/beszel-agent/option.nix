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
              enabled = mkEnableOption "Monitoring agent that reports this homeserver's CPU, memory, disk and app usage to your Beszel dashboard" {rank = 0;};
              # HUB_URL: websocket connection to the hub.
              hubUrl = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Address of your Beszel dashboard. Use http://beszel:8090 when it runs on this homeserver.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Dashboard address";};
              };
              # KEY: hub public key.
              key = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Key shown in the Beszel dashboard when you click Add System.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Dashboard key";};
              };
              # TOKEN: universal or per-system token (hub /settings/tokens).
              token = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Token from the Beszel dashboard under Settings › Tokens.";
                rank = 30;
                ui = lib.neo.ui.mkUi {label = "Token";};
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
