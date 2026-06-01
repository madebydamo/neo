# Service presentation metadata helper (follows the exact mk*Options pattern of mkReverseProxyOptions / mkVpnOptions).
# Pass the values you want as defaults; they are automatically wired as the defaults for the `meta` submodule.
{lib, ...}: {
  libExtensions.serviceMeta = {
    neo = {
      mkServiceMeta = {
        icon ? null,
        description ? "",
        projectUrl ? null,
        githubUrl ? null,
        releaseUrl ? null,
        screenshots ? [],
      } @ args:
        with lib; {
          meta = mkOption {
            type = types.submodule {
              options = {
                icon = mkOption {
                  type = types.nullOr types.str;
                  default = icon;
                  description = mdDoc "Icon shown in the option pane (emoji or image URL recommended).";
                };
                description = mkOption {
                  type = types.str;
                  default = description;
                  description = mdDoc "Longer human-friendly introduction shown above the settings form.";
                };
                projectUrl = mkOption {
                  type = types.nullOr types.str;
                  default = projectUrl;
                  description = mdDoc "Link to the project's homepage.";
                };
                githubUrl = mkOption {
                  type = types.nullOr types.str;
                  default = githubUrl;
                  description = mdDoc "Link to the GitHub repository.";
                };
                releaseUrl = mkOption {
                  type = types.nullOr types.str;
                  default = releaseUrl;
                  description = mdDoc "Link to releases / changelog.";
                };
                screenshots = mkOption {
                  type = types.listOf (types.submodule {
                    options = {
                      url = mkOption {
                        type = types.str;
                        description = mdDoc "Public image URL for an example screenshot.";
                      };
                      caption = mkOption {
                        type = types.nullOr types.str;
                        default = null;
                        description = mdDoc "Optional short caption for the screenshot.";
                      };
                    };
                  });
                  default = screenshots;
                  description = mdDoc "Example screenshots to show in the option pane.";
                };
              };
            };
            default = args;
            internal = true;
            description = mdDoc "Rich metadata used by the neo web UI to render an introductory header for the service.";
          };
        };
    };
  };
}
