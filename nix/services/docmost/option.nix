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
              enabled = mkEnableOption "A shared wiki where you write and organize pages together, like Notion" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3000;
                description = "Internal port Docmost listens on (container port 3000)";
              };
              # APP_SECRET (32+ chars) for signing and encryption; Docmost refuses to
              # start with the image's placeholder. Changing it invalidates sessions
              # and re-keys the SSO passwords.
              appSecret = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 10;
                description = "Secret Docmost uses to keep you signed in. Press Generate once and keep it: changing it signs everyone out.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "App secret";};
              };
              # POSTGRES_PASSWORD / DATABASE_URL of the internal Postgres container.
              dbPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                rank = 20;
                description = "Password for Docmost's own database. Press Generate before the first start and keep it: changing it later cuts Docmost off from its data.";
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Database password";};
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
                  Usernames or emails that become Docmost admins when they sign in.
                  With this and Admin groups both empty, the first person to sign in becomes admin and you manage roles in Docmost; otherwise everyone else is a regular member.
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
                # Matched against the OAuth groups tinyauth passes on (oauthGroups).
                description = ''
                  Sign-in groups whose members become Docmost admins.
                  For Microsoft Entra, use the group's display name or object ID.
                '';
                ui = lib.neo.ui.mkUi {
                  group = ssoGroup;
                  visibleWhen = "sso";
                  label = "Admin groups";
                };
              };
              mcp = mkOption {
                type = types.bool;
                default = true;
                rank = 40;
                # Served at https://<subdomain>.<domain>/mcp; OAuth sign-in goes through tinyauth.
                description = ''
                  Lets AI apps (Claude, Cursor, …) search, read and edit your pages as you.
                  Add the Docmost address followed by /mcp in the app, then sign in and approve it in the browser. Needs single sign-on.
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
              "docmost-redis" = "redis:8-alpine";
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
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/docmost/docmost/blob/main/LICENSE";
                openSource = false;
                note = "The core is AGPL licensed, but the official image also contains enterprise code from a private repository, so part of what runs is closed source.";
              };
              icon = ./icon.png;
              tagline = "A shared wiki for notes and how-tos, like Notion";
              description = ''
                Write and organise pages together: how-tos, family recipes, travel plans or team documentation.
                Several people can edit the same page at the same time, and everything is searchable.
              '';
              notes = ''
                Runs the official stack: the Docmost app, Postgres for workspace data and Redis for queues and sessions.
                Neo sets APP_URL to the public HTTPS address. The health endpoint skips tinyauth; the UI stays behind it.
              '';
              replaces = ["Notion" "Confluence"];
              privacy = ["notes"];
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
