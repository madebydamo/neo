# Service presentation metadata helper (follows the exact mk*Options pattern of mkReverseProxyOptions / mkVpnOptions).
# Pass the values you want as defaults; they are automatically wired as the defaults for the `meta` submodule.
{lib, ...}: {
  libExtensions.serviceMeta = {
    neo = {
      mkServiceMeta = {
        icon ? null,
        # One short line for the services grid card (≤ ~80 chars, what you get).
        # Empty: the card shows the first line of `description`.
        tagline ? "",
        description ? "",
        # Operator facts (first login, paths that skip the login page, upgrade
        # cautions); shown collapsed as "Good to know" in the pane.
        notes ? "",
        # Familiar products this replaces ("Google Photos"); search + privacy map.
        replaces ? [],
        # Privacy map categories (docs/design/descriptions-and-guides.md §4).
        privacy ? [],
        projectUrl ? null,
        githubUrl ? null,
        releaseUrl ? null,
        screenshots ? [],
        rank ? null,
        iframeCompatible ? true,
        # Suggested values: Core, Network, Security, Media, Files, Monitoring, Utilities, AI, Other
        category ? "Other",
        # Upstream license, linked from the pane: { spdx; url; openSource ? true; note ? ""; }.
        # openSource = false labels the service "Not fully open source" (open-core,
        # closed parts in the image, a closed service it depends on); `note` says what.
        license ? null,
        # No longer supported: { reason; instead; alternatives ? []; }. Hidden from the
        # services grid and the privacy map unless it is enabled.
        deprecated ? null,
      } @ args:
        with lib; {
          meta = mkOption {
            type = types.submodule {
              options = {
                icon = mkOption {
                  # String first so a site path (`/static/…`) or `https://` URL stays a
                  # string. A path (`./icon.svg`) is the other branch; eval's toJSON
                  # copies it into the Nix store and the web UI serves that file.
                  type = types.nullOr (types.either types.str types.path);
                  default = icon;
                  description = "Icon shown in the web UI. A URL string, or a file path (for example ./icon.svg) that neo serves itself.";
                };
                tagline = mkOption {
                  type = types.str;
                  default = tagline;
                  description = "One short line for the services grid card. Empty: the first line of description.";
                };
                description = mkOption {
                  type = types.str;
                  default = description;
                  description = "Plain-language introduction (what you can do with it) shown above the settings form.";
                };
                notes = mkOption {
                  type = types.str;
                  default = notes;
                  description = "Technical notes for the operator, shown collapsed under Good to know.";
                };
                replaces = mkOption {
                  type = types.listOf types.str;
                  default = replaces;
                  description = "Familiar products this service replaces, e.g. Google Photos.";
                };
                privacy = mkOption {
                  type = types.listOf types.str;
                  default = privacy;
                  description = "Privacy map categories this service keeps at home, e.g. calendar.";
                };
                projectUrl = mkOption {
                  type = types.nullOr types.str;
                  default = projectUrl;
                  description = "Link to the project's homepage.";
                };
                githubUrl = mkOption {
                  type = types.nullOr types.str;
                  default = githubUrl;
                  description = "Link to the GitHub repository.";
                };
                releaseUrl = mkOption {
                  type = types.nullOr types.str;
                  default = releaseUrl;
                  description = "Link to releases / changelog.";
                };
                screenshots = mkOption {
                  type = types.listOf (types.submodule {
                    options = {
                      url = mkOption {
                        type = types.str;
                        description = "Public image URL for an example screenshot.";
                      };
                      caption = mkOption {
                        type = types.nullOr types.str;
                        default = null;
                        description = "Optional short caption for the screenshot.";
                      };
                    };
                  });
                  default = screenshots;
                  description = "Example screenshots to show in the option pane.";
                };
                rank = mkOption {
                  type = types.nullOr types.int;
                  default = rank;
                  description = "Optional sort rank for the neo web UI. Services with a rank are listed first (sorted by rank asc), followed by the rest (by name).";
                };
                iframeCompatible = mkOption {
                  type = types.bool;
                  default = iframeCompatible;
                  description = "Whether this service can be loaded inside an iframe in the neo navigator UI.";
                };
                license = mkOption {
                  type = types.nullOr (types.submodule {
                    options = {
                      spdx = mkOption {
                        type = types.str;
                        description = "SPDX license id or expression, e.g. AGPL-3.0-only or MIT OR Apache-2.0.";
                      };
                      url = mkOption {
                        type = types.str;
                        description = "Link to the license text.";
                      };
                      openSource = mkOption {
                        type = types.bool;
                        default = true;
                        description = "False when the service is not fully open source (open-core, closed parts in the image, or a closed service it relies on).";
                      };
                      note = mkOption {
                        type = types.str;
                        default = "";
                        description = "What is not open source, and the open alternative if there is one.";
                      };
                    };
                  });
                  default = license;
                  description = "License of the upstream project.";
                };
                deprecated = mkOption {
                  type = types.nullOr (types.submodule {
                    options = {
                      reason = mkOption {
                        type = types.str;
                        description = "Why Neo no longer supports it, in one plain sentence.";
                      };
                      instead = mkOption {
                        type = types.str;
                        description = "What to use instead, in one or two plain sentences.";
                      };
                      alternatives = mkOption {
                        type = types.listOf types.str;
                        default = [];
                        description = "Services that replace it, linked from the notice.";
                      };
                    };
                  });
                  default = deprecated;
                  description = "Set when Neo no longer supports this service. Hidden from the services grid unless enabled.";
                };
                category = mkOption {
                  type = types.str;
                  default = category;
                  description = "Category used to group this service in the neo web UI services grid (e.g. Core, Network, Security, Media, Files, Monitoring, Utilities, AI).";
                };
              };
            };
            default = args;
            internal = true;
            description = "Rich metadata used by the neo web UI to render an introductory header for the service.";
          };
        };
    };
  };
}
