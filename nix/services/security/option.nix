# Security scan options.
# Scanners: vulnxscan (vulnix + grype + OSV, sbomnix) for the NixOS system,
# Trivy for the Docker images. An AI reviews what is left after the
# deterministic filters (`neo security`, cli/src/commands/security/).
# The report has its own subdomain behind tinyauth, not a page in neo web.
# Off by default: most findings are upstream work (nixpkgs, image
# maintainers), not something a user can act on.
{...}: {
  flake.modules.nixos.security-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      aiGroup = lib.neo.ui.mkGroup {
        id = "security-ai";
        label = "AI review";
        description = "Which AI reads the findings and decides which ones matter on this server.";
        icon = lib.neo.ui.icons.sparkles;
        rank = 20;
      };
    in {
      options.neo.services.security = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "regular checks of your server's software for known security holes, reviewed by AI" {rank = 0;};

              # Loopback; SWAG reaches it via host.docker.internal + DNAT.
              port = mkOption {
                type = types.port;
                internal = true;
                default = 8094;
                description = "Internal port of the security report page.";
              };

              # systemd OnCalendar expression.
              schedule = mkOption {
                type = types.str;
                default = "weekly";
                example = "daily";
                rank = 10;
                description = "How often the check runs, for example daily or weekly. A check takes a while and downloads the vulnerability databases.";
                ui = lib.neo.ui.mkUi {label = "How often";};
              };

              minSeverity = mkOption {
                type = types.enum ["low" "medium" "high" "critical"];
                default = "high";
                rank = 15;
                description = "Findings below this severity are left out of the report.";
                ui = lib.neo.ui.mkUi {
                  label = "Lowest severity shown";
                  choiceLabels = {
                    low = "Low";
                    medium = "Medium";
                    high = "High";
                    critical = "Critical";
                  };
                };
              };

              ai = mkOption {
                type = types.submodule {
                  options = {
                    # auto: Hermes when its gateway token turns on its API
                    # server (OpenAI-compatible /v1/chat/completions), else off.
                    source = mkOption {
                      type = types.enum ["auto" "off" "hermes" "custom"];
                      default = "auto";
                      rank = 0;
                      description = "Which AI reviews the findings. Automatic uses the assistant when its gateway token is set and otherwise skips the review.";
                      ui = lib.neo.ui.mkUi {
                        label = "AI";
                        choiceLabels = {
                          auto = "Automatic";
                          off = "No AI review";
                          hermes = "The assistant (Hermes)";
                          custom = "Another AI service";
                        };
                      };
                    };

                    # OpenAI-compatible base URL including /v1.
                    baseUrl = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 10;
                      example = "http://127.0.0.1:11434/v1";
                      description = "Address of an OpenAI-compatible AI service, for example Ollama at http://127.0.0.1:11434/v1 or https://api.openai.com/v1.";
                      ui = lib.neo.ui.mkUi {
                        label = "AI server address";
                        visibleWhen = "source=custom";
                      };
                    };

                    apiKey = lib.neo.mkSecretOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 20;
                      description = "The key from your AI service's account page. Leave empty when your own AI server needs no key.";
                      ui = lib.neo.ui.mkUi {
                        label = "API key";
                        visibleWhen = "source=custom";
                      };
                    };

                    model = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 30;
                      example = "gpt-5-mini";
                      description = "The AI model to use. With the assistant, empty means the assistant's own model.";
                      ui = lib.neo.ui.mkUi {
                        label = "Model";
                        visibleWhen = "source=hermes,custom";
                      };
                    };

                    # Budget per run. Verdicts are cached, so later runs only
                    # review new findings.
                    maxReviews = mkOption {
                      type = types.ints.between 1 1000;
                      default = 40;
                      rank = 40;
                      description = "At most this many groups of findings are reviewed per check. The rest wait for the next check.";
                      ui = lib.neo.ui.mkUi {
                        label = "Reviews per check";
                        group = lib.neo.ui.groups.advanced;
                      };
                    };
                  };
                };
                default = {};
                rank = 20;
                description = "The AI that reads the findings, explains them and decides which ones matter on this server. Package names and vulnerability details are sent to it.";
                ui = lib.neo.ui.mkUi {
                  group = aiGroup;
                  label = "AI review";
                };
              };
            }
            // lib.neo.mkReverseProxyOptions {subdomain = "security";}
            // lib.neo.mkSystemdUnits ["neo-security-web" "neo-security-scan"]
            // lib.neo.mkSystemdTimers ["neo-security-scan"]
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/security"
            // lib.neo.mkServiceMeta {
              category = "Security";
              icon = ./icon.svg;
              license = {
                spdx = "Apache-2.0";
                url = "https://github.com/aquasecurity/trivy/blob/main/LICENSE";
              };
              tagline = "Checks your server's software for known security holes";
              description = ''
                Regularly checks every program on the server and in its apps against public lists of known security holes.
                An AI then sorts out the false alarms and explains what is left, on a page of its own.
              '';
              notes = ''
                The report lives at its own subdomain behind tinyauth.
                Scanners: vulnxscan (vulnix, grype, OSV) over /run/current-system, Trivy over the local Docker images. Run a check now with `systemctl start neo-security-scan`, or `neo security scan` / `neo security triage` as root.
                The AI review runs through the assistant's OpenAI-compatible API (or another one) and may read /proc/*/maps to see what is loaded. Package names and CVE details are sent to it.
              '';
            };
        };
        default = {};
        description = "Security scan of the system and the containers";
      };
    };
}
