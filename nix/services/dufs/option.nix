# Dufs (static file server + WebDAV) service options.
# Web UI protected with tinyauth; /__dufs__/health and WebDAV skip edge auth
# when password is set so native clients can use HTTP Basic.
{...}: {
  flake.modules.nixos.dufs-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      networkDriveGroup = lib.neo.ui.mkGroup {
        id = "drive";
        label = "Network drive";
        description = "Open your files as a drive in Finder, Windows Explorer or rclone";
        icon = lib.neo.ui.icons.folder;
        rank = 50;
      };
    in {
      options.neo.services.dufs = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "A simple file server: browse, upload and share your files in the browser or as a network drive" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 5000;
                description = "Internal port dufs listens on";
              };
              # HTTP Basic username (DUFS_AUTH); only used when password is set.
              username = mkOption {
                type = types.str;
                default = "dufs";
                rank = 10;
                description = "Username for network-drive apps (used with the password).";
                ui = lib.neo.ui.mkUi {
                  group = networkDriveGroup;
                };
              };
              # HTTP Basic password (DUFS_AUTH). When set, WebDAV clients skip
              # tinyauth and authenticate to dufs; the browser UI stays behind
              # tinyauth and SWAG injects these credentials (no second login).
              # When unset, only tinyauth protects dufs and WebDAV clients that
              # cannot keep cookies fail.
              password = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 11;
                description = "Password for apps that connect as a network drive (Finder, Windows Explorer, rclone). Leave empty if you only use the browser.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {
                  group = networkDriveGroup;
                };
              };
            }
            # Entries bind-mount localPath (host) at containerPath (inside dufs);
            # media and documents are mounted under /data by default.
            // lib.neo.mkAdditionalMountPoints {
              rank = 20;
              app = "Dufs";
              example = "/data/Holiday photos";
              description = "More folders from your server to show in Dufs, next to Media and Documents.";
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "dufs";
              auth = {
                enabled = true;
                publicPaths = [
                  # Health probe for smoke tests and monitors (no session cookie).
                  "^/__dufs__/health$"
                ];
              };
            }
            // lib.neo.mkContainerDefinitions {
              dufs = "sigoden/dufs:latest";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/dufs"
            // lib.neo.mkServiceMeta {
              category = "Files";
              license = {
                spdx = "MIT OR Apache-2.0";
                url = "https://github.com/sigoden/dufs/blob/main/LICENSE-APACHE";
              };
              icon = ./icon.png;
              tagline = "Share folders and use them as a network drive";
              description = ''
                A simple file server in the browser: browse, upload by drag and drop, search and download whole folders.
                Your computer can also connect to it as a network drive in Windows Explorer or Mac Finder.
              '';
              notes = ''
                Neo mounts media and documents under /data and adds a writable share; add more under Extra folders.
                The web UI is behind tinyauth. Set username and password so rclone, davfs2, Windows Explorer and macOS Finder can use WebDAV over HTTPS with Basic auth; SWAG adds those credentials after tinyauth, so the browser does not ask twice.
                GET /__dufs__/health skips the login for health checks.
              '';
              replaces = ["Dropbox"];
              privacy = ["files"];
              projectUrl = "https://github.com/sigoden/dufs";
              githubUrl = "https://github.com/sigoden/dufs";
              releaseUrl = "https://github.com/sigoden/dufs/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Dufs file server and WebDAV configuration";
      };
    };
}
