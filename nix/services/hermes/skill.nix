# Hermes skill for hermes.
{...}: {
  flake.modules.nixos.hermes-skill = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.hermes;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.hermes.skill.conf = lib.neo.mkServiceSkill {
      service = "hermes";
      inherit cfg domain;
      description = "Hermes agent, skills, dashboard, gateway, memory";
      tags = ["neo" "hermes" "ai"];
      title = "Neo · Hermes Agent";
      body = ''
        ## When to Use
        Hermes itself: dashboard, gateway, Telegram, skills, memory, HERMES_HOME.

        ## Architecture notes
        - User: `hermes` (wheel + docker, passwordless sudo)
        - HERMES_HOME: `<stateDir>/.hermes` (default under appdata/hermes)
        - Workspace (terminal cwd): `<stateDir>/workspace` — generated **AGENTS.md**
        - SOUL.md identity: `HERMES_HOME/SOUL.md` (seeded once by Neo unless forceSoul)
        - Managed skills: `skills.external_dirs` store tree of `/neo-*` skills (rebuild-stable)
        - Gateway is local API; dashboard is SWAG + tinyauth + internal basic-auth auto-login

        ## CLI & tooling
        ```bash
        sudo -u hermes env HERMES_HOME=<stateDir>/.hermes hermes --help
        # Skills UI in dashboard, or hermes skills list with HERMES_HOME set
        ```
        When Hermes is enabled, Neo also installs agent-oriented host CLIs
        on system PATH and on the hermes user/gateway/dashboard PATH:
        python3, pip, uv, node/npm/npx, ffmpeg, convert (ImageMagick), pdftoppm,
        tesseract, jq, yq, rg, fd, tree, htmlq, gron, jo, bat, eza, gh,
        docker-compose, … Ad-hoc Python packages: `uv venv` / `uv pip` in
        workspace, or `pip install --user` (HOME is writable). Do not pip-install
        into the Nix store.

        ## Credentials
        - Required: `services.hermes.dashboardPassword` (Generate helper); optional `gatewayToken`, Telegram bot token
        - LLM: optional API keys (`xaiApiKey`, `anthropicApiKey`, `openaiApiKey`) **or** OAuth (no key)
        - xAI OAuth (no API key): set `modelProvider = "xai-oauth"`, optional `defaultModel`, then:
          `sudo -u hermes env HERMES_HOME=<stateDir>/.hermes hermes auth add xai-oauth`
        - Optional pins: `defaultModel`, `modelProvider` — leave empty so Nix does not overwrite model.*
        - Hermes NixOS managed mode blocks dashboard model/config saves; pin via Neo settings or OAuth CLI
        - Read from `/etc/neo/settings.toml` or Neo UI — do not invent keys
        - Dashboard internal basic auth is auto-posted by SWAG; operators use tinyauth only

        ## Procedures
        1. Health-check units (see derived cheatsheet)
        2. Confirm AGENTS.md present in workspace
        3. Confirm Neo skills appear (`/neo-homeserver`, `/neo-*`)
        4. Config changes: edit hermes options in settings → activate
        5. OAuth-only: clear API keys, set modelProvider if needed, run `hermes auth add …` as user hermes
        6. `superviseUpdates`: after system/docker updater runs that changed something, `neo-hermes-supervise` classifies logs (`/neo-update-supervisor`). Needs a home channel (`/sethome`) or `telegramAllowedUserId` fallback

        ## Pitfalls
        - Clearing hermes appdata wipes memory, sessions, local skills, and custom SOUL
        - Agent-created skills under HERMES_HOME/skills override external Neo skills with the same name
        - Managed Neo skills are replaced on rebuild — do not edit the store path
        - Setting `xaiApiKey` forces provider `xai` (API key path), not `xai-oauth`
        - Dashboard cannot switch models under Nix managed mode — use Neo options or OAuth CLI

        ## Verification
        - Both units active; dashboard reachable behind tinyauth; chat responds
      '';
    };
  };
}
