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
        subdomains = catAttrs "subdomain" (attrValues appServices);
        customDomains = concatLists (catAttrs "customDomains" (attrValues appServices));
        proxyPass = cfg.proxyPass;
        proxyPassDomains = attrNames proxyPass;
        domain = cfg.domain;
        customProxyConfScripts = flatten (map (
          svc:
            map (
              customDomain:
                lib.neo.mkActivationScriptForFile config {
                  filePath = "${config.neo.volumes.appdata}/swag/nginx/site-confs/${customDomain}.conf";
                  content = ''
                    server {
                      listen 80;
                      listen [::]:80;
                      server_name ${customDomain};

                      # Redirect HTTP to HTTPS
                      return 301 https://$server_name$request_uri;
                    }

                    server {
                      listen 443 ssl;
                      http2 on;
                      server_name ${customDomain};

                      include /config/nginx/ssl.conf;
                      client_max_body_size 0;

                      location / {
                        # include /config/nginx/proxy.conf;
                        include /config/nginx/resolver.conf;

                        proxy_pass https://${svc.subdomain}.${domain}:443;

                        proxy_set_header Host $proxy_host;
                        proxy_set_header X-Real-IP $remote_addr;
                        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                        proxy_set_header X-Forwarded-Proto $scheme;
                        proxy_set_header X-Forwarded-Host $host;

                        proxy_ssl_server_name on;
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
              filePath = "${config.neo.volumes.appdata}/swag/nginx/proxy-confs/${svc.subdomain}.subdomain.conf";
              content = svc.proxyConf;
            }
        ) (attrValues appServices);
        proxyPassConfScripts =
          mapAttrsToList (
            domain: upstream:
              lib.neo.mkActivationScriptForFile config {
                filePath = "${config.neo.volumes.appdata}/swag/nginx/site-confs/${domain}.conf";
                content = ''
                  server {
                    listen 80;
                    listen [::]:80;
                    server_name ${domain};

                    return 301 https://$server_name$request_uri;
                  }

                  server {
                    listen 443 ssl;
                    http2 on;
                    server_name ${domain};

                    include /config/nginx/ssl.conf;
                    client_max_body_size 0;

                    location / {
                      include /config/nginx/proxy.conf;
                      include /config/nginx/resolver.conf;
                      proxy_pass ${upstream};

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
      in
        mkIf cfg.enabled {
          networking.firewall.extraCommands = "
            iptables -I nixos-fw 1 -i br+ -j ACCEPT
          ";
          networking.firewall.extraStopCommands = "
            iptables -D nixos-fw -i br+ -j ACCEPT
          ";
          systemd.services.docker-swag = {
            preStart = lib.concatStringsSep "\n" ([
                "rm -r ${config.neo.volumes.appdata}/swag/nginx/proxy-confs || true"
                "rm -r ${config.neo.volumes.appdata}/swag/nginx/site-confs || true"
                "rm -f ${config.neo.volumes.appdata}/swag/nginx/proxy.conf || true"
                "rm -f ${config.neo.volumes.appdata}/swag/nginx/nginx.conf || true"
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
                (lib.neo.mkActivationScriptForDir config {
                  dirPath = "${config.neo.volumes.appdata}/swag/nginx/conf.d";
                })
              ]
              ++ proxyConfScripts
              ++ customProxyConfScripts
              ++ proxyPassConfScripts);
            wants = ["swag-patcher.service"];
          };

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
              EXTRA_DOMAINS = concatStringsSep "," (proxyPassDomains ++ customDomains);
              SWAG_AUTORELOAD = "true";
              SWAG_AUTORELOAD_WATCHLIST = "/config/etc/letsencrypt";
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
              set -uo pipefail

              WATCH_DIR="${config.neo.volumes.appdata}/swag/etc/letsencrypt/live"
              CONTAINER_NAME="swag"
              NGINX_RELOAD_CMD="nginx -c /config/nginx/nginx.conf -s reload"

              log() { echo "$(date '+%F %T') [swag-cert] $*"; }

              get_latest_mtime() {
                local m
                m=$(find "$WATCH_DIR" -name fullchain.pem -exec stat -c %Y {} + 2>/dev/null | sort -rn | head -1)
                echo "''${m:-0}"
              }

              reload_nginx() {
                  log "Triggering nginx reload in container '$CONTAINER_NAME'..."
                  if docker exec "$CONTAINER_NAME" $NGINX_RELOAD_CMD; then
                      log "nginx reload successful"
                  else
                      log "ERROR: nginx reload failed (container may not be running or nginx config error)"
                  fi
              }

              wait_for_watch_dir() {
                  if [ -d "$WATCH_DIR" ]; then
                      log "Watch directory ready: $WATCH_DIR"
                      return
                  fi
                  log "Directory does not exist yet: $WATCH_DIR — waiting..."
                  while [ ! -d "$WATCH_DIR" ]; do
                      sleep 5
                  done
                  sleep 2
                  log "Watch directory ready: $WATCH_DIR"
              }

              main() {
                  log "Starting SWAG certificate reloader (watching $WATCH_DIR)"
                  last_mtime=0
                  while true; do
                      wait_for_watch_dir

                      current=$(get_latest_mtime)
                      if [ "$current" -gt "$last_mtime" ]; then
                          if [ "$last_mtime" -ne 0 ]; then
                              log "Detected updated certificate (mtime $current > $last_mtime) — reloading"
                              reload_nginx
                          fi
                          last_mtime=$current
                      elif [ "$last_mtime" -eq 0 ] && [ "$current" -gt 0 ]; then
                          last_mtime=$current
                      fi
                      log "Monitoring certificate mtimes (baseline $last_mtime)..."

                      while [ -d "$WATCH_DIR" ]; do
                          sleep 30
                          current=$(get_latest_mtime)
                          if [ "$current" -gt "$last_mtime" ]; then
                              log "Detected updated certificate (mtime $current > $last_mtime)"
                              reload_nginx
                              last_mtime=$current
                          fi
                      done

                      log "Watch directory disappeared — re-watching"
                      sleep 2
                  done
              }

              main "$@"
            '';
          };

          systemd.services."swag-patcher" = {
            after = ["docker-swag.service"];
            requires = ["docker-swag.service"];
            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
            };
            script = ''
              set -uo pipefail

              APPDATA="${config.neo.volumes.appdata}/swag"

              # ====================
              # 1. nginx.conf patcher (conf.d include)
              # ====================
              NGINX_CONF="$APPDATA/nginx/nginx.conf"
              echo "=== Patching nginx.conf ==="

              for i in $(seq 1 60); do
                if [ -f "$NGINX_CONF" ]; then
                  break
                fi
                sleep 1
              done

              if [ -f "$NGINX_CONF" ]; then
                if ! grep -qE '^[[:space:]]*include[[:space:]]+/config/nginx/conf\.d/\*\.conf;' "$NGINX_CONF"; then
                  sed -i '/include \/config\/nginx\/resolver\.conf;/a \    include /config/nginx/conf.d/*.conf;' "$NGINX_CONF"
                  echo "→ Added conf.d include to nginx.conf"
                else
                  echo "→ conf.d include already present"
                fi
              else
                echo "⚠ nginx.conf not found after waiting"
              fi

              # ====================
              # 2. proxy.conf patcher (iframe/embed support)
              # ====================
              PROXY_CONF="$APPDATA/nginx/proxy.conf"
              echo "=== Patching proxy.conf ==="

              for i in $(seq 1 120); do
                if [ -f "$PROXY_CONF" ]; then
                  break
                fi
                sleep 1
              done

              if [ ! -f "$PROXY_CONF" ]; then
                echo "⚠ proxy.conf not found after waiting"
              else
                touch "$PROXY_CONF"
                chown ${toString config.neo.uid}:${toString config.neo.gid} "$PROXY_CONF" || true
                chmod 0664 "$PROXY_CONF" || true

                MARKER="# neo-iframe-embed-support"
                NEO_SUPPORT=${lib.boolToString ((config.neo.services.neo.iframeCookieSupport) && (config.neo.services.neo.enabled))}

                if $NEO_SUPPORT; then
                  if ! grep -q "$MARKER" "$PROXY_CONF"; then
                    printf '%s\n' "$MARKER" "proxy_hide_header X-Frame-Options;" "proxy_hide_header Content-Security-Policy;" >> "$PROXY_CONF"
                    echo "→ Added iframe/embed headers to proxy.conf"
                  else
                    echo "→ iframe/embed support already present"
                  fi
                else
                  sed -i "/$MARKER/d" "$PROXY_CONF" || true
                  sed -i '/proxy_hide_header X-Frame-Options/d' "$PROXY_CONF" || true
                  sed -i '/proxy_hide_header Content-Security-Policy/d' "$PROXY_CONF" || true
                  echo "→ Removed iframe/embed headers from proxy.conf"
                fi
              fi
            '';
          };
        };
    };
}
