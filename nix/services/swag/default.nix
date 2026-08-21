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
        appServices = lib.neo.getProxiedServices config;
        # (service name, subdomain) for every enabled proxied service.
        subdomainClaims =
          mapAttrsToList (name: svc: {
            service = name;
            subdomain = svc.subdomain;
          })
          appServices;
        subdomains = map (c: c.subdomain) subdomainClaims;
        # Subdomains must be unique DNS labels of lowercase a-z only.
        invalidSubdomains = filter (s: builtins.match "[a-z]+" s == null) subdomains;
        invalidSubdomainMessage =
          concatMapStringsSep "\n" (
            s: let
              owners = filter (c: c.subdomain == s) subdomainClaims;
              names = concatMapStringsSep ", " (c: c.service) owners;
            in "  - \"${s}\" (service: ${names})"
          )
          invalidSubdomains;
        duplicateSubdomains = unique (filter (s: (count (c: c.subdomain == s) subdomainClaims) > 1) subdomains);
        duplicateSubdomainMessage =
          concatMapStringsSep "\n" (
            s: let
              owners = filter (c: c.subdomain == s) subdomainClaims;
              names = concatMapStringsSep ", " (c: c.service) owners;
            in "  - \"${s}\": claimed by ${names}"
          )
          duplicateSubdomains;
        customDomains = concatLists (catAttrs "customDomains" (attrValues appServices));
        proxyPass = cfg.proxyPass;
        proxyPassDomains = attrNames proxyPass;
        domain = cfg.domain;
        appdataSwag = "${config.neo.core.volumes.appdata}/swag";
        ppContainerPort = lib.neo.httpsProxyProtocolContainerPort;
        customProxyConfScripts = flatten (map (
          svc:
            map (
              customDomain:
                lib.neo.mkActivationScriptForFile config {
                  filePath = "${appdataSwag}/nginx/site-confs/${customDomain}.conf";
                  content = ''
                    server {
                      listen 80;
                      listen [::]:80;
                      server_name ${customDomain};

                      return 301 https://$server_name$request_uri;
                    }

                    server {
                      include /config/nginx/listen-https.conf;
                      http2 on;
                      server_name ${customDomain};

                      include /config/nginx/ssl.conf;
                      client_max_body_size 0;
                      include /config/nginx/geo-access.conf;
                      location / {
                        proxy_pass https://127.0.0.1:443;

                        proxy_set_header Host ${svc.subdomain}.${domain};
                        proxy_set_header X-Real-IP $remote_addr;
                        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                        proxy_set_header X-Forwarded-Proto $scheme;
                        proxy_set_header X-Forwarded-Host $host;

                        proxy_ssl_server_name on;
                        proxy_ssl_name ${svc.subdomain}.${domain};
                        proxy_ssl_verify off;

                        proxy_http_version 1.1;
                        proxy_set_header Connection "";
                      }
                    }
                  '';
                }
            ) (svc.customDomains or [])
        ) (attrValues appServices));
        proxyConfScripts = map (
          svc:
            lib.neo.mkActivationScriptForFile config {
              filePath = "${appdataSwag}/nginx/proxy-confs/${svc.subdomain}.subdomain.conf";
              content = svc.proxyConf;
            }
        ) (filter (svc: svc.proxyConf != null) (attrValues appServices));
        proxyPassConfScripts =
          mapAttrsToList (
            domain: upstream:
              lib.neo.mkActivationScriptForFile config {
                filePath = "${appdataSwag}/nginx/site-confs/${domain}.conf";
                content = ''
                  server {
                    listen 80;
                    listen [::]:80;
                    server_name ${domain};

                    return 301 https://$server_name$request_uri;
                  }

                  server {
                    include /config/nginx/listen-https.conf;
                    http2 on;
                    server_name ${domain};

                    include /config/nginx/ssl.conf;
                    client_max_body_size 0;
                    include /config/nginx/geo-access.conf;
                    location / {
                      include /config/nginx/proxy.conf;
                      include /config/nginx/resolver.conf;
                      # Variable so nginx does not resolve the backend at start
                      # (missing hostname → 502 this vhost, not a crash loop).
                      set $neo_upstream "${upstream}";
                      proxy_pass $neo_upstream;

                      proxy_set_header Host $host;
                      proxy_set_header X-Real-IP $remote_addr;
                      proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                      proxy_set_header X-Forwarded-Proto $scheme;
                      proxy_set_header X-Forwarded-Host $host;
                    }
                  }
                '';
              }
          )
          proxyPass;
        edgeConfScripts = [
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdataSwag}/nginx/dbip.conf";
            content = lib.neo.mkDbipConf cfg.geo;
          })
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdataSwag}/nginx/geo-access.conf";
            content = lib.neo.geoAccessConf;
          })
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdataSwag}/nginx/listen-https.conf";
            content = lib.neo.listenHttpsConf;
          })
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdataSwag}/nginx/conf.d/real-ip.conf";
            content = lib.neo.realIpConf;
          })
          # SWAG tinyauth snippets — included by authBlock / authLocations so the
          # dashboard can detect auth via tinyauth-location.conf in proxy-confs.
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdataSwag}/nginx/tinyauth-location.conf";
            content = lib.neo.tinyauthLocationConf;
          })
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdataSwag}/nginx/tinyauth-server.conf";
            content = lib.neo.mkTinyauthServerConf config;
          })
        ];
      in
        mkIf cfg.enabled {
          assertions = [
            {
              assertion = invalidSubdomains == [];
              message = ''
                neo.services.swag: every service subdomain must consist of lowercase a-z only (no digits, hyphens, or uppercase).
                Invalid subdomains:
                ${invalidSubdomainMessage}
              '';
            }
            {
              assertion = duplicateSubdomains == [];
              message = ''
                neo.services.swag: service subdomains must be unique across all enabled proxied services.
                Duplicate subdomains:
                ${duplicateSubdomainMessage}
              '';
            }
          ];

          networking.firewall.extraCommands = "
            iptables -I nixos-fw 1 -i br+ -j ACCEPT
          ";
          networking.firewall.extraStopCommands = "
            iptables -D nixos-fw -i br+ -j ACCEPT
          ";
          networking.firewall.allowedTCPPorts = [cfg.localHttpsProxyProtocolPort];

          systemd.services.docker-swag = {
            preStart = lib.concatStringsSep "\n" ([
                "rm -r ${appdataSwag}/nginx/proxy-confs || true"
                "rm -r ${appdataSwag}/nginx/site-confs || true"
                "rm -f ${appdataSwag}/nginx/proxy.conf || true"
                "rm -f ${appdataSwag}/nginx/nginx.conf || true"
                "/bin/sh -c '${pkgs.docker}/bin/docker network ls --format \"{{.Name}}\" | grep -q \"^internal$\" || ${pkgs.docker}/bin/docker network create internal'"
                (lib.neo.mkEnsureDirs config [
                  appdataSwag
                  "${appdataSwag}/nginx"
                  "${appdataSwag}/nginx/proxy-confs"
                  "${appdataSwag}/nginx/conf.d"
                  "${appdataSwag}/geoip2db"
                ])
              ]
              ++ proxyConfScripts
              ++ customProxyConfScripts
              ++ proxyPassConfScripts
              ++ edgeConfScripts);
            wants = ["swag-patcher.service"];
          };

          virtualisation.oci-containers.containers.swag = {
            image = cfg.containers.swag;
            autoStart = true;
            environment = {
              PUID = toString config.neo.core.uid;
              PGID = toString config.neo.core.gid;
              TZ = "Europe/Zurich";
              URL = cfg.domain;
              SUBDOMAINS = concatStringsSep "," subdomains;
              VALIDATION = "http";
              EMAIL = cfg.email;
              ONLY_SUBDOMAINS = boolToString cfg.onlySubdomains;
              EXTRA_DOMAINS = concatStringsSep "," (proxyPassDomains ++ customDomains);
              SWAG_AUTORELOAD = "true";
              SWAG_AUTORELOAD_WATCHLIST = "/config/etc/letsencrypt";
              DOCKER_MODS = "linuxserver/mods:swag-dashboard|linuxserver/mods:swag-dbip";
            };
            volumes = [
              "${config.neo.core.volumes.appdata}/swag:/config"
            ];
            ports = [
              "${toString cfg.localHttpPort}:80"
              "${toString cfg.localHttpsPort}:443"
              "${toString cfg.localHttpsProxyProtocolPort}:${toString ppContainerPort}"
            ];
            capabilities = {
              NET_ADMIN = true;
            };
            extraOptions = [
              "--network=internal"
              "--add-host=host.docker.internal:host-gateway"
            ];
          };

          systemd.services."swag-cert-reloader" = {
            wantedBy = ["multi-user.target"];
            after = ["docker.service" "docker-swag.service"];
            wants = ["docker-swag.service"];
            path = [pkgs.docker];
            serviceConfig = {
              Type = "simple";
              Restart = "always";
              RestartSec = "10";
            };
            script = ''
              WATCH_DIR="${config.neo.core.volumes.appdata}/swag/etc/letsencrypt/live"
              CONTAINER_NAME="swag"
              NGINX_RELOAD_CMD="nginx -c /config/nginx/nginx.conf -s reload"
              ${builtins.readFile ./swag-cert-reloader.sh}
            '';
          };

          systemd.services."swag-patcher" = {
            after = ["docker-swag.service"];
            requires = ["docker-swag.service"];
            # Re-run when SWAG restarts: preStart deletes nginx.conf, so a
            # one-shot that already "succeeded" would never inject dbip again.
            partOf = ["docker-swag.service"];
            path = [pkgs.docker];
            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
              Restart = "on-failure";
              RestartSec = "5s";
            };
            script = ''
              APPDATA="${config.neo.core.volumes.appdata}/swag"
              NEO_UID="${toString config.neo.core.uid}"
              NEO_GID="${toString config.neo.core.gid}"
              NEO_SUPPORT=${lib.boolToString ((config.neo.services.neo.iframeCookieSupport) && (config.neo.services.neo.enabled))}
              DASHBOARD_SUBDOMAIN="${cfg.subdomain}"
              ${builtins.readFile ./swag-patcher.sh}
            '';
          };
        };
    };
}
