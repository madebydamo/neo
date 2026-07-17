# Hermes skill helpers — declarative per-service skills collected by hermes/skills.nix.
# Parallel to reverse-proxy: skills.nix sets skill.conf; hermes materializes SKILL.md trees.
{lib, ...}: {
  libExtensions.skills = {
    neo = rec {
      # Options merged into neo.services.<name> (like mkReverseProxyOptions).
      # Parent rank 200 places skill.* after customDomains (130) and before containers (300).
      mkSkillOptions = {
        # Default skill publication flag when the service is enabled.
        enabled ? true,
      }:
        with lib; {
          skill =
            mkOption {
              type = types.submodule {
                options = {
                  enabled =
                    mkOption {
                      type = types.bool;
                      default = enabled;
                      description = "Publish a Hermes skill for this service when it is enabled";
                    }
                    // {rank = 0;};
                  # Filled by nix/services/<name>/skills.nix (internal, like proxyConf).
                  conf = mkOption {
                    type = types.nullOr (types.submodule {
                      options = {
                        name = mkOption {
                          type = types.str;
                          description = "Skill directory / slash-command name (e.g. neo-paperless)";
                        };
                        description = mkOption {
                          type = types.str;
                          description = "Short skill description (≤60 chars preferred for Hermes index)";
                        };
                        category = mkOption {
                          type = types.str;
                          default = "neo";
                          description = "Hermes metadata category label";
                        };
                        tags = mkOption {
                          type = types.listOf types.str;
                          default = [];
                          description = "Hermes skill tags";
                        };
                        content = mkOption {
                          type = types.str;
                          description = "Full SKILL.md file contents (frontmatter + body)";
                        };
                        references = mkOption {
                          type = types.attrsOf types.str;
                          default = {};
                          description = "Optional references/<name>.md supporting files";
                        };
                        scripts = mkOption {
                          type = types.attrsOf types.str;
                          default = {};
                          description = "Optional scripts/<name> helper scripts";
                        };
                      };
                    });
                    default = null;
                    internal = true;
                    description = "Hermes skill definition; set by skills.nix when the service is enabled";
                  };
                };
              };
              default = {};
              description = "Hermes agent skill published for this Neo service";
            }
            // {rank = 200;};
        };

      getSkillServices = config:
        lib.filterAttrs (
          _: v:
            (v.enabled or false)
            && (v.skill.enabled or true)
            && (v.skill.conf or null) != null
        ) (config.neo.services or {});

      skillPublicUrl = {
        subdomain,
        domain,
      }:
        if domain != null && domain != "" && subdomain != null
        then "https://${subdomain}.${domain}"
        else if subdomain != null
        then "(set services.swag.domain; subdomain=${subdomain})"
        else "n/a (no subdomain)";

      skillCredentialsFooter = ''
        ## Credentials (general)

        - Operator settings live in `/etc/neo/settings.toml` on a full install (server profile), or the config repo path used by `neo`.
        - Prefer the Neo web UI (`https://neo.<domain>`) to view/edit options.
        - Do not invent API tokens Neo does not store — create them in the app UI and, if needed, save a short note in Hermes MEMORY (not in chat logs).
        - Avoid pasting secrets into messaging platforms; use files under appdata when automation needs them.
      '';

      # Auto-derived inventory from a neo.services.<name> cfg (+ optional domain).
      # Safe to call with cfg=null for meta skills (neo-homeserver).
      mkSkillDerivedSection = {
        cfg ? null,
        domain ? null,
        service ? null,
      }: let
        subdomain =
          if cfg == null
          then null
          else (cfg.subdomain or null);
        units =
          if cfg == null
          then []
          else (cfg.systemdUnits or []);
        containers =
          if cfg == null
          then []
          else lib.attrNames (cfg.containers or {});
        appdata =
          if cfg == null
          then null
          else (cfg.appdata or null);
        meta = cfg.meta or {};
        metaDesc = meta.description or "";
        metaCategory = meta.category or null;
        metaProject = meta.projectUrl or null;
        metaGithub = meta.githubUrl or null;
        auth = cfg.auth or null;
        authEnabled =
          if auth == null
          then null
          else (auth.enabled or null);
        publicPaths =
          if auth == null
          then []
          else (auth.publicPaths or []);
        publicUrl = skillPublicUrl {
          inherit subdomain domain;
        };
        unitsStr =
          if units == []
          then "(none declared)"
          else lib.concatStringsSep ", " units;
        containersStr =
          if containers == []
          then "(none / host units)"
          else lib.concatStringsSep ", " containers;
        statusCmd =
          if units == []
          then "# no systemdUnits declared"
          else "systemctl status ${lib.concatStringsSep " " units}";
        journalCmd =
          if units == []
          then "# journalctl -u <unit> -b --no-pager"
          else "journalctl ${lib.concatMapStringsSep " " (u: "-u ${u}") units} -b --no-pager";
        dockerLogs =
          if containers == []
          then ""
          else "\n${lib.concatMapStringsSep "\n" (c: "docker logs ${c} --tail 100") containers}";
        about =
          if metaDesc == ""
          then ""
          else ''
            ## About this service

            ${lib.removeSuffix "\n" metaDesc}
            ${lib.optionalString (metaCategory != null) "- UI category: **${metaCategory}**\n"}${lib.optionalString (metaProject != null) "- Project: ${metaProject}\n"}${lib.optionalString (metaGithub != null) "- GitHub: ${metaGithub}\n"}
          '';
        live =
          if cfg == null
          then ""
          else ''
            ## Live config (this machine)

            | Field | Value |
            |-------|-------|
            | Service | ${
              if service != null
              then service
              else "n/a"
            } |
            | Settings key | `services.${
              if service != null
              then service
              else "<name>"
            }` |
            | Subdomain | ${
              if subdomain != null
              then subdomain
              else "n/a"
            } |
            | Public URL | ${publicUrl} |
            | Domain | ${
              if domain != null
              then domain
              else "unset"
            } |
            | Units | `${unitsStr}` |
            | Containers | `${containersStr}` |
            | Appdata | ${
              if appdata != null
              then "`${appdata}`"
              else "n/a"
            } |
            | Edge auth (tinyauth) | ${
              if authEnabled == null
              then "n/a"
              else if authEnabled
              then "enabled"
              else "disabled"
            } |
            ${lib.optionalString (publicPaths != []) "| Public paths (auth bypass) | ${lib.concatStringsSep ", " (map (p: "`${p}`") publicPaths)} |\n"}
            ## Ops cheatsheet (derived)

            ```bash
            ${statusCmd}
            ${journalCmd}${dockerLogs}
            ```
          '';
      in
        about + live;

      # Build a complete SKILL.md from metadata + markdown body.
      # Pass cfg (+ domain, service) to auto-include About + Live config + ops cheatsheet.
      mkSkillMd = {
        name,
        description,
        category ? "neo",
        tags ? [],
        version ? "1.0.0",
        platforms ? ["linux"],
        requiresToolsets ? ["terminal"],
        # Optional service context for auto-derived sections
        cfg ? null,
        domain ? null,
        service ? null,
        title ? null,
        includeCredentialsFooter ? true,
        includeDerived ? true,
        body,
      }: let
        tagsYaml =
          if tags == []
          then "[neo]"
          else "[${lib.concatStringsSep ", " tags}]";
        platformsYaml = "[${lib.concatStringsSep ", " platforms}]";
        toolsetsYaml = "[${lib.concatStringsSep ", " requiresToolsets}]";
        heading =
          if title != null
          then title
          else if service != null
          then "Neo · ${service}"
          else "Neo · ${name}";
        derived =
          if includeDerived
          then
            mkSkillDerivedSection {
              inherit cfg domain service;
            }
          else "";
        footer =
          if includeCredentialsFooter
          then skillCredentialsFooter
          else "";
      in ''
        ---
        name: ${name}
        description: ${description}
        version: ${version}
        platforms: ${platformsYaml}
        metadata:
          hermes:
            tags: ${tagsYaml}
            category: ${category}
            requires_toolsets: ${toolsetsYaml}
        ---

        # ${heading}

        ${derived}
        ${body}
        ${footer}
      '';

      # Convenience: full skill.conf attr for a neo.services.<name> entry.
      # skills.nix typically only needs: service, cfg, domain, description, body.
      mkServiceSkill = {
        service,
        cfg,
        domain ? null,
        description,
        name ? "neo-${service}",
        category ? "neo",
        tags ? ["neo" service],
        title ? null,
        includeCredentialsFooter ? true,
        body,
        references ? {},
        scripts ? {},
      }: {
        inherit name description category tags references scripts;
        content = mkSkillMd {
          inherit
            name
            description
            category
            tags
            cfg
            domain
            service
            title
            includeCredentialsFooter
            body
            ;
        };
      };
    };
  };
}
