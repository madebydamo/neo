# Vaultwarden service options.
{...}: {
  flake.modules.nixos.vaultwarden-option = {lib, ...}:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.vaultwarden = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "vaultwarden password manager service" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 8888;
                internal = true;
                description = "Internal port vaultwarden listens on (ROCKET_PORT)";
                rank = 10;
              };
              adminToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Random auth token to authenticate in admin page";
                rank = 20;
                helper = lib.neo.helpers.randomToken;
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "vaultwarden";
              auth.publicPaths = [
                "^/api/"
                "^/identity/"
                "^/notifications/"
                "^/icons/"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              vaultwarden = "vaultwarden/server:latest";
            }
            // lib.neo.mkServiceMeta {
              icon = "https://raw.githubusercontent.com/dani-garcia/vaultwarden/main/resources/vaultwarden-icon.svg";
              description = ''
                Vaultwarden is a lightweight alternative server implementation of the Bitwarden password manager API, written entirely in Rust for efficiency and security.
                It delivers full compatibility with official Bitwarden clients, browser extensions, desktop apps, and mobile apps, enabling complete self-hosted password management including logins, secure notes, cards, identities, attachments, and Send for temporary shares.
                The project supports organizations with advanced sharing, collections, policies, multiple 2FA options (including hardware keys and WebAuthn), emergency access, website favicons, and includes a bundled admin backend and modified web vault.
                Perfect for privacy-conscious users and small teams who want a resource-light, private Bitwarden-compatible vault without relying on the official hosted or heavy server software.
              '';
              githubUrl = "https://github.com/dani-garcia/vaultwarden";
              releaseUrl = "https://github.com/dani-garcia/vaultwarden/releases";
            };
        };
        default = {};
        description = "Vaultwarden service configuration";
      };
    };
}
