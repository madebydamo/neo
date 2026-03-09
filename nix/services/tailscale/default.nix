# Tailscale service implementation.
{...}: {
  flake.modules.nixos.tailscale = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.tailscale;
    in {
      config = mkIf cfg.enabled {
        services.tailscale.enable = true;
        system.activationScripts.create-tailscale-files = let
          flags =
            []
            ++ (
              if !cfg.acceptDns
              then ["--accept-dns=false"]
              else []
            )
            ++ (
              if cfg.acceptRoutes
              then ["--accept-routes"]
              else []
            )
            ++ (
              if cfg.advertiseExitNode
              then ["--advertise-exit-node"]
              else []
            )
            ++ (
              if cfg.advertiseRoutes != []
              then ["--advertise-routes=${concatStringsSep "," cfg.advertiseRoutes}"]
              else []
            )
            ++ (
              if cfg.exitNode != null
              then ["--exit-node=${cfg.exitNode}"]
              else []
            )
            ++ (
              if cfg.exitNodeAllowLanAccess
              then ["--exit-node-allow-lan-access"]
              else []
            )
            ++ (
              if cfg.hostname != null
              then ["--hostname=${cfg.hostname}"]
              else []
            )
            ++ (
              if cfg.loginServer != "https://controlplane.tailscale.com"
              then ["--login-server=${cfg.loginServer}"]
              else []
            )
            ++ (
              if cfg.ssh
              then ["--ssh"]
              else []
            );
          flagStr = concatStringsSep " " flags;

          checkLoggedIn = "/run/current-system/sw/bin/tailscale status --json 2>/dev/null | /run/current-system/sw/bin/grep -q 'BackendState.*Running'";
        in {
          text = ''
            mkdir -p /etc/tailscale
            ${
              if cfg.authKey != null
              then ''
                echo '${cfg.authKey}' > /etc/tailscale/auth-key
                chmod 600 /etc/tailscale/auth-key
              ''
              else ""
            }
            echo 'if [ -f /etc/tailscale/auth-key ] && ! ${checkLoggedIn}; then' > /etc/tailscale/up.sh
            echo '  /run/current-system/sw/bin/tailscale up --auth-key=file:/etc/tailscale/auth-key ${flagStr}' >> /etc/tailscale/up.sh
            echo 'else' >> /etc/tailscale/up.sh
            echo '  /run/current-system/sw/bin/tailscale up ${flagStr}' >> /etc/tailscale/up.sh
            echo 'fi' >> /etc/tailscale/up.sh
            chmod +x /etc/tailscale/up.sh
          '';
        };

        systemd.services.tailscale-up = {
          description = "Configure Tailscale with user settings";
          wantedBy = ["multi-user.target"];
          after = [
            "tailscaled.service"
            "network.target"
          ];
          serviceConfig = {
            Type = "oneshot";
            ExecStart = "/run/current-system/sw/bin/bash /etc/tailscale/up.sh";
            RemainAfterExit = true;
            Restart = "on-failure";
            RestartSec = 5;
            StartLimitBurst = 3;
            StartLimitIntervalSec = 300;
          };
        };
      };
    };
}
