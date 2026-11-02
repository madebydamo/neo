# Mealie (self-hosted recipe manager and meal planner) service options.
# SQLite layout from the upstream install docs (v3.30.0): one container, data at /app/data.
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
              # ALLOW_SIGNUP. Does not create the first user: Mealie seeds
              # changeme@example.com on an empty database.
              allowSignup = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = "Let anyone who can open Mealie create an account without an invite. Leave off unless you want public sign-ups.";
                ui = lib.neo.ui.mkUi {label = "Allow public sign-up";};
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
              mealie = "ghcr.io/mealie-recipes/mealie:v3.30.0";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/mealie"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              icon = ./icon.svg;
              tagline = "Recipes, meal plans and shopping lists";
              description = ''
                Import recipes from any website with one click, keep a family cookbook, plan the week's meals and turn them into a shopping list.
              '';
              notes = ''
                Runs the official image with SQLite under appdata. The web UI stays behind tinyauth; /api/ skips it so API tokens and phone apps use Mealie's own login.
                The image tag is pinned. Read the release notes before upgrading.
                The first boot creates changeme@example.com with password MyPassword. Change that password. Public signup stays off unless allowSignup is enabled.
              '';
              replaces = ["Paprika"];
              privacy = ["recipes"];
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
