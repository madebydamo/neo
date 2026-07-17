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
                description = "Internal port for the Neo web UI (binds to 0.0.0.0 on host; proxied via SWAG)";
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
            // lib.neo.mkReverseProxyOptions {subdomain = "neo";}
            // lib.neo.mkSystemdUnits [
              "neo-web"
            ]
            // lib.neo.mkServiceMeta {
              category = "Core";
              icon = "/static/neo-icon.png";
              description = ''
                The neo web UI gives you a live, in-browser editor for all your homeserver services.
                Changes are written to settings.toml and can be reviewed/applied with a single click.
                Use the sidebar on the left (in the main navigator) to quickly jump between your self-hosted apps.
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
