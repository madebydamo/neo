# Neo web UI service implementation.
# Launches the neo CLI with `web` subcommand (Rocket server) as a systemd service.
# Runs as homeserver user (write access to configPath/settings.toml).
# Listens on configurable port (default 8081) via ROCKET_* env vars; proxied locally via SWAG.
{self, ...}: {
  flake.modules.nixos.neo = {
    config,
    pkgs,
    lib,
    ...
  }: let
    cfg = config.neo.services.neo;
    neoPkg = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
    swagDomain = config.neo.services.swag.domain;
    secRuleBuilder = pkg: name: [
      {
        command = "${pkg}/bin/${name}";
        options = [
          "NOPASSWD"
          "SETENV"
        ];
      }
      {
        command = "/run/current-system/sw/bin/${name}";
        options = [
          "NOPASSWD"
          "SETENV"
        ];
      }
      {
        command = "/nix/store/*-nixos-rebuild-*/bin/${name}";
        options = [
          "NOPASSWD"
          "SETENV"
        ];
      }
    ];
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
        preStart = lib.optionalString (cfg.iframeCookieSupport && swagDomain != null) (lib.neo.mkActivationScriptForFile config {
          filePath = "${config.neo.core.volumes.appdata}/swag/nginx/conf.d/neo-iframe-cookies.conf";
          content = ''
            # Auto-generated because the neo web UI is enabled with iframeCookieSupport.
            # This makes session/auth cookies from all your subdomains work when the pages
            # are loaded inside the neo dashboard iframes (different origin, same registrable domain).
            proxy_cookie_domain ~ .${swagDomain};
            proxy_cookie_flags ~ secure samesite=none;
          '';
        });

        environment = {
          NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
          SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
          ROCKET_ADDRESS = "0.0.0.0";
          ROCKET_PORT = toString cfg.port;
          NEO_HELPER_BASH = "${pkgs.bash}/bin/bash";
          # Explicit tool dirs for option helpers (used ahead of ambient PATH).
          NEO_HELPER_PATH = lib.makeBinPath [
            pkgs.bash
            pkgs.coreutils
            pkgs.openssl
            pkgs.jq
            pkgs.apacheHttpd # htpasswd
            pkgs.whois # mkpasswd (bcrypt + sha-512)
          ];
        };

        path = [
          pkgs.nix
          pkgs.git
          pkgs.coreutils
          pkgs.bash
          pkgs.openssl
          pkgs.jq
          pkgs.apacheHttpd
          pkgs.whois
        ];
      };

      # neo-web runs as homeserver and uses `sudo -n` for privileged ops
      # (units, activate via systemd-run, store repair). Keep NOPASSWD in sync
      # with every binary the web UI may invoke under sudo.
      security.sudo.extraRules = [
        {
          users = ["homeserver"];
          commands =
            (secRuleBuilder pkgs.nixos-rebuild "nixos-rebuild")
            ++ (secRuleBuilder pkgs.systemd "systemctl")
            ++ (secRuleBuilder pkgs.systemd "journalctl")
            ++ (secRuleBuilder pkgs.systemd "systemd-run")
            ++ (secRuleBuilder pkgs.coreutils "rm")
            # Store repair from the web UI (`sudo -n nix-store --verify --repair`).
            ++ (secRuleBuilder pkgs.nix "nix-store")
            ++ [
              {
                command = "/nix/store/*-nix-*/bin/nix-store";
                options = [
                  "NOPASSWD"
                  "SETENV"
                ];
              }
            ];
        }
      ];
    };
  };
}
