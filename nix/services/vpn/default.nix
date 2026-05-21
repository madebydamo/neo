# VPN (gluetun) service implementation.
{...}: {
  flake.modules.nixos.vpn = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.vpn;
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfg.wireguardPrivateKey != null && cfg.wireguardPresharedKey != null && cfg.wireguardAddresses != null;
            message = "You must configure your wireguard tunnel with a private key, preshared key and an address";
          }
        ];
        virtualisation.oci-containers.containers.vpn = {
          image = cfg.image;
          autoStart = true;
          capabilities = {
            NET_ADMIN = true;
          };
          devices = [
            "/dev/net/tun:/dev/net/tun"
          ];
          environment =
            {
              VPN_SERVICE_PROVIDER = cfg.vpnServiceProvider;
              VPN_TYPE = "wireguard";
              WIREGUARD_PRIVATE_KEY = cfg.wireguardPrivateKey;
              WIREGUARD_PRESHARED_KEY = cfg.wireguardPresharedKey;
              WIREGUARD_ADDRESSES = cfg.wireguardAddresses;
              SERVER_COUNTRIES = cfg.serverCountries;
            }
            // optionalAttrs (cfg.firewallVpnInputPorts != null) {
              FIREWALL_VPN_INPUT_PORTS = cfg.firewallVpnInputPorts;
            };
          extraOptions = [
            "--network=internal"
          ];
        };
      };
    };
}

