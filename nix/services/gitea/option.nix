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
              enabled = mkEnableOption "Your own place to keep code and config in Git, like a private GitHub" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 3000;
                internal = true;
                description = "Internal HTTP port Gitea listens on (HTTP_PORT)";
              };
              # ROOT_URL. Empty: first customDomains entry as https://…, else
              # https://<subdomain>.<swag.domain>.
              rootUrl = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "The web address people use to reach Gitea, for example https://git.example.com. Leave empty to set it automatically.";
                ui = lib.neo.ui.mkUi {
                  label = "Web address";
                };
              };
              disableRegistration = mkOption {
                type = types.bool;
                default = true;
                rank = 20;
                description = "Stop strangers from creating accounts; only the admin can add users.";
                ui = lib.neo.ui.mkUi {
                  label = "Disable sign-ups";
                };
              };
              ssh = mkOption {
                type = types.submodule {
                  options = {
                    # Served by the image's bundled OpenSSH (deploy-key clones).
                    enable = mkEnableOption "copying repositories over SSH. Only turn it on once SSH traffic reaches this server" {rank = 0;};
                    port = mkOption {
                      type = types.port;
                      default = 2222;
                      rank = 10;
                      # SSH_PORT advertised in clone URLs (edge port, not the host sshd).
                      description = "Port shown in SSH clone links; the port people connect to from outside.";
                      ui = lib.neo.ui.mkUi {
                        visibleWhen = "enable";
                      };
                    };
                    listenPort = mkOption {
                      type = types.port;
                      default = 2222;
                      rank = 20;
                      # Host loopback publish: 127.0.0.1:listenPort -> container OpenSSH :22.
                      description = "Port on the server where Gitea's SSH listens locally, for forwarding SSH traffic to it.";
                      ui = lib.neo.ui.mkUi {
                        group = lib.neo.ui.groups.advanced;
                        label = "Local SSH port";
                        visibleWhen = "enable";
                      };
                    };
                    domain = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 30;
                      # SSH_DOMAIN; empty = hostname of rootUrl.
                      description = "Address shown in SSH clone links; leave empty to use your Gitea address.";
                      ui = lib.neo.ui.mkUi {
                        label = "SSH address";
                        visibleWhen = "enable";
                      };
                    };
                  };
                };
                default = {};
                rank = 30;
                # Bundled OpenSSH on container :22 (built-in Go SSH server stays off;
                # host keys persist in appdata/data/ssh). Needs an edge TCP forward.
                description = "Copy and update repositories over SSH with keys instead of a password.";
                ui = lib.neo.ui.mkUi {
                  label = "SSH access";
                };
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
              gitea = "gitea/gitea:28.3.0";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/gitea"
            // lib.neo.mkServiceMeta {
              category = "Files";
              license = {
                spdx = "MIT";
                url = "https://github.com/go-gitea/gitea/blob/main/LICENSE";
              };
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
