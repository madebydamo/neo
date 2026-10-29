# Syncthing service options.
{...}: {
  flake.modules.nixos.syncthing-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.syncthing = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "syncthing service" {rank = 0;};
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "syncthing";
              auth.enabled = true;
            }
            // lib.neo.mkContainerDefinitions {
              syncthing = "linuxserver/syncthing:latest";
              extraUnits = ["syncthing-config"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/syncthing"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = ./icon.svg;
              tagline = "Keep folders in sync between your devices";
              description = ''
                Keeps folders identical on your computers, phone and server, in real time.
                Files go straight between your devices, encrypted, without a cloud in between.
              '';
              notes = ''
                Peer-to-peer with TLS (perfect forward secrecy); devices authenticate with certificates. Syncs over the LAN and the internet with automatic discovery. Open source (MPLv2).
              '';
              replaces = ["Dropbox" "OneDrive"];
              privacy = ["files"];
              projectUrl = "https://syncthing.net/";
              githubUrl = "https://github.com/syncthing/syncthing";
              releaseUrl = "https://github.com/syncthing/syncthing/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Syncthing service configuration";
      };
    };
}
