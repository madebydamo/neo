# Vikunja service options.
# Nix package (pkgs.vikunja / services.vikunja), not an OCI image.
# First user to sign up is the admin. Registration is off after that.
# UI stays behind tinyauth. Apps, CalDAV, feeds, and health bypass via publicPaths.
{...}: {
  flake.modules.nixos.vikunja-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.vikunja = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Vikunja task manager (Todoist-compatible, self-hosted)" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 3456;
                description = "Internal port Vikunja listens on (binds 127.0.0.1; SWAG via host.docker.internal + DNAT)";
              };
              allowRegistration = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = ''
                  Allow anyone who can reach the site to create an account.
                  The first user is the admin even when this is off, as long as no users exist yet.
                '';
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "tasks";
              # GET / stays behind tinyauth (302). Official apps use /api with
              # Vikunja's own JWT or API tokens. CalDAV is /dav (not the site
              # root) and uses HTTP Basic or a CalDAV token.
              auth.publicPaths = [
                # Smoke tests and monitors (no session cookie).
                "^/health$"
                "^/api/v1/info$"
                # Apps, CLI, websockets, and the SPA's API calls.
                "^/api/"
                # CalDAV clients and well-known discovery.
                "^/dav"
                "^/\\.well-known/caldav"
                # Notification feeds authenticate with HTTP Basic.
                "^/feeds"
              ];
            }
            // lib.neo.mkSystemdUnits ["vikunja"]
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/vikunja"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = "https://vikunja.io/images/vikunja-logo.svg";
              description = ''
                Vikunja is the open-source task manager closest to Todoist: projects, labels, priorities, repeating tasks, natural-language quick add, saved filters, sharing, and list / Kanban / Gantt / table views.
                It is installed from nixpkgs (pkgs.vikunja) and run by the NixOS module services.vikunja. The process binds 127.0.0.1; SWAG reaches it through host.docker.internal. SQLite and uploaded files live in Neo appdata (<appdata>/vikunja).
                The web UI is behind tinyauth. Official apps, /api, CalDAV (/dav), well-known discovery, notification feeds, and /health bypass edge auth and use Vikunja's own accounts or tokens.
                Import Todoist, Microsoft To Do, Trello, TickTick, or CSV from Settings. Neo already serves calendars from RustiCal; this is the task side.
              '';
              projectUrl = "https://vikunja.io";
              githubUrl = "https://github.com/go-vikunja/vikunja";
              releaseUrl = "https://github.com/go-vikunja/vikunja/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Vikunja task manager";
      };
    };
}
