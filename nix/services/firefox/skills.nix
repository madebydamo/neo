# Hermes skill for firefox.
{...}: {
  flake.modules.nixos.firefox-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.firefox;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.firefox.skill.conf = lib.neo.mkServiceSkill {
      service = "firefox";
      inherit cfg domain;
      description = "Browser Firefox desktop (LinuxServer)";
      tags = ["neo" "firefox" "browser" "desktop"];
      title = "Neo · Firefox";
      body = ''
        ## When to Use
        Isolated Firefox in the browser (Selkies), e.g. private browsing from home, geo-unblocking with VPN.

        ## Architecture notes
        - Image: `lscr.io/linuxserver/firefox:latest` (override via `services.firefox.containers.firefox`)
        - Home/config volume: appdata → `/config` (Firefox profile + downloads)
        - SWAG proxies HTTP **3060** (`CUSTOM_PORT`; not the self-signed HTTPS **3061**)
        - WebSocket **8083** (`CUSTOM_WS_PORT`); all three listed in `vpn.ports` for conflict checks
        - `--shm-size=1gb` required for modern sites (e.g. YouTube)
        - Edge auth via tinyauth (container has no PASSWORD by default)
        - Optional start URL / CLI: `services.firefox.firefoxCli` → `FIREFOX_CLI`
        - **VPN** (`services.firefox.vpn.enabled`, default false): routes `firefox` through gluetun. Requires the shared `services.vpn` stack. UI still reaches the container via the VPN network alias on `internal`. Ports avoid webtop (3050/3051/8082) and karakeep (3000).

        ## settings.toml example
        ```toml
        [services.firefox]
        enabled = true
        # title = "Firefox"
        # firefoxCli = "https://www.linuxserver.io/"
        # containers.firefox = "lscr.io/linuxserver/firefox:latest"
        ```

        Or nested:
        ```toml
        [services.firefox.containers]
        firefox = "lscr.io/linuxserver/firefox:latest"
        ```

        Docs: https://docs.linuxserver.io/images/docker-firefox/

        ## Credentials
        - tinyauth at the edge (keep enabled — session has passwordless sudo)
        - Optional container basic auth: CUSTOM_USER / PASSWORD env (not set by Neo by default)

        ## Procedures
        1. Open `https://firefox.<domain>/` (or configured subdomain)
        2. Health-check: `systemctl status docker-firefox`
        3. Mount host data via `additionalMountPoints` if needed
        4. Optional VPN: set `services.firefox.vpn.enabled = true` (and ensure `services.vpn` is healthy)

        ## Pitfalls
        - Do not expose without auth; treat as root-capable session
        - HTTPS at the edge is required for full Selkies features (WebCodecs audio/video)
        - Blurry text: enable FullColor 4:4:4 in Selkies sidebar
        - shm too small → broken heavy sites; keep `--shm-size=1gb`
        - VPN on without a healthy `docker-vpn` breaks outbound browser traffic and may block the session
        - Alpine-based tags: no Nvidia GPU path per LinuxServer docs

        ## Verification
        - Firefox loads over HTTPS; keyboard/mouse work; session survives refresh
        - With VPN: open a tab and confirm public IP matches the VPN exit
      '';
    };
  };
}
