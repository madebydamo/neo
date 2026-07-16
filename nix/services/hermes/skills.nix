# Hermes skills collector + always-on Neo curriculum (AGENTS.md, SOUL.md).
# Materializes enabled services' skill.conf into a store tree pointed at by
# skills.external_dirs. Parallel to how SWAG collects proxyConf.
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

    skillServices = neo.getSkillServices config;
    skillConfs = lib.mapAttrsToList (_: svc: svc.skill.conf) skillServices;

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
        body = ''
          ## When to Use
          Platform questions, disk layout, plugins, reverse proxy model, how to apply config, rollbacks.

          ## Mental model
          1. Operator edits **settings.toml** (Neo web UI or editor).
          2. NixOS modules under `neo.services.*` turn options into containers, units, SWAG conf, and volumes.
          3. **`neo activate`** (or web UI Apply) builds and switches the system generation.
          4. App data lives under Neo volumes — never only in the Nix store.

          ## Volumes (this machine)
          | Volume | Path |
          |--------|------|
          | root | `${volumes.root}` |
          | data | `${volumes.data}` |
          | appdata | `${volumes.appdata}` |
          | media | `${volumes.media}` |
          | documents | `${volumes.documents}` |

          Service appdata is typically `${volumes.appdata}/<service>`.

          ## Edge traffic
          - **SWAG** terminates TLS on 80/443 (or streamproxy tunnel ports).
          - **tinyauth** is the first login gate for most UIs (`auth_request`).
          - Backends sit on Docker network **`internal`** (or host units via `host.docker.internal`).
          - Domain: ${
            if domain != null
            then domain
            else "(services.swag.domain unset)"
          }

          ## How to change configuration safely
          1. Prefer Neo web UI (`neo` service / `https://neo.<domain>`).
          2. Or edit settings and run `neo activate` (server profile uses `/etc/neo/settings.toml` when present).
          3. Use `neo --dry-run activate` when unsure.
          4. Never edit `/nix/store`. Do not hand-edit SWAG proxy-confs for durable changes — they are regenerated.
          5. Destructive ops (clear appdata, nuke config, disk wipe) require **explicit user confirmation**.

          ## CLI toolbox
          - `neo --help`, `neo activate`, `neo update`, `neo web`
          - `systemctl status|restart <unit>`
          - `journalctl -u <unit> -b --no-pager`
          - `docker ps`, `docker logs <name>`
          - Hermes managed skills: `/neo-<service>` (see workspace AGENTS.md index)

          ## Plugins
          Extra flakes listed under Neo plugins export `nixosModules.default` into the same `neo.services.*` option space. Treat them like built-in services once enabled.

          ## Rollbacks
          NixOS generations: bad activate → boot previous generation or `nixos-rebuild switch --rollback` (with care). Data in volumes is independent of generations.

          ## Verification
          - Failed units: `systemctl --failed`
          - Containers: `docker ps`
          - HTTPS to service subdomains after tinyauth
        '';
      };
      references = {};
      scripts = {};
    };

    allSkillConfs = skillConfs ++ [homeserverSkill];

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

    agentsMd = ''
      # Neo homeserver (this machine)

      Generated by Neo for Hermes. Deep procedures live in `/neo-*` skills — load them before operating a service.

      ## What Neo is
      Declarative NixOS homeserver: **settings.toml → neo.services.* modules → containers/units/SWAG/volumes**.
      Day-to-day: Neo web UI. Apply changes with **Activate** or `neo activate`.

      ## Volumes
      | Volume | Path |
      |--------|------|
      | root | `${volumes.root}` |
      | data | `${volumes.data}` |
      | appdata | `${volumes.appdata}` |
      | media | `${volumes.media}` |
      | documents | `${volumes.documents}` |

      Hermes state: `${cfg.stateDir}` (HERMES_HOME = `${cfg.stateDir}/.hermes`).
      Managed Neo skills are published via Hermes `skills.external_dirs` (store path; rebuild-stable).

      ## Domain & edge
      - Domain: ${
        if domain != null
        then domain
        else "(unset — configure services.swag.domain)"
      }
      - Edge: SWAG (TLS) → tinyauth (most UIs) → service on Docker `internal` or host
      - Only 80/443 need to be reachable for normal web access (or streamproxy tunnel)

      ## How to change configuration
      1. Neo web UI or edit settings.toml
      2. `neo activate` (prefer `neo --dry-run activate` first when unsure)
      3. Never edit `/nix/store`; do not permanently hand-edit generated SWAG confs
      4. Confirm with the user before destructive actions (clear appdata, nuke, disk ops)

      ## Enabled services
      | Service | Subdomain | Units | URL | Skill |
      |---------|-----------|-------|-----|-------|
      ${serviceTableRows}
      ## Neo skills index
      | Skill | Description |
      |-------|-------------|
      ${skillIndexRows}
      ## Global ops cheatsheet
      - Logs: `journalctl -u <unit> -b --no-pager` / `docker logs <container>`
      - Restart: `systemctl restart <unit>`
      - System updates: `/neo-system-updater` · `neo update && neo activate`
      - Container images: `/neo-docker-updater`
      - Backup: `/neo-backup`
      - Full architecture: `/neo-homeserver`
    '';

    soulMd = ''
      # Identity

      You are the AI co-pilot for **this Neo homeserver**. You help the operator manage services, diagnose issues, and apply safe changes. Real user data lives on this machine.

      ## Style
      - Be direct, practical, and operationally careful.
      - Prefer verified commands and Neo-documented paths over guessing.
      - Load the relevant `/neo-*` skill before deep work on a service.
      - Keep secrets out of chat when possible; point at settings keys and files instead.

      ## Defaults
      - Read-only diagnosis first (status, logs), then propose a fix.
      - Durable config changes go through **settings.toml + neo activate** (or Neo web UI), not one-off container hacks.
      - For app-native API tokens Neo does not store, guide the user to create them in the app UI.

      ## Avoid
      - Destructive actions (wipe appdata, nuke config, disk format, mass deletes) without **explicit** user confirmation.
      - Editing `/nix/store` or treating generated SWAG confs as permanent.
      - Dumping password vaults, full `.env` files, or unrelated secrets into chat.
      - Inventing credentials or claiming Neo has tokens it does not manage.
    '';

    soulFile = pkgs.writeText "neo-hermes-SOUL.md" soulMd;
  in {
    config = lib.mkIf cfg.enabled {
      services.hermes-agent.settings.skills.external_dirs = [skillsTree];
      services.hermes-agent.documents."AGENTS.md" = lib.mkForce agentsMd;

      system.activationScripts.hermes-neo-soul = lib.stringAfter ["users" "hermes-agent-setup"] (
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
