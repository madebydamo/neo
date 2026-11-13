# Monitoring vhost: Grafana behind tinyauth, plus the ingest endpoints other
# machines' agents write to. Those skip tinyauth and need
# `Authorization: Bearer <ingestToken>` (a rendered snippet: the token is a
# secret). Without a token they answer 403.
#
#   /ingest/loki/api/v1/push        → loki:3100/loki/api/v1/push
#   /ingest/prometheus/api/v1/write → prometheus:9090/api/v1/write
#   /ingest/dashboards/<source>/<uid>.json (PUT, DELETE) → apps/<source>/ (Grafana "Apps")
#
# The check is an `if … return` in the rewrite phase of each exact location;
# no tinyauth include there, so nothing else runs before it. The path rewrite
# comes after swagUpstream's `set`s: `rewrite … break` stops the directives after it.
# proxy.conf already sets Upgrade/Connection and proxy timeouts — do not re-set them.
{...}: {
  flake.modules.nixos.monitoring-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.monitoring;
    tokenSet = cfg.ingestToken != null && cfg.ingestToken != "";
    authOn = cfg.auth.enabled && config.neo.services.tinyauth.enabled;
    check = lib.neo.secrets.swagSnippet config "monitoring-ingest" ''
      if ($http_authorization != "Bearer ${cfg.ingestToken}") {
        return 401;
      }
    '';
    guard =
      if tokenSet
      then check.include
      else "return 403;";
    ingest = path: app: port: target: ''
      location = ${path} {
        ${guard}
        ${lib.neo.swagUpstream {
        inherit app port;
        # After the `set`s: `break` ends the rewrite directives.
        extra = "rewrite ^ ${target} break;\n";
      }}
      }
    '';
  in {
    config.neo.core.secrets.templates = lib.mkIf (cfg.enabled && tokenSet) check.templates;

    # SWAG writes the uploaded app dashboards (as PUID, like appdata).
    config.systemd.services.docker-swag.preStart = lib.mkIf cfg.enabled (lib.mkAfter (lib.neo.mkEnsureDirs config [cfg.appdata "${cfg.appdata}/apps"]));
    config.virtualisation.oci-containers.containers.swag.volumes = lib.mkIf cfg.enabled [
      "${cfg.appdata}/apps:/neo-app-dashboards"
    ];

    config.neo.services.monitoring.proxyConf = lib.mkDefault (lib.neo.mkSwagServer {
      inherit cfg;
      body = ''
        ${ingest "/ingest/loki/api/v1/push" "loki" 3100 "/loki/api/v1/push"}
        ${ingest "/ingest/prometheus/api/v1/write" "prometheus" 9090 "/api/v1/write"}
        # App dashboards from other machines' agents: PUT/DELETE
        # <source>/<uid>.json into apps/ (WebDAV), which Grafana provisions.
        # Quoted: an unquoted `{` in the regex would end the location.
        location ~ "^/ingest/dashboards/(?<neo_src>[a-z0-9][a-z0-9-]{0,62})/(?<neo_dash>app-[a-z0-9-]{1,36}\.json)$" {
          ${guard}
          limit_except PUT DELETE {
            deny all;
          }
          client_max_body_size 8m;
          alias /neo-app-dashboards/$neo_src/$neo_dash;
          dav_methods PUT DELETE;
          create_full_put_path on;
          dav_access user:rw group:r all:r;
        }
        location /ingest/ {
          return 404;
        }

        location / {
          ${lib.neo.swagUpstream {
          app = "grafana";
          port = 3000;
          # Grafana trusts Remote-User (auth proxy). Without the login in front,
          # never pass on a header the visitor sent.
          extra = lib.optionalString (!authOn) ''
            proxy_set_header Remote-User "";
            proxy_set_header Remote-Email "";
            proxy_set_header Remote-Name "";
          '';
        }}
          ${lib.neo.authBlock config cfg}
        }

        ${lib.neo.authLocations config cfg}
      '';
    });
  };
}
