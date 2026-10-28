# Docmost (open-source collaborative wiki / Notion alternative) service options.
# Production layout: app + Postgres + Redis. UI behind tinyauth; /api/health for probes.
# With sso (default) the docmost-sso bridge signs tinyauth users into Docmost;
# with mcp (default) docmost-mcp serves /mcp with OAuth sign-in through tinyauth.
{...}: {
  flake.modules.nixos.docmost-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      ssoGroup = lib.neo.ui.mkGroup {
        id = "sso";
        label = "Single sign-on";
        description = "Sign tinyauth users into Docmost; admin role by user or group";
        icon = lib.neo.ui.icons.key;
        rank = 50;
      };
      tinyauthCfg = config.neo.services.tinyauth;
      docmostCfg = config.neo.services.docmost;
      ssoActive = docmostCfg.sso && docmostCfg.auth.enabled && tinyauthCfg.enabled;
      mcpActive = ssoActive && docmostCfg.mcp;
    in {
      options.neo.services.docmost = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Docmost collaborative documentation platform" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3000;
                description = "Internal port Docmost listens on (container port 3000)";
              };
              appSecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = ''
                  APP_SECRET: long random secret (32+ characters) used by Docmost for signing and encryption.
                  Generate once with the helper (openssl rand -hex 32) and keep stable; the app refuses to start with the default placeholder.
                '';
                helper = lib.neo.helpers.randomToken;
              };
              dbPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Password for the internal Postgres container (POSTGRES_PASSWORD / DATABASE_URL)";
                helper = lib.neo.helpers.randomToken;
              };
              sso = mkOption {
                type = types.bool;
                default = true;
                rank = 30;
                description = ''
                  Sign tinyauth users straight into Docmost (no second login). Accounts are created on
                  first visit and join the "Everyone" group, so all users can edit the shared spaces.
                  Docmost's own password, invite and setup pages are disabled. Needs tinyauth.
                '';
                ui = lib.neo.ui.mkUi {
                  group = ssoGroup;
                  summary = true;
                };
              };
              admins = mkOption {
                type = types.listOf types.str;
                default = [];
                rank = 31;
                description = ''
                  Tinyauth usernames or emails that get the Docmost admin role at sign-in.
                  With admins and adminGroups both empty, the first user to sign in becomes admin
                  and roles are left to the Docmost UI; otherwise everyone else is set to member.
                '';
                ui = lib.neo.ui.mkUi {
                  group = ssoGroup;
                  visibleWhen = "sso";
                };
              };
              adminGroups = mkOption {
                type = types.listOf types.str;
                default = [];
                rank = 32;
                description = ''
                  OAuth groups (as in tinyauth oauthGroups; for Entra the display name or object ID)
                  whose members get the Docmost admin role at sign-in.
                '';
                ui = lib.neo.ui.mkUi {
                  group = ssoGroup;
                  visibleWhen = "sso";
                };
              };
              mcp = mkOption {
                type = types.bool;
                default = true;
                rank = 40;
                description = ''
                  MCP server at https://<subdomain>.<domain>/mcp so AI clients (Claude, Cursor, …) can search,
                  read and edit pages as the signed-in user. Clients connect with only the URL: OAuth sign-in
                  opens tinyauth in the browser, then asks to approve the client. Needs sso.
                '';
                ui = lib.neo.ui.mkUi {
                  group = lib.neo.ui.groups.assistant;
                  label = "MCP server";
                  summary = true;
                };
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "docmost";
              auth.publicPaths = [
                # Health probe for smoke tests and monitors (no session cookie).
                "^/api/health$"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              docmost = "docmost/docmost:latest";
              "docmost-db" = "postgres:18";
              "docmost-redis" = "redis:8";
              # Tinyauth sign-in bridge (sso.py); only run while sso is active.
              "docmost-sso" = "python:3.13-alpine";
              # MCP server with tinyauth OAuth (mcp.py); only run while mcp is active.
              "docmost-mcp" = "python:3.13-alpine";
            }
            // {
              systemdUnits = mkOption {
                type = types.listOf types.str;
                default =
                  ["docker-docmost" "docker-docmost-db" "docker-docmost-redis"]
                  ++ optionals ssoActive ["docker-docmost-sso" "docmost-sso-setup"]
                  ++ optionals mcpActive ["docker-docmost-mcp"];
                internal = true;
                description = "Systemd units of Docmost and, with tinyauth SSO, its sign-in bridge and MCP server.";
              };
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/docmost"
            // lib.neo.mkServiceMeta {
              category = "Files";
              icon = ./icon.png;
              description = ''
                Docmost is an open-source collaborative wiki and knowledge base (Notion-style) with real-time page editing over WebSockets.
                Neo runs the official Docker stack: the Docmost app, Postgres for workspace data, and Redis for queues/sessions, all on the internal network behind SWAG and tinyauth.
                Set APP_URL to the public HTTPS URL so links and editor clients work; the health endpoint bypasses tinyauth while the UI stays protected.
                Ideal for self-hosted team docs without sending content to a SaaS wiki host.
              '';
              projectUrl = "https://docmost.com/docs";
              githubUrl = "https://github.com/docmost/docmost";
              releaseUrl = "https://github.com/docmost/docmost/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Docmost service configuration";
      };
    };
}
