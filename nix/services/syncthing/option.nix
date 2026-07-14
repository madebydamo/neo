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
              additionalMountPoints = mkOption {
                type = types.attrsOf types.str;
                default = {};
                rank = 10;
                description = "Additional volume mounts";
              };
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
              icon = "https://raw.githubusercontent.com/syncthing/syncthing/main/assets/logo-only.svg";
              description = ''
                Syncthing is a continuous file synchronization program that keeps files in sync across two or more computers in real time.
                It is fully peer-to-peer with no central server or third-party storage, giving you complete control over your data and where it lives.
                All communication is encrypted with TLS (including perfect forward secrecy) and devices are authenticated using strong cryptographic certificates, ensuring only allowed peers can connect.
                A powerful responsive web UI lets you configure, monitor, and manage sync folders from any browser; it supports LAN and internet syncing with automatic discovery, works on virtually every platform, and requires minimal setup.
                Syncthing is open source under MPLv2 with a fully documented open protocol and transparent development on GitHub.
              '';
              projectUrl = "https://syncthing.net/";
              githubUrl = "https://github.com/syncthing/syncthing";
              releaseUrl = "https://github.com/syncthing/syncthing/releases";
            };
        };
        default = {};
        description = "Syncthing service configuration";
      };
    };
}
