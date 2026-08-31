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
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.dufs = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "dufs file server and WebDAV service" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 5000;
                description = "Internal port dufs listens on";
              };
              username = mkOption {
                type = types.str;
                default = "dufs";
                rank = 10;
                description = "HTTP Basic username for dufs and WebDAV clients. Used when password is set.";
              };
              password = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 11;
                description = ''
                  HTTP Basic password for dufs and WebDAV clients. Generate once and keep stable.
                  When set, native WebDAV clients skip tinyauth and authenticate to dufs; the browser UI stays behind tinyauth and SWAG injects these credentials so there is no second login form.
                  When unset, only tinyauth protects the service and public WebDAV clients that cannot store cookies will fail.
                '';
                helper = lib.neo.helpers.randomToken;
              };
            }
            // lib.neo.mkAdditionalMountPoints {
              rank = 20;
              description = ''
                Extra host directories to expose in dufs (beyond the default media and documents mounts under /data).
                Each entry pairs a localPath (absolute host path) with a containerPath (e.g. /data/MyShare).
              '';
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
              icon = "https://raw.githubusercontent.com/sigoden/dufs/main/assets/favicon.ico";
              description = ''
                Dufs is a distinctive utility file server written in Rust: static serving, drag-and-drop uploads, search, resumable transfers, and first-class WebDAV.
                Neo mounts media and documents under /data by default and adds a writable share at /data; extra host paths go in additionalMountPoints.
                The web UI is behind tinyauth. Set username/password so rclone, davfs2, Windows Explorer, and macOS Finder can use HTTPS WebDAV with HTTP Basic; SWAG injects those credentials after tinyauth so the browser does not see a second login.
                GET /__dufs__/health bypasses edge auth for probes.
              '';
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
