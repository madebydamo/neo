# Hermes skill for neo.
{...}: {
  flake.modules.nixos.neo-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.neo;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.neo.skill.conf = lib.neo.mkServiceSkill {
      service = "neo";
      inherit cfg domain;
      description = "Neo web UI, settings.toml, activate, CLI";
      tags = ["neo" "core" "cli"];
      title = "Neo · Control plane (web UI + CLI)";
      body = ''
        ## When to Use
        Change homeserver settings, apply config, browse services, run `neo` CLI.

        ## CLI & tooling
        ```bash
        neo --help
        neo web
        neo activate
        neo --dry-run activate
        neo update && neo activate
        neo edit
        ```

        ## Credentials
        - Edge: tinyauth (if enabled on neo service)
        - No app password in Neo options; access is tinyauth + host user `homeserver` for config files

        ## Procedures
        1. **Health**: `systemctl is-active neo-web`
        2. **Edit config**: open Neo UI or `neo edit` / edit settings.toml
        3. **Apply**: Activate in UI or `neo activate`
        4. **Troubleshoot apply**: read neo-web logs; check nix build errors in journal

        ## Pitfalls
        - Do not activate a laptop/local config path against the live server by accident — use server profile on the host
        - `neo nuke` is destructive; require confirmation
        - Home-screen / PWA standalone needs publicPaths for `/site.webmanifest` and `/static/favicon/` (browsers fetch the manifest without cookies). After activate, re-add the icon if an old shortcut was created while the manifest was blocked.

        ## Verification
        - UI loads at neo subdomain; activate completes without failed units
        - `curl -sS https://neo.<domain>/site.webmanifest` returns JSON (no tinyauth 302) with `Content-Type: application/manifest+json`
      '';
    };
  };
}
