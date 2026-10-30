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
              enabled = mkEnableOption "the neo web UI for changing your settings" {
                default = true;
                internal = true;
              };

              theme = mkOption {
                type = types.enum ["lofi" "halloween" "dark" "light"];
                default = "lofi";
                rank = 10;
                description = "Look of the neo dashboard and settings pages. lofi is a calm black-and-white style, halloween is dark with orange, dark and light are plain dark or light.";
                ui = lib.neo.ui.mkUi {label = "Color theme";};
              };

              guides = mkOption {
                type = types.bool;
                default = true;
                rank = 15;
                description = "Show the Guides tab with step-by-step guides and the privacy map. Turning it off also removes the guides from the assistant.";
                ui = lib.neo.ui.mkUi {label = "Show guides";};
              };

              port = mkOption {
                type = types.port;
                internal = true;
                default = 8091;
                description = "Internal port for the Neo web UI (binds 127.0.0.1 on host; SWAG via host.docker.internal + DNAT)";
              };

              # Lets neo.* embed sub.domain pages in iframes:
              # - writes a SWAG conf.d snippet with relaxed cookies so auth works across
              #   subdomains (requires the include added to nginx.conf)
              # - ensures (one script in swag preStart) that proxy.conf has proxy_hide_header
              #   for X-Frame-Options and Content-Security-Policy.
              # This fixes cross-origin frame blocking centrally, without per-service swag.nix edits.
              iframeCookieSupport = mkOption {
                type = types.bool;
                default = true;
                rank = 20;
                description = "Lets your apps open inside the neo dashboard; without it many apps refuse to load there. Turn off only if an app breaks when embedded.";
                ui = lib.neo.ui.mkUi {
                  label = "Show apps inside the dashboard";
                  group = lib.neo.ui.groups.advanced;
                };
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
