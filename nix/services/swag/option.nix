# SWAG reverse proxy service options.
{...}: {
  flake.modules.nixos.swag-option = {lib, ...}:
    with lib; {
      options.neo.services.swag = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "swag service";
              domain = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Primary domain for swag";
              };
              email = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "LetsEncrypt email for swag";
              };
              extraDomains = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Extra domains for swag";
              };
              onlySubdomains = mkOption {
                type = types.bool;
                default = true;
                description = "Only use subdomains";
              };
              localHttpPort = mkOption {
                type = types.port;
                internal = true;
                default = 80;
                description = "Local HTTP port for SWAG container (overridden to 9980 with streamproxy)";
              };
              localHttpsPort = mkOption {
                type = types.port;
                internal = true;
                default = 443;
                description = "Local HTTPS port for SWAG container (overridden to 9981 with streamproxy)";
              };
            }
            // lib.neo.mkServiceMeta {
              icon = "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/nginx.svg";
              description = ''
                SWAG (Secure Web Application Gateway) is the foundational reverse proxy and SSL termination layer for the Neo homeserver.
                Built on Nginx with integrated Certbot, it automatically provisions and renews trusted SSL certificates from Let's Encrypt or ZeroSSL for the primary domain and all configured subdomains.
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
