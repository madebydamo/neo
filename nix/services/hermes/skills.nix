# Hermes skills collector + always-on Neo curriculum (AGENTS.md, SOUL.md).
# Materializes enabled services' skill.conf into a store tree. Activation
# symlinks each non-shadowed skill into HERMES_HOME/skills and into
# HERMES_HOME/neo-external-skills (skills.external_dirs) so one name has one
# resolved path.
# Parallel to how SWAG collects proxyConf.
{...}: {
  flake.modules.nixos.hermes-skills = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.hermes;
    volumes = config.neo.core.volumes;
    domain = config.neo.services.swag.domain or null;
    neo = lib.neo;
    gitCfg = config.neo.core.git;
    gitSshHosts = lib.attrNames gitCfg.knownHosts;

    skillServices = neo.getSkillServices config;
    skillConfs = lib.mapAttrsToList (_: svc: svc.skill.conf) skillServices;

    # Prose in prose/*.md with @name@ placeholders, like guides' body = ./guide.md.
    fill = file: vars:
      builtins.replaceStrings (map (n: "@${n}@") (lib.attrNames vars)) (lib.attrValues vars) (builtins.readFile file);
    domainOr = unset:
      if domain != null
      then domain
      else unset;
    volumeVars = {inherit (volumes) root data appdata media documents;};
    gitVars = {
      gitUserName = gitCfg.userName;
      gitUserEmail = gitCfg.userEmail;
      gitSshKey = neo.gitSshKey;
      gitSshHosts =
        if gitSshHosts == []
        then "no hosts configured"
        else lib.concatStringsSep ", " gitSshHosts;
      gitTokens =
        (
          if gitCfg.tokens == {}
          then "none"
          else lib.concatStringsSep ", " (lib.attrNames gitCfg.tokens)
        )
        + lib.optionalString (gitCfg.tokens ? "github.com") " (`gh` is authenticated through `GH_TOKEN`)";
    };

    # Meta architecture skill always published when Hermes is on.
    homeserverSkill = {
      name = "neo-homeserver";
      description = "Neo homeserver architecture, volumes, apply workflow";
      category = "neo";
      tags = ["neo" "architecture" "homeserver"];
      content = neo.mkSkillMd {
        name = "neo-homeserver";
        description = "Neo homeserver architecture, volumes, apply workflow";
        tags = ["neo" "architecture" "homeserver"];
        includeDerived = false;
        includeCredentialsFooter = true;
        title = "Neo · Homeserver architecture";
        body = fill ./prose/homeserver-skill.md (volumeVars // gitVars // {domain = domainOr "(services.swag.domain unset)";});
      };
      references = {};
      scripts = {};
    };

    updateSupervisorSkill = {
      name = "neo-update-supervisor";
      description = "Classify system/docker updater runs; notify; Docker rollback";
      category = "neo";
      tags = ["neo" "updates" "supervision"];
      content = neo.mkSkillMd {
        name = "neo-update-supervisor";
        description = "Classify system/docker updater runs; notify; Docker rollback";
        tags = ["neo" "updates" "supervision"];
        includeDerived = false;
        includeCredentialsFooter = false;
        title = "Neo · Update supervisor";
        body = fill ./prose/update-supervisor-skill.md {inherit (volumes) appdata;};
      };
      references = {};
      scripts = {};
    };

    # Every guide (neo.guides) as a skill, so "Do it with AI" can load it.
    # {{domain}} / {{service.<name>.url}} filled like the web UI does.
    services = config.neo.services;
    serviceUrls = lib.filterAttrs (_: v: v != null) (lib.mapAttrs (_: svc: let
      sub = svc.subdomain or null;
    in
      if sub == null || domain == null
      then null
      else "https://${sub}.${domain}")
    services);
    fillGuide = text:
      builtins.replaceStrings
      (["{{domain}}"] ++ map (n: "{{service.${n}.url}}") (lib.attrNames serviceUrls))
      ([
          (
            if domain == null
            then "<your domain>"
            else domain
          )
        ]
        ++ lib.attrValues serviceUrls)
      text;
    toolsOn = cfg.neoToolsToken != null && cfg.neoToolsToken != "";
    guideSkills = lib.mapAttrsToList (id: g: let
      name = "neo-guide-${id}";
      description = "Guide: ${g.title}";
      tags = ["neo" "guide"] ++ g.tags;
    in {
      inherit name description tags;
      category = "neo";
      content = neo.mkSkillMd {
        inherit name description tags;
        includeDerived = false;
        includeCredentialsFooter = false;
        title = "Neo guide · ${g.title}";
        body = ''
          ## When to Use
          The user wants to: ${g.title}. ${g.summary}
          Services: ${lib.concatStringsSep ", " g.services}${lib.optionalString (g.optional != []) " (optional: ${lib.concatStringsSep ", " g.optional})"}.

          ## How to run this guide
          1. Check what is already done (neo_settings_get, systemctl, docker ps) and skip those steps; say so.
          2. One step at a time, in plain words. Ask only what you cannot find out yourself (router model, phone type).
          3. ${
            if toolsOn
            then ''Settings: propose them with neo_settings_propose and wait until neo_settings_status is saved or applied. Never send secret values; use op "copy" or "move" when another setting has the value, "generate" where the setting has a Generate button, otherwise "ask".''
            else ''Settings: the neo settings tools are not connected, so do not search for them. Tell the user which setting to change (as an option: link) and to save it and press Apply; never edit settings files yourself.''
          }
          4. After each "Check" block, run its neo-check command (when there is one) and tell the user the result.
          5. Links in the guide: `service:<name>` is a service; `option:<service>.<setting>` is the setting `services.<service>.<setting>` in the neo tools (`option:core.<x>` is `core.<x>`); `guide:<id>` is another guide (skill neo-guide-<id>). Use the same links in your answers, they become clickable.

          ## Guide
          ${fillGuide (builtins.readFile g.body)}
        '';
      };
      references = {};
      scripts = {};
    }) (lib.optionalAttrs config.neo.services.neo.guides (config.neo.guides or {}));

    allSkillConfs =
      skillConfs
      ++ guideSkills
      ++ [homeserverSkill]
      ++ lib.optional cfg.superviseUpdates updateSupervisorSkill;

    skillsTree = pkgs.runCommand "neo-hermes-skills" {} (
      ''
        mkdir -p $out
      ''
      + lib.concatMapStrings (
        conf: let
          skillFile = pkgs.writeText "${conf.name}-SKILL.md" conf.content;
          refCmds = lib.concatStringsSep "\n" (
            lib.mapAttrsToList (rname: rbody: let
              rf = pkgs.writeText "${conf.name}-ref-${rname}" rbody;
            in ''
              mkdir -p $out/${conf.name}/references
              cp ${rf} $out/${conf.name}/references/${rname}
            '')
            (conf.references or {})
          );
          scriptCmds = lib.concatStringsSep "\n" (
            lib.mapAttrsToList (sname: sbody: let
              sf = pkgs.writeText "${conf.name}-script-${sname}" sbody;
            in ''
              mkdir -p $out/${conf.name}/scripts
              cp ${sf} $out/${conf.name}/scripts/${sname}
              chmod +x $out/${conf.name}/scripts/${sname}
            '')
            (conf.scripts or {})
          );
        in ''
          mkdir -p $out/${conf.name}
          cp ${skillFile} $out/${conf.name}/SKILL.md
          ${refCmds}
          ${scriptCmds}
        ''
      )
      allSkillConfs
    );

    enabledServiceNames = lib.sort (a: b: a < b) (
      lib.attrNames (lib.filterAttrs (_: v: v.enabled or false) config.neo.services)
    );

    serviceTableRows =
      lib.concatMapStrings (
        name: let
          svc = config.neo.services.${name};
          sub =
            if (svc.subdomain or null) != null
            then svc.subdomain
            else "-";
          units = lib.concatStringsSep ", " (svc.systemdUnits or []);
          skillName =
            if (svc.skill.conf or null) != null
            then "/${svc.skill.conf.name}"
            else "-";
          url =
            if domain != null && sub != "-"
            then "https://${sub}.${domain}"
            else "-";
        in "| ${name} | ${sub} | ${units} | ${url} | ${skillName} |\n"
      )
      enabledServiceNames;

    skillIndexRows = lib.concatMapStrings (conf: "| `/${conf.name}` | ${conf.description} |\n") (
      lib.sort (a: b: a.name < b.name) allSkillConfs
    );

    agentsMd = fill ./prose/workspace-agents.md (volumeVars
      // {
        inherit (cfg) stateDir;
        domain = domainOr "(unset — configure services.swag.domain)";
        serviceTable = serviceTableRows;
        skillIndex = skillIndexRows;
      });

    soulMd = builtins.readFile ./prose/soul.md;

    soulFile = pkgs.writeText "neo-hermes-SOUL.md" soulMd;

    # Skill names published this generation (for pruning stale Neo-managed symlinks).
    managedSkillNames = lib.sort (a: b: a < b) (map (c: c.name) allSkillConfs);
    managedNamesFile = pkgs.writeText "neo-hermes-managed-skill-names" (
      lib.concatMapStrings (n: n + "\n") managedSkillNames
    );
    # Activation fills this with the non-shadowed store links. external_dirs
    # cannot point at skillsTree itself: a local directory of the same name
    # would be a second resolved path, and skill_view then refuses the name.
    externalSkillsDir = "${cfg.stateDir}/.hermes/neo-external-skills";
  in {
    config = lib.mkIf cfg.enabled {
      services.hermes-agent.settings.skills.external_dirs = [externalSkillsDir];
      services.hermes-agent.documents."AGENTS.md" = lib.mkForce agentsMd;

      # skill_view resolves HERMES_HOME/skills and external_dirs and refuses two resolved paths for one name.
      system.activationScripts.hermes-neo-skills = lib.stringAfter ["users" "hermes-reown" "hermes-agent-setup"] ''
        set -euo pipefail
        hermes_home="${cfg.stateDir}/.hermes"
        skills_dir="$hermes_home/skills"
        external_dir="${externalSkillsDir}"
        tree="${skillsTree}"
        names_file="${managedNamesFile}"

        # True when the symlink text is one this generator created.
        neo_managed() {
          case "$1" in
            /nix/store/*-neo-hermes-skills/*) return 0 ;;
            *) return 1 ;;
          esac
        }

        link_store() {
          ln -sfn "$1" "$2"
          chown -h hermes:hermes "$2"
        }

        # Shadowed names must not stay visible through external_dirs.
        drop_external() {
          ext="$external_dir/$1"
          if [ ! -L "$ext" ] && [ -e "$ext" ]; then
            echo "hermes-neo-skills: $ext is not a Neo symlink; refusing to hide a shadowed skill behind it" >&2
            exit 1
          fi
          if [ -L "$ext" ]; then
            target=$(readlink "$ext" || true)
            if neo_managed "$target"; then
              rm -f "$ext"
            else
              echo "hermes-neo-skills: $ext is not a Neo symlink; refusing to hide a shadowed skill behind it" >&2
              exit 1
            fi
          fi
        }

        mkdir -p "$skills_dir" "$external_dir"
        chown hermes:hermes "$skills_dir" "$external_dir"
        chmod 2770 "$skills_dir"
        chmod 0755 "$external_dir"

        if [ -d "$tree" ]; then
          for src in "$tree"/*; do
            [ -d "$src" ] || continue
            name=$(basename "$src")
            dest="$skills_dir/$name"
            if [ -L "$dest" ]; then
              target=$(readlink "$dest" || true)
              if ! neo_managed "$target"; then
                drop_external "$name"
                continue
              fi
            elif [ -e "$dest" ]; then
              drop_external "$name"
              continue
            fi
            link_store "$src" "$dest"
            link_store "$src" "$external_dir/$name"
          done
        fi

        # Removed skills: only symlinks this generator created.
        prune_neo_links() {
          dir="$1"
          for dest in "$dir"/*; do
            [ -L "$dest" ] || continue
            name=$(basename "$dest")
            target=$(readlink "$dest" || true)
            neo_managed "$target" || continue
            if ! grep -Fxq -- "$name" "$names_file"; then
              rm -f "$dest"
            fi
          done
        }
        prune_neo_links "$skills_dir"
        prune_neo_links "$external_dir"
      '';

      system.activationScripts.hermes-neo-soul = lib.stringAfter ["users" "hermes-reown" "hermes-agent-setup"] (
        if cfg.forceSoul
        then ''
          install -o hermes -g hermes -m 0640 ${soulFile} ${cfg.stateDir}/.hermes/SOUL.md
          touch ${cfg.stateDir}/.hermes/.neo-soul-managed
          chown hermes:hermes ${cfg.stateDir}/.hermes/.neo-soul-managed
        ''
        else ''
          if [ ! -f ${cfg.stateDir}/.hermes/SOUL.md ]; then
            install -o hermes -g hermes -m 0640 ${soulFile} ${cfg.stateDir}/.hermes/SOUL.md
            touch ${cfg.stateDir}/.hermes/.neo-soul-managed
            chown hermes:hermes ${cfg.stateDir}/.hermes/.neo-soul-managed
          fi
        ''
      );
    };
  };
}
