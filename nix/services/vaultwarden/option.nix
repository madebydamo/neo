# Vaultwarden service options.
{...}: {
  flake.modules.nixos.vaultwarden-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.vaultwarden = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "your own password manager that works with the Bitwarden apps" {rank = 0;};
              port = mkOption {
                type = types.port;
                default = 8888;
                internal = true;
                description = "Internal port vaultwarden listens on (ROCKET_PORT)";
              };
              adminToken = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                # ADMIN_TOKEN; without it Vaultwarden disables /admin.
                description = "Password for the Vaultwarden admin page (/admin). Leave empty to turn that page off.";
                rank = 10;
                ui = lib.neo.ui.mkUi {label = "Admin page password";};
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
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/vaultwarden"
            // lib.neo.mkServiceMeta {
              category = "Security";
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/dani-garcia/vaultwarden/blob/main/LICENSE.txt";
              };
              icon = ./icon.svg;
              tagline = "A password manager for the whole family";
              description = ''
                Store passwords, notes and cards safely and fill them in on every device with the Bitwarden apps and browser extensions.
                Share passwords with family members in shared collections.
              '';
              notes = ''
                Bitwarden-compatible server written in Rust. Supports organizations, collections, policies, two-factor login (including hardware keys and WebAuthn), emergency access, Send and an admin page.
              '';
              replaces = ["1Password" "LastPass" "iCloud Keychain"];
              privacy = ["passwords"];
              githubUrl = "https://github.com/dani-garcia/vaultwarden";
              releaseUrl = "https://github.com/dani-garcia/vaultwarden/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Vaultwarden service configuration";
      };
    };
}
