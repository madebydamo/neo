# Neo web UI service options.
# Exposes the `neo web` config editor at the `neo.*` subdomain.
# Auth enabled by default via mkReverseProxyOptions.
{...}: {
  flake.modules.nixos.neo-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.neo = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Neo web UI service (config editor launched via `neo web`)" {
                default = true;
                internal = true;
              };

              theme = mkOption {
                type = types.enum ["lofi" "halloween" "dark" "light"];
                default = "lofi";
                rank = 10;
                description = "Color theme for the neo web UI (navigator dashboard and configuration editor).";
              };

              port = mkOption {
                type = types.port;
                internal = true;
                default = 8091;
                description = "Internal port for the Neo web UI (binds 127.0.0.1 on host; SWAG via host.docker.internal + DNAT)";
              };

              iframeCookieSupport = mkOption {
                type = types.bool;
                default = true;
                rank = 20;
                description = ''
                  When enabled (default), configures support for loading other services inside
                  the neo web UI iframes (neo.* embeds sub.domain pages):
                  - writes conf.d snippet (relaxed cookies for auth across subdomains; requires
                    include added to nginx.conf)
                  - ensures (via a single script in swag preStart) that proxy.conf has
                    proxy_hide_header for X-Frame-Options and Content-Security-Policy.
                  This resolves cross-origin frame blocking centrally without edits to per-service
                  swag.nix (e.g. pastebin). Set to false to opt out.
                '';
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "neo";
              # Browsers fetch the web app manifest *without* credentials by default.
              # If tinyauth 302s that request to HTML login, install/standalone never works.
              # Icons are fetched the same way for home-screen / splash. Keep the UI itself gated.
              auth.publicPaths = [
                "^/site\\.webmanifest$"
                "^/static/manifest\\.json$"
                "^/static/favicon/"
                "^/static/neo-icon\\.png$"
              ];
            }
            // lib.neo.mkSystemdUnits [
              "neo-web"
              "neo-bootstrap"
            ]
            // lib.neo.mkServiceMeta {
              category = "Core";
              icon = ../../../cli/static/neo-icon.png;
              tagline = "Set up and manage your homeserver";
              description = ''
                The control panel you are using right now: turn apps on, change their settings and apply the changes with one click.
                The sidebar takes you to all your apps.
              '';
              notes = ''
                Changes are written to settings.toml in the config repo and applied by activating.
                neo-bootstrap creates the server config repo at boot; neo-web requires that unit, also when system-updater is off.
              '';
              githubUrl = "https://github.com/madebydamo/neo";
              rank = 0;
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Neo web UI configuration";
      };
    };
}
