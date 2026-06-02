# Neo web UI service implementation.
# Launches the neo CLI with `web` subcommand (Rocket server) as a systemd service.
# Runs as homeserver user (to match section=nixos logic and write access to configPath/settings.toml).
# Listens on configurable port (default 8081) via ROCKET_* env vars; proxied locally via SWAG.
{self, ...}: {
  flake.modules.nixos.neo = {
    config,
    pkgs,
    lib,
    ...
  }: let
    cfg = config.neo.services.neo;
    neoPkg = self.packages.${pkgs.system}.neo;
    cookieDomain = ".${config.neo.services.swag.domain}";
  in {
    config = lib.mkIf cfg.enabled {
      systemd.services.neo-web = {
        description = "Neo Homeserver Web UI (config editor)";
        wantedBy = ["multi-user.target"];
        after = ["network-online.target"];
        wants = ["network-online.target"];

        serviceConfig = {
          User = "homeserver";
          Group = "homeserver";
          ExecStart = "${neoPkg}/bin/neo web";
          Restart = "always";
          RestartSec = 5;
        };
        preStart = lib.optionalString cfg.iframeCookieSupport (lib.neo.mkActivationScriptForFile config {
          filePath = "${config.neo.volumes.appdata}/swag/nginx/conf.d/neo-iframe-cookies.conf";
          content = ''
            # Auto-generated because the neo web UI is enabled with iframeCookieSupport.
            # This makes session/auth cookies from all your subdomains work when the pages
            # are loaded inside the neo dashboard iframes (different origin, same registrable domain).
            proxy_cookie_domain ~ ${cookieDomain};
            proxy_cookie_flags ~ secure samesite=none;
          '';
        });

        environment = {
          NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
          SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
          ROCKET_ADDRESS = "0.0.0.0";
          ROCKET_PORT = toString cfg.port;
        };

        path = [
          pkgs.nix
          pkgs.git
          pkgs.coreutils
        ];
      };
    };
  };
}
