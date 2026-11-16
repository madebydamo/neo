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
              # OIDC login through tinyauth's OIDC server (tinyauth.oidcClients.vaultwarden).
              sso = mkOption {
                type = types.bool;
                default = false;
                description = "Sign in to Vaultwarden with the shared login page (tinyauth), using the same account as your other apps. You still unlock your vault with your master password.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Sign in with the login page";};
              };
              # SSO_ONLY: hides email + master password login.
              ssoOnly = mkOption {
                type = types.bool;
                default = false;
                description = "Allow signing in only through the login page, not with email and master password. Existing accounts are linked by their email address on their first sign-in.";
                rank = 21;
                ui = lib.neo.ui.mkUi {
                  label = "Login page only";
                  visibleWhen = "sso";
                };
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
                Sign in with the login page: the Bitwarden apps and the web vault show "Use single sign-on" after you enter your email. Everyone still sets a master password on the first sign-in, because only it unlocks the vault.
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
