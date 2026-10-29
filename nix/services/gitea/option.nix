# Gitea (self-hosted Git) service options.
# SQLite by default for a lean Heimcloud credentials forge. Tinyauth off so Gitea owns login
# (HTTPS git clone / API tokens); enable auth in settings if you want forward-auth in front.
{...}: {
  flake.modules.nixos.gitea-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.gitea = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Gitea self-hosted Git service" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 3000;
                internal = true;
                description = "Internal HTTP port Gitea listens on (HTTP_PORT)";
              };
              rootUrl = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  Public ROOT_URL for Gitea (e.g. https://git.heimcloud.site).
                  When null: first customDomains entry as https://…, else https://<subdomain>.<swag.domain>.
                '';
              };
              disableRegistration = mkOption {
                type = types.bool;
                default = true;
                rank = 20;
                description = "Disable open registration (invite/admin-created users only)";
              };
              ssh = mkOption {
                type = types.submodule {
                  options = {
                    enable = mkEnableOption "Gitea SSH via the image's bundled OpenSSH (deploy-key clones)" {rank = 0;};
                    port = mkOption {
                      type = types.port;
                      default = 2222;
                      rank = 10;
                      description = "Public SSH_PORT advertised in clone URLs / UI (edge port; not the host sshd)";
                    };
                    listenPort = mkOption {
                      type = types.port;
                      default = 2222;
                      rank = 20;
                      description = "Host loopback publish port: 127.0.0.1:listenPort → container OpenSSH :22";
                    };
                    domain = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 30;
                      description = "SSH_DOMAIN; null = hostname derived from rootUrl / customDomains";
                    };
                  };
                };
                default = {};
                rank = 30;
                description = ''
                  Gitea SSH for deploy-key clones, served by the image's bundled OpenSSH on container :22
                  (built-in Go SSH server stays off; host keys persist in appdata/data/ssh).
                  Keep disabled until the edge TCP forward exists.
                '';
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "gitea";
              auth.enabled = false;
              auth.publicPaths = [
                "^/api/healthz$"
                "^/api/v1/version$"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              gitea = "gitea/gitea:1.22.6";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/gitea"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = ./icon.svg;
              tagline = "Your own GitHub for code and configuration";
              description = ''
                Keep code, scripts and configuration in your own Git repositories, with issues, pull requests and a package registry.
                Good for private projects and for keeping your homeserver configuration safe.
              '';
              notes = ''
                Runs the official image with SQLite under appdata, so no separate database is needed.
                Point rootUrl / customDomains at your public hostname when it is published through streamproxy.
                Registration is off by default; create users from the admin account after first boot.
              '';
              replaces = ["GitHub" "GitLab"];
              privacy = ["code"];
              projectUrl = "https://about.gitea.com/";
              githubUrl = "https://github.com/go-gitea/gitea";
              releaseUrl = "https://github.com/go-gitea/gitea/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Gitea service configuration";
      };
    };
}
