# Authentik (identity provider: SSO via OIDC / OAuth2 / SAML / LDAP / forward auth) service options.
# Production layout from upstream docker-compose.yml (2026.8): server + worker + Postgres.
# Authentik is the login itself, so tinyauth is not available in front of it.
{...}: {
  flake.modules.nixos.authentik-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.authentik = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "One login for all your apps (single sign-on)" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 9000;
                description = "Internal HTTP port the Authentik server listens on (container port 9000)";
              };
              # AUTHENTIK_SECRET_KEY: signs cookies and tokens; at least 50 characters.
              secretKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "Secret that keeps logins secure. Click Generate once and keep it; changing it signs everyone out of all connected apps.";
                ui = lib.neo.ui.mkUi {label = "Secret key";};
                helper = lib.neo.helpers.randomToken;
              };
              # POSTGRES_PASSWORD / AUTHENTIK_POSTGRESQL__PASSWORD of the internal Postgres container.
              dbPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Password for Authentik's own database. Press Generate before the first start and keep it: changing it later cuts Authentik off from its data.";
                ui = lib.neo.ui.mkUi {
                  label = "Database password";
                };
                helper = lib.neo.helpers.randomToken;
              };
              # AUTHENTIK_BOOTSTRAP_EMAIL: only read on an empty database.
              bootstrapEmail = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 30;
                description = "Email address of the admin account (akadmin). Only used the first time Authentik starts.";
                ui = lib.neo.ui.mkUi {label = "Admin email";};
              };
              # AUTHENTIK_BOOTSTRAP_PASSWORD: only read on an empty database. Without it the
              # password is set at /if/flow/initial-setup/ (do that before exposing it to the Internet).
              bootstrapPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 40;
                description = "Starting password for the admin account (akadmin), only used the first time Authentik starts. Leave empty to choose it on the setup page instead.";
                ui = lib.neo.ui.mkUi {label = "Admin password";};
                helper = lib.neo.helpers.randomToken;
              };
              # SMTP for email stages (magic links, recovery, verification). AUTHENTIK_EMAIL__*.
              # Authentik only supports password auth for SMTP (no OAuth2).
              email = mkOption {
                type = types.submodule {
                  options = {
                    host = mkOption {
                      type = types.str;
                      default = "";
                      description = "SMTP server, e.g. smtp.office365.com. Empty: Authentik sends no email.";
                      rank = 0;
                      ui = lib.neo.ui.mkUi {label = "Mail server";};
                    };
                    port = mkOption {
                      type = types.port;
                      default = 587;
                      description = "Port of your mail server: usually 587, or 465 for SSL.";
                      rank = 10;
                      ui = lib.neo.ui.mkUi {visibleWhen = "host";};
                    };
                    username = mkOption {
                      type = types.str;
                      default = "";
                      description = "Login for your mail server. Leave empty if it needs none.";
                      rank = 20;
                      ui = lib.neo.ui.mkUi {visibleWhen = "host";};
                    };
                    password = lib.neo.mkSecretOption {
                      type = types.str;
                      default = "";
                      description = "Password for your mail server login.";
                      rank = 30;
                      ui = lib.neo.ui.mkUi {visibleWhen = "host";};
                    };
                    from = mkOption {
                      type = types.str;
                      default = "";
                      description = "Sender shown on the emails, e.g. Authentik <noreply@example.com>. Your mail account must be allowed to send as this address.";
                      rank = 40;
                      ui = lib.neo.ui.mkUi {
                        label = "Sender";
                        visibleWhen = "host";
                      };
                    };
                    # STARTTLS; mutually exclusive with useSsl (assertion in default.nix).
                    useTls = mkOption {
                      type = types.bool;
                      default = true;
                      description = "Encrypt with STARTTLS, usual for port 587. Turn off if you use SSL.";
                      rank = 50;
                      ui = lib.neo.ui.mkUi {
                        label = "Use STARTTLS";
                        visibleWhen = "host";
                      };
                    };
                    # Implicit TLS; mutually exclusive with useTls.
                    useSsl = mkOption {
                      type = types.bool;
                      default = false;
                      description = "Encrypt with SSL, usual for port 465. Turn off Use STARTTLS when you turn this on.";
                      rank = 60;
                      ui = lib.neo.ui.mkUi {
                        label = "Use SSL";
                        visibleWhen = "host";
                      };
                    };
                    timeout = mkOption {
                      type = types.int;
                      default = 10;
                      description = "Seconds to wait for the mail server before giving up.";
                      rank = 70;
                      ui = lib.neo.ui.mkUi {
                        label = "Timeout (seconds)";
                        group = lib.neo.ui.groups.advanced;
                        visibleWhen = "host";
                      };
                    };
                  };
                };
                default = {};
                description = "Mail server Authentik uses to send login links, password resets and verification emails.";
                rank = 50;
                ui = lib.neo.ui.mkUi {
                  group = lib.neo.ui.mkGroup {
                    id = "email";
                    label = "Email";
                    description = "SMTP server Authentik sends magic links and recovery emails through";
                    icon = lib.neo.ui.icons.mail;
                    rank = 40;
                  };
                };
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "authentik";
              # OIDC / SAML endpoints and the login flow must be reachable without a prior login.
              auth.available = false;
            }
            // lib.neo.mkContainerDefinitions {
              # Pinned: Authentik upgrades run DB migrations and may only skip no major versions.
              # Read the release notes before upgrading. Keep server and worker on the same tag.
              authentik = "ghcr.io/goauthentik/server:2026.8.4";
              "authentik-worker" = "ghcr.io/goauthentik/server:2026.8.4";
              "authentik-db" = "postgres:16-alpine";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/authentik"
            // lib.neo.mkServiceMeta {
              category = "Security";
              icon = ./icon.svg;
              tagline = "One account for all your apps, with advanced login rules";
              description = ''
                Gives you and your family a single account that works across your apps, with two-factor login, user groups and invitations.
                Apps that offer a "Sign in with…" button (OpenID Connect, SAML or LDAP) can use it.
                Most homes are fine with tinyauth; Authentik is for when you need more control.
              '';
              notes = ''
                Runs the upstream stack: the Authentik server (web UI and API), a background worker and Postgres.
                Authentik is the login itself, so tinyauth does not sit in front of it. Apps reach its OIDC and SAML endpoints directly over HTTPS.
                The image tag is pinned. Read the release notes before upgrading, and change the server and worker image together.
                The admin user is akadmin. Set a bootstrap password here, or finish /if/flow/initial-setup/ on first boot.
              '';
              replaces = ["Okta" "Microsoft Entra ID"];
              projectUrl = "https://docs.goauthentik.io";
              githubUrl = "https://github.com/goauthentik/authentik";
              releaseUrl = "https://github.com/goauthentik/authentik/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Authentik service configuration";
      };
    };
}
