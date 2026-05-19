# SWAG reverse proxy service implementation.
{...}: {
  flake.modules.nixos.swag = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; {
      config = let
        cfg = config.neo.services.swag;
        appServices =
          filterAttrs (
            n: v: v.enabled && v.subdomain or null != null && n != "swag"
          )
          config.neo.services;
        subdomains = catAttrs "subdomain" (attrValues appServices);
        customDomains = concatLists (catAttrs "customDomains" (attrValues appServices));
        domain = cfg.domain;
        customProxyConfScripts = flatten (map (
          svc:
            map (
              customDomain:
                lib.neo.mkActivationScriptForFile config {
                  filePath = "${config.neo.volumes.appdata}/swag/nginx/proxy-confs/custom-${customDomain}.conf";
                  content = ''
                    server {
                      listen 443 ssl http2;
                      server_name ${customDomain};
                      include /config/nginx/ssl.conf;

                      location / {
                        proxy_pass https://${svc.subdomain}.${domain};
                        proxy_set_header Host ${svc.subdomain}.${domain};
                        proxy_set_header X-Real-IP $remote_addr;
                        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                        proxy_set_header X-Forwarded-Proto $scheme;
                        proxy_set_header X-Forwarded-Host $host;
                        proxy_ssl_server_name on;
                        proxy_ssl_verify off;
                      }
                    }
                  '';
                }
            ) (svc.customDomains or [])
        ) (attrValues appServices));
        proxyConfScripts = map (
          svc:
            lib.neo.mkActivationScriptForFile config {
              filePath = "${config.neo.volumes.appdata}/swag/nginx/proxy-confs/${svc.subdomain}.subdomain.conf";
              content = svc.proxyConf;
            }
        ) (attrValues appServices);
      in
        mkIf cfg.enabled {
          networking.firewall.extraCommands = "
            iptables -I nixos-fw 1 -i br+ -j ACCEPT
          ";
          networking.firewall.extraStopCommands = "
            iptables -D nixos-fw -i br+ -j ACCEPT
          ";
          systemd.services.docker-swag.preStart = lib.concatStringsSep "\n" ([
              "rm -r ${config.neo.volumes.appdata}/swag/nginx/proxy-confs"
              "/bin/sh -c '${pkgs.docker}/bin/docker network ls --format \"{{.Name}}\" | grep -q \"^internal$\" || ${pkgs.docker}/bin/docker network create internal'"
              (lib.neo.mkActivationScriptForDir config {
                dirPath = "${config.neo.volumes.appdata}/swag/nginx/proxy-confs";
              })
              (lib.neo.mkActivationScriptForDir config {
                dirPath = "${config.neo.volumes.appdata}/swag/nginx";
              })
              (lib.neo.mkActivationScriptForDir config {
                dirPath = "${config.neo.volumes.appdata}/swag";
              })
            ]
            ++ proxyConfScripts
            ++ customProxyConfScripts);
          virtualisation.oci-containers.containers.swag = {
            image = "lscr.io/linuxserver/swag:latest";
            autoStart = true;
            environment = {
              PUID = toString config.neo.uid;
              PGID = toString config.neo.gid;
              TZ = "Europe/Zurich";
              URL = cfg.domain;
              SUBDOMAINS = concatStringsSep "," subdomains;
              VALIDATION = "http";
              EMAIL = cfg.email;
              ONLY_SUBDOMAINS = boolToString cfg.onlySubdomains;
              EXTRA_DOMAINS = concatStringsSep "," (cfg.extraDomains ++ customDomains);
            };
            volumes = [
              "${config.neo.volumes.appdata}/swag:/config"
            ];
            ports = [
              "${toString cfg.localHttpPort}:80"
              "${toString cfg.localHttpsPort}:443"
            ];
            capabilities = {
              NET_ADMIN = true;
            };
            extraOptions = [
              "--network=internal"
              "--add-host=host.docker.internal:host-gateway"
            ];
          };
        };
    };
}
