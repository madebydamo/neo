# SWAG reverse proxy service options.
{...}: {
  flake.modules.nixos.swag-option = {lib, ...}:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.swag = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "swag service" {rank = 0;};
              domain = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Primary domain for swag";
                rank = 10;
              };
              email = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "LetsEncrypt email for swag";
                rank = 20;
              };
              proxyPass = mkOption {
                type = types.attrsOf types.str;
                default = {};
                description = "Map of extra domains to http upstream URLs (plain http backends) to create direct proxy server blocks for (e.g. { \"octo.example.com\" = \"http://192.168.178.42:8123\"; }). SWAG handles TLS termination; no need if the target already speaks HTTPS.";
                rank = 30;
              };
              onlySubdomains = mkOption {
                type = types.bool;
                default = true;
                description = "Only use subdomains";
                rank = 40;
              };
              localHttpPort = mkOption {
                type = types.port;
                internal = true;
                default = 80;
                description = "Local HTTP port for SWAG container (overridden to 9980 with streamproxy)";
                rank = 50;
              };
              localHttpsPort = mkOption {
                type = types.port;
                internal = true;
                default = 443;
                description = "Local HTTPS port for SWAG container (overridden to 9981 with streamproxy)";
                rank = 60;
              };
            }
            // lib.neo.mkContainerDefinitions {
              swag = "lscr.io/linuxserver/swag:latest";
            }
            // lib.neo.mkServiceMeta {
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/nginx.svg";
              description = ''
                SWAG (Secure Web Application Gateway) is the foundational reverse proxy and SSL termination layer for the Neo homeserver.
                Built on Nginx with integrated Certbot, it automatically provisions and renews trusted SSL certificates from Let's Encrypt or ZeroSSL for the primary domain, all configured subdomains, and domains listed in proxyPass.
                Every other service's public web interface is routed exclusively through SWAG using its extensive library of reverse proxy configurations, enabling centralized HTTPS, optional auth, fail2ban protection, and keeping backends isolated on the internal Docker network.
                As the entry point for all external traffic, SWAG must be configured with your domain and email before other proxied services can be reached securely from the internet. It uses the container image lscr.io/linuxserver/swag:latest.
              '';
              projectUrl = "https://docs.linuxserver.io/general/swag/";
              githubUrl = "https://github.com/linuxserver/docker-swag";
              releaseUrl = "https://github.com/linuxserver/docker-swag/releases";
            };
        };
        default = {};
        description = "Swag service configuration";
      };
    };
}
