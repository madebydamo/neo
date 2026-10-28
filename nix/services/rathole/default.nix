# Rathole client service implementation.
{...}: {
  flake.modules.nixos.rathole = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.rathole;
      # Deliver HTTPS to SWAG's PROXY-protocol listener so streamproxy can pass real client IPs.
      swagHttpsPP = config.neo.services.swag.localHttpsProxyProtocolPort;
      extraServiceStanzas = concatStringsSep "\n" (
        mapAttrsToList (svcName: svc: ''

          [client.services.${cfg.name}_${svcName}]
          token = "${cfg.token}"
          local_addr = "${svc.localAddr}"
        '')
        cfg.extraServices
      );
      configText = ''
        [client]
        remote_addr = "${cfg.remoteAddr}:${toString cfg.port}"

        [client.services.${cfg.name}_http]
        token = "${cfg.token}"
        local_addr = "127.0.0.1:80"
        ${optionalString (!cfg.certificateOnly) ''

          [client.services.${cfg.name}_https]
          token = "${cfg.token}"
          local_addr = "127.0.0.1:${toString swagHttpsPP}"
        ''}
        ${extraServiceStanzas}
      '';
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = !cfg.certificateOnly || lib.neo.privateHostnameDnsActive config;
            message = "neo.services.rathole.certificateOnly requires Pi-hole with localIP or Tailscale split DNS, so services remain reachable on LAN or the tailnet after public HTTPS is dropped.";
          }
        ];
        # The token is a secret: rendered at activation, handed to the
        # DynamicUser service as a credential.
        neo.core.secrets.templates."rathole-client.toml" = {
          content = configText;
          restartUnits = ["rathole.service"];
        };

        systemd.services.rathole = {
          description = "Rathole client tunnel";
          after = ["network-online.target"];
          wants = ["network-online.target"];
          wantedBy = ["multi-user.target"];
          serviceConfig = {
            LoadCredential = "config.toml:${config.neo.core.secrets.templates."rathole-client.toml".path}";
            ExecStart = "${pkgs.rathole}/bin/rathole --client %d/config.toml";
            Restart = "always";
            RestartSec = 5;
            DynamicUser = true;
          };
        };
      };
    };
}
