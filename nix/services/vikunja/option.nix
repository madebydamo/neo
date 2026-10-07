# Vikunja service options.
# Nix package (pkgs.vikunja / services.vikunja), not an OCI image.
# First user to sign up is the admin. Registration is off after that.
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
                description = "Host port Vikunja listens on";
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
              # Vikunja has its own accounts, tokens, and CalDAV basic-auth.
              # Edge auth would break the mobile apps and CalDAV clients.
              auth.enabled = false;
            }
            // lib.neo.mkSystemdUnits ["vikunja"]
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = "https://vikunja.io/images/vikunja-logo.svg";
              description = ''
                Vikunja is the open-source task manager closest to Todoist: projects, labels, priorities, repeating tasks, natural-language quick add, saved filters, sharing, and list / Kanban / Gantt / table views.
                It is installed from nixpkgs (pkgs.vikunja) and run by the NixOS module services.vikunja, with SQLite under /var/lib/vikunja.
                Import Todoist, Microsoft To Do, Trello, TickTick, or CSV from Settings. CalDAV is built in, so DAVx5, Apple Reminders, and Thunderbird can sync tasks. Neo already serves calendars from RustiCal; this is the task side.
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
