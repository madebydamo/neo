# Mealie (self-hosted recipe manager and meal planner) service options.
# SQLite layout from the upstream install docs (v3.29.0): one container, data at /app/data.
# UI behind tinyauth. /api/ bypasses it so API tokens and phone apps use Mealie's own auth.
# The first boot seeds changeme@example.com / MyPassword. Change that password.
{...}: {
  flake.modules.nixos.mealie-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.mealie = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Mealie recipe manager and meal planner" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 9000;
                description = "Internal port Mealie listens on (API_PORT; upstream says leave this at 9000 in Docker)";
              };
              allowSignup = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = ''
                  ALLOW_SIGNUP: let anyone who can reach the site create an account without an invite.
                  Off by default. This does not create the first user. Mealie seeds changeme@example.com on an empty database.
                '';
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "mealie";
              # API tokens, phone apps, and GET /api/app/about (health).
              # Mealie authenticates these with its own session or API token.
              auth.publicPaths = [
                "^/api/"
              ];
            }
            // lib.neo.mkContainerDefinitions {
              # Pinned: upstream says to read the release notes before upgrading.
              mealie = "ghcr.io/mealie-recipes/mealie:v3.29.0";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/mealie"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.svg;
              description = ''
                Mealie is a self-hosted recipe manager and meal planner. Import a recipe from a URL, keep a family cookbook, plan meals, and build shopping lists.
                Neo runs the official image on the internal network with SQLite under appdata, behind SWAG and tinyauth. The web UI stays behind tinyauth. /api/ bypasses it so API tokens and phone apps use Mealie's own login.
                The image tag is pinned to v3.29.0. Mealie asks you to read the release notes before upgrading.
                The first boot creates changeme@example.com with password MyPassword. Change that password. Public signup stays off unless allowSignup is enabled.
              '';
              projectUrl = "https://docs.mealie.io/";
              githubUrl = "https://github.com/mealie-recipes/mealie";
              releaseUrl = "https://github.com/mealie-recipes/mealie/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Mealie service configuration";
      };
    };
}
