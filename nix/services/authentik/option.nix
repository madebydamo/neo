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
              enabled = mkEnableOption "Authentik identity provider" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 9000;
                description = "Internal HTTP port the Authentik server listens on (container port 9000)";
              };
              secretKey = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  AUTHENTIK_SECRET_KEY: long random secret used to sign cookies and tokens.
                  Generate once and keep it. Changing it signs everyone out and invalidates issued tokens.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              dbPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Password for the internal Postgres container (POSTGRES_PASSWORD / AUTHENTIK_POSTGRESQL__PASSWORD)";
                helper = lib.neo.helpers.randomToken;
              };
              bootstrapEmail = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 30;
                description = ''
                  AUTHENTIK_BOOTSTRAP_EMAIL: email of the default admin user akadmin.
                  Only read on the very first start (empty database).
                '';
              };
              bootstrapPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 40;
                description = ''
                  AUTHENTIK_BOOTSTRAP_PASSWORD: initial password of the default admin user akadmin.
                  Only read on the very first start (empty database). Leave empty to set the password
                  yourself at /if/flow/initial-setup/ instead. Do that before you expose the service to the Internet.
                '';
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
                    };
                    port = mkOption {
                      type = types.port;
                      default = 587;
                      description = "SMTP port (587 STARTTLS, 465 implicit TLS, 25 relay).";
                      rank = 10;
                    };
                    username = mkOption {
                      type = types.str;
                      default = "";
                      description = "SMTP login. Empty for an unauthenticated relay (e.g. M365 connector by IP).";
                      rank = 20;
                    };
                    password = lib.neo.mkSecretOption {
                      type = types.str;
                      default = "";
                      description = "SMTP password.";
                      rank = 30;
                    };
                    from = mkOption {
                      type = types.str;
                      default = "";
                      description = "Sender, e.g. Authentik <noreply@example.com>. Must be allowed to send as this address.";
                      rank = 40;
                    };
                    useTls = mkOption {
                      type = types.bool;
                      default = true;
                      description = "STARTTLS (port 587). Mutually exclusive with useSsl.";
                      rank = 50;
                    };
                    useSsl = mkOption {
                      type = types.bool;
                      default = false;
                      description = "Implicit TLS (port 465). Mutually exclusive with useTls.";
                      rank = 60;
                    };
                    timeout = mkOption {
                      type = types.int;
                      default = 10;
                      description = "SMTP connection timeout in seconds.";
                      rank = 70;
                    };
                  };
                };
                default = {};
                description = "Outgoing email (SMTP) for magic links, recovery and verification emails.";
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
              description = ''
                Authentik is an open-source identity provider for single sign-on across your self-hosted apps, with OAuth2 / OpenID Connect, SAML, LDAP, SCIM and proxy (forward auth) providers.
                Neo runs the upstream Docker stack: the Authentik server (web UI and API), a background worker, and Postgres, all on the internal network behind SWAG.
                Authentik is the login itself, so tinyauth does not sit in front of it. Apps reach its OIDC and SAML endpoints directly over HTTPS.
                The image tag is pinned. Read the release notes before upgrading, and change the server and worker image together.
                The admin user is akadmin. Set a bootstrap password here, or finish /if/flow/initial-setup/ on first boot.
              '';
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
