# Docmost service implementation (app + Postgres + Redis).
# UI behind tinyauth via SWAG; /api/health on publicPaths for probes.
# WebSockets for the real-time editor are covered by SWAG proxy.conf (do not re-set Upgrade/Connection).
# With sso + tinyauth, docmost-sso (sso.py) signs tinyauth users in: SWAG sends
# /login there; docmost-sso-setup creates the workspace / service account and
# sets every account's password to HMAC(ssoKey, email) so the bridge can log in.
# With mcp as well, docmost-mcp (mcp.py, imports sso.py) serves /mcp; its OAuth
# authorize step is behind tinyauth and tools log in to Docmost as that user.
{...}: {
  flake.modules.nixos.docmost = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.docmost;
      appdata = "${config.neo.core.volumes.appdata}/docmost";
      domain = config.neo.services.swag.domain;
      secretSet = v: v != null && v != "";
      dbPassword = cfg.dbPassword or "";
      tinyauthCfg = config.neo.services.tinyauth;
      ssoActive = cfg.sso && cfg.auth.enabled && tinyauthCfg.enabled;
      mcpActive = ssoActive && cfg.mcp;
      # Derived from appSecret: no extra secret to manage. Must match swag.nix.
      ssoKey = builtins.hashString "sha256" "${cfg.appSecret or ""}:docmost-sso";
      swagKey = builtins.hashString "sha256" "${cfg.appSecret or ""}:docmost-sso-swag";
      # Signs MCP OAuth clients, codes and tokens; a new appSecret revokes them all.
      mcpKey = builtins.hashString "sha256" "${cfg.appSecret or ""}:docmost-mcp";
      # Never a real mailbox, so no tinyauth user can map onto it.
      serviceEmail = "docmost-sso@neo.invalid";
      ssoPort = 8080;
      mcpPort = 8081; # Must match swag.nix.
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = secretSet cfg.appSecret && (builtins.stringLength cfg.appSecret) >= 32;
            message = "neo.services.docmost: appSecret must be at least 32 characters when enabled.";
          }
          {
            assertion = secretSet cfg.dbPassword;
            message = "neo.services.docmost: dbPassword must be set when enabled.";
          }
        ];

        # Postgres 18 official image expects the volume at /var/lib/postgresql (not .../data).
        systemd.services.docker-docmost-db.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/pgdata"
        ];
        systemd.services.docker-docmost-redis.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/redisdata"
        ];
        systemd.services.docker-docmost.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/storage"
        ];

        virtualisation.oci-containers.containers = {
          docmost-redis = {
            image = cfg.containers."docmost-redis";
            autoStart = true;
            cmd = ["redis-server" "--appendonly" "yes" "--maxmemory-policy" "noeviction"];
            volumes = [
              "${appdata}/redisdata:/data"
            ];
            networks = ["internal"];
          };

          docmost-db = {
            image = cfg.containers."docmost-db";
            autoStart = true;
            environment = {
              POSTGRES_DB = "docmost";
              POSTGRES_USER = "docmost";
              POSTGRES_PASSWORD = dbPassword;
            };
            volumes = [
              "${appdata}/pgdata:/var/lib/postgresql"
            ];
            networks = ["internal"];
          };

          docmost = {
            image = cfg.containers.docmost;
            autoStart = true;
            environment = {
              APP_URL = "https://${cfg.subdomain}.${domain}";
              APP_SECRET = cfg.appSecret or "";
              DATABASE_URL = "postgresql://docmost:${dbPassword}@docmost-db:5432/docmost";
              REDIS_URL = "redis://docmost-redis:6379";
            };
            volumes = [
              "${appdata}/storage:/app/data/storage"
            ];
            networks = ["internal"];
          };

          docmost-sso = mkIf ssoActive {
            image = cfg.containers."docmost-sso";
            autoStart = true;
            cmd = ["python3" "/app/sso.py"];
            environment = {
              PORT = toString ssoPort;
              DOCMOST_URL = "http://docmost:${toString cfg.port}";
              SSO_KEY = ssoKey;
              SWAG_KEY = swagKey;
              SERVICE_EMAIL = serviceEmail;
              MAIL_DOMAIN = domain;
              LOGOUT_URL = "https://${tinyauthCfg.subdomain}.${domain}/logout";
              ADMINS = concatStringsSep "," cfg.admins;
              ADMIN_GROUPS = concatStringsSep "," cfg.adminGroups;
            };
            volumes = ["${./sso.py}:/app/sso.py:ro"];
            networks = ["internal"];
          };

          docmost-mcp = mkIf mcpActive {
            image = cfg.containers."docmost-mcp";
            autoStart = true;
            cmd = ["python3" "/app/mcp.py"];
            environment = {
              PORT = toString mcpPort;
              PUBLIC_URL = "https://${cfg.subdomain}.${domain}";
              MCP_KEY = mcpKey;
              # sso.py (sign-in, Docmost login) reads these.
              DOCMOST_URL = "http://docmost:${toString cfg.port}";
              SSO_KEY = ssoKey;
              SWAG_KEY = swagKey;
              SERVICE_EMAIL = serviceEmail;
              MAIL_DOMAIN = domain;
              ADMINS = concatStringsSep "," cfg.admins;
              ADMIN_GROUPS = concatStringsSep "," cfg.adminGroups;
            };
            volumes = [
              "${./sso.py}:/app/sso.py:ro"
              "${./mcp.py}:/app/mcp.py:ro"
            ];
            networks = ["internal"];
          };
        };

        # Fresh Docmost: create the workspace with the service account as owner.
        # Existing workspace: add the service account as admin in SQL. Then set
        # every account's password to HMAC(ssoKey, email) (pgcrypto, bcrypt like
        # Docmost); accounts made before SSO switch over here. Runs again on
        # every container restart, so a new appSecret re-keys all accounts.
        systemd.services.docmost-sso-setup = mkIf ssoActive (lib.neo.mkSetupService {
          inherit pkgs;
          name = "docmost-sso-setup";
          description = "Prepare Docmost accounts for tinyauth sign-in";
          containers = ["docmost" "docmost-db" "docmost-sso"];
          path = [pkgs.docker];
          retryInterval = 10;
          script = ''
            set -euo pipefail
            docker exec docmost-sso python3 /app/sso.py setup
            docker exec -i docmost-db psql -U docmost -d docmost -q -v ON_ERROR_STOP=1 \
              -v key=${escapeShellArg ssoKey} -v svc=${escapeShellArg serviceEmail} <<'SQL'
            SET client_min_messages = warning;
            CREATE EXTENSION IF NOT EXISTS pgcrypto;
            INSERT INTO users (email, name, role, workspace_id, email_verified_at)
            SELECT :'svc', 'Neo SSO', 'admin', w.id, now()
            FROM (SELECT id FROM workspaces ORDER BY created_at LIMIT 1) w
            WHERE NOT EXISTS (
              SELECT 1 FROM users u WHERE u.email = :'svc' AND u.workspace_id = w.id
            );
            -- pgcrypto only reads $2a$; Docmost's $2b$ hashes are the same bcrypt.
            UPDATE users SET password = crypt(p.pw, gen_salt('bf', 12))
            FROM (
              SELECT id, encode(hmac(lower(email), :'key', 'sha256'), 'hex') AS pw
              FROM users WHERE deleted_at IS NULL
            ) p
            WHERE users.id = p.id
              AND (
                users.password IS NULL
                OR users.password NOT LIKE '$2_$%'
                OR overlay(users.password placing '$2a$' from 1 for 4)
                  <> crypt(p.pw, overlay(users.password placing '$2a$' from 1 for 4))
              );
            SQL
          '';
        });
      };
    };
}
