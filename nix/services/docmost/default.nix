# Docmost service implementation (app + Postgres + Redis).
# UI behind tinyauth via SWAG; /api/health on publicPaths for probes.
# WebSockets for the real-time editor are covered by SWAG proxy.conf (do not re-set Upgrade/Connection).
# With sso + tinyauth, docmost-sso (sso.py) signs tinyauth users in: SWAG sends
# /login there; docmost-sso-setup creates the workspace / service account and
# sets every account's password to HMAC(ssoKey, email) so the bridge can log in.
# With mcp as well, docmost-mcp (mcp.py, imports sso.py) serves /mcp; its OAuth
# authorize step is behind tinyauth and tools log in to Docmost as that user.
# docmost-space-admins (timer) makes workspace admins admins of every space.
{...}: {
  flake.modules.nixos.docmost = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.docmost;
      appdata = cfg.appdata;
      domain = config.neo.services.swag.domain;
      dbPassword = cfg.dbPassword or "";
      tinyauthCfg = config.neo.services.tinyauth;
      ssoActive = cfg.sso && cfg.auth.enabled && tinyauthCfg.enabled;
      mcpActive = ssoActive && cfg.mcp;
      # Derived from appSecret: no extra secret to manage. Must match swag.nix.
      # Hashed at activation when appSecret is encrypted (lib.neo.secrets.sha256).
      ssoKey = lib.neo.secrets.sha256 "${cfg.appSecret or ""}:docmost-sso";
      swagKey = lib.neo.secrets.sha256 "${cfg.appSecret or ""}:docmost-sso-swag";
      # Signs MCP OAuth clients, codes and tokens; a new appSecret revokes them all.
      mcpKey = lib.neo.secrets.sha256 "${cfg.appSecret or ""}:docmost-mcp";
      ssoKeyFile = config.neo.core.secrets.templates."docmost-sso-key".path;
      # Never a real mailbox, so no tinyauth user can map onto it.
      serviceEmail = "docmost-sso@neo.invalid";
      ssoPort = 8080;
      mcpPort = 8081; # Must match swag.nix.
      # A file here means Neo's SQL failed; sso.py then refuses every sign-in.
      # Holds no error text: psql errors can quote the SSO key.
      sqlStatusDir = "/run/neo/docmost";
      sqlStatusMount = "/run/docmost-sql-status";
      # One transaction; the error goes to the journal, the marker blocks sign-in.
      runSql = {
        marker,
        args ? "",
        sql,
      }: ''
        if ! output=$(docker exec -i docmost-db psql -U docmost -d docmost -q -1 -v ON_ERROR_STOP=1 \
          -v svc=${escapeShellArg serviceEmail} ${args} 2>&1 <<'SQL'
        ${sql}
        SQL
        ); then
          echo "$output" >&2
          touch ${sqlStatusDir}/${marker}
          echo "${marker}: SQL failed, Docmost sign-in is blocked" >&2
          exit 1
        fi
        rm -f ${sqlStatusDir}/${marker}
      '';
    in {
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkPostgresSidecar config {
          name = "docmost-db";
          image = cfg.containers."docmost-db";
          inherit appdata;
          db = "docmost";
          password = dbPassword;
          # The Postgres 18 official image expects the volume here, not at .../data.
          dataPath = "/var/lib/postgresql";
        })
        (lib.neo.mkRedisSidecar config {
          name = "docmost-redis";
          image = cfg.containers."docmost-redis";
          inherit appdata;
          cmd = ["redis-server" "--appendonly" "yes" "--maxmemory-policy" "noeviction"];
        })
        {
          assertions =
            lib.neo.requireSecrets "docmost" cfg [
              {
                name = "appSecret";
                minLength = 32;
              }
              "dbPassword"
            ]
            ++ map (group: {
              assertion = stringLength (lib.trim group) >= 2 && stringLength group <= 100 && !(hasInfix "," group);
              message = "neo.services.docmost.syncGroups: '${group}' must be 2 to 100 characters without a comma (Docmost's group name rules; the sign-in header separates groups by comma).";
            })
            cfg.syncGroups;

          systemd.services.docker-docmost.preStart = lib.neo.mkEnsureDirs config [
            "${appdata}/storage"
          ];

          virtualisation.oci-containers.containers = {
            docmost = {
              image = cfg.containers.docmost;
              environment = {
                APP_URL = lib.neo.publicUrl config cfg;
                REDIS_URL = "redis://docmost-redis:6379";
              };
              volumes = [
                "${appdata}/storage:/app/data/storage"
              ];
              networks = ["internal"];
            };

            docmost-sso = mkIf ssoActive {
              image = cfg.containers."docmost-sso";
              cmd = ["python3" "/app/sso.py"];
              environment = {
                PORT = toString ssoPort;
                DOCMOST_URL = "http://docmost:${toString cfg.port}";
                SERVICE_EMAIL = serviceEmail;
                MAIL_DOMAIN = domain;
                LOGOUT_URL = "${lib.neo.publicUrl config tinyauthCfg}/logout";
                ADMINS = concatStringsSep "," cfg.admins;
                ADMIN_GROUPS = concatStringsSep "," cfg.adminGroups;
                SYNC_GROUPS = concatStringsSep "," cfg.syncGroups;
                SQL_STATUS_DIR = sqlStatusMount;
              };
              volumes = [
                "${./sso.py}:/app/sso.py:ro"
                "${sqlStatusDir}:${sqlStatusMount}:ro"
              ];
              networks = ["internal"];
            };

            docmost-mcp = mkIf mcpActive {
              image = cfg.containers."docmost-mcp";
              cmd = ["python3" "/app/mcp.py"];
              environment = {
                PORT = toString mcpPort;
                PUBLIC_URL = lib.neo.publicUrl config cfg;
                # sso.py (sign-in, Docmost login) reads these (keys: containerEnv).
                DOCMOST_URL = "http://docmost:${toString cfg.port}";
                SERVICE_EMAIL = serviceEmail;
                MAIL_DOMAIN = domain;
                ADMINS = concatStringsSep "," cfg.admins;
                ADMIN_GROUPS = concatStringsSep "," cfg.adminGroups;
                SYNC_GROUPS = concatStringsSep "," cfg.syncGroups;
                SQL_STATUS_DIR = sqlStatusMount;
              };
              volumes = [
                "${./sso.py}:/app/sso.py:ro"
                "${./mcp.py}:/app/mcp.py:ro"
                "${sqlStatusDir}:${sqlStatusMount}:ro"
              ];
              networks = ["internal"];
            };
          };

          # Secrets reach the containers through rendered env files.
          neo.core.secrets.containerEnv =
            {
              docmost = {
                APP_SECRET = cfg.appSecret or "";
                DATABASE_URL = "postgresql://docmost:${dbPassword}@docmost-db:5432/docmost";
              };
            }
            // lib.optionalAttrs ssoActive {
              docmost-sso = {
                SSO_KEY = ssoKey;
                SWAG_KEY = swagKey;
              };
            }
            // lib.optionalAttrs mcpActive {
              docmost-mcp = {
                MCP_KEY = mcpKey;
                SSO_KEY = ssoKey;
                SWAG_KEY = swagKey;
              };
            };

          systemd.tmpfiles.rules = mkIf ssoActive ["d ${sqlStatusDir} 0755 root root -"];

          # The SQL setup below reads the SSO key at runtime.
          neo.core.secrets.templates = mkIf ssoActive {
            "docmost-sso-key" = {
              content = ssoKey;
              restartUnits = ["docmost-sso-setup.service"];
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
              ${runSql {
                marker = "sso-setup";
                args = ''-v key="$(cat ${ssoKeyFile})"'';
                sql = ''
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
                '';
              }}
              # After the SQL: on an existing workspace it makes the service account.
              # Created up front so a space can be shared before anyone signed in.
              docker exec docmost-sso python3 /app/sso.py groups
            '';
          });

          # The service account creates the workspace, so it alone would administer
          # General, and Docmost's space checks ignore the workspace role. A timer
          # rather than the sign-in bridge: sessions last 90 days, and the bridge has
          # no database access and cannot see spaces its account is not in. Grants
          # carry added_by_id = service account, so only those are revoked; a space
          # role given by hand stays.
          systemd.services.docmost-space-admins = mkIf ssoActive {
            description = "Make Docmost workspace admins admins of every space";
            path = [pkgs.docker];
            serviceConfig.Type = "oneshot";
            script = ''
              # Skip only while the database is down or Docmost never migrated it.
              # Anything else is a real failure, such as an upstream schema change.
              docker exec docmost-db pg_isready -q -U docmost -d docmost || exit 0
              migrated=$(docker exec docmost-db psql -U docmost -d docmost -tAc "SELECT to_regclass('kysely_migration') IS NOT NULL")
              [ "$migrated" = t ] || exit 0
              ${runSql {
                marker = "space-admins";
                sql = ''
                  -- The service account is always owner or admin. If it no longer looks
                  -- like one, role names changed: stop before the DELETE revokes everyone.
                  DO $$
                  BEGIN
                    IF EXISTS (SELECT 1 FROM users WHERE email = '${serviceEmail}')
                      AND NOT EXISTS (
                        SELECT 1 FROM users
                        WHERE email = '${serviceEmail}' AND role IN ('owner', 'admin')
                      ) THEN
                      RAISE EXCEPTION 'service account is not owner or admin: Docmost roles changed?';
                    END IF;
                  END $$;
                  DELETE FROM space_members m
                  USING users s
                  WHERE s.email = :'svc'
                    AND m.added_by_id = s.id
                    AND m.user_id <> s.id
                    AND NOT EXISTS (
                      SELECT 1 FROM users a
                      WHERE a.id = m.user_id
                        AND a.role IN ('owner', 'admin')
                        AND a.deleted_at IS NULL
                        AND a.deactivated_at IS NULL
                    );
                  INSERT INTO space_members (user_id, space_id, role, added_by_id)
                  SELECT a.id, sp.id, 'admin', s.id
                  FROM users s
                  JOIN users a ON a.workspace_id = s.workspace_id AND a.id <> s.id
                  JOIN spaces sp ON sp.workspace_id = s.workspace_id
                  WHERE s.email = :'svc'
                    AND a.role IN ('owner', 'admin')
                    AND a.deleted_at IS NULL
                    AND a.deactivated_at IS NULL
                    AND sp.deleted_at IS NULL
                    AND sp.is_personal IS NOT TRUE
                  ON CONFLICT (space_id, user_id) DO UPDATE
                  SET role = 'admin', added_by_id = EXCLUDED.added_by_id, updated_at = now()
                  WHERE space_members.role <> 'admin';
                '';
              }}
            '';
          };

          systemd.timers.docmost-space-admins = mkIf ssoActive {
            wantedBy = ["timers.target"];
            timerConfig = {
              OnCalendar = "minutely";
              AccuracySec = "10s";
            };
          };
        }
      ]);
    };
}
