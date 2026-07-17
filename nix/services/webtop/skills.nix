# Hermes skill for webtop.
{...}: {
  flake.modules.nixos.webtop-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.webtop;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.webtop.skill.conf = lib.neo.mkServiceSkill {
      service = "webtop";
      inherit cfg domain;
      description = "Browser full desktop (LinuxServer Webtop)";
      tags = ["neo" "webtop" "desktop"];
      title = "Neo · Webtop";
      body = ''
        ## When to Use
        Remote Linux desktop in the browser, install GUI tools, switch distro/DE via image tag.

        ## Architecture notes
        - Image: `lscr.io/linuxserver/webtop:<tag>` (default tag `ubuntu-xfce`)
        - Home/config volume: appdata → `/config`
        - SWAG proxies HTTP **3000** (not the self-signed HTTPS 3001)
        - `--shm-size=1gb` set for Chromium/Electron stability
        - Edge auth via tinyauth (container has no PASSWORD by default)
        - **VPN** (`services.webtop.vpn.enabled`, default false): routes `webtop` through gluetun. Requires the shared `services.vpn` stack. UI still reaches the container via the VPN network alias on `internal`.

        ## Switching distro / desktop (Docker tag only)
        Change `services.webtop.containers.webtop` image to the same repo with another tag, then re-apply.

        settings.toml example:
        ```toml
        [services.webtop]
        enabled = true
        # containers.webtop = "lscr.io/linuxserver/webtop:ubuntu-i3"
        ```

        Or nested:
        ```toml
        [services.webtop.containers]
        webtop = "lscr.io/linuxserver/webtop:debian-kde"
        ```

        Common tags (see https://docs.linuxserver.io/images/docker-webtop/):
        | Tag | Distro + DE |
        |-----|-------------|
        | `latest` | Alpine XFCE |
        | `ubuntu-xfce` | Ubuntu XFCE (Neo default) |
        | `ubuntu-i3` | Ubuntu i3 (lighter) |
        | `ubuntu-mate` | Ubuntu MATE |
        | `ubuntu-kde` | Ubuntu KDE |
        | `debian-xfce` / `debian-i3` / `debian-mate` / `debian-kde` | Debian |
        | `fedora-xfce` / `fedora-i3` / `fedora-mate` / `fedora-kde` | Fedora |
        | `arch-xfce` / `arch-i3` / `arch-mate` / `arch-kde` | Arch |
        | `alpine-i3` / `alpine-kde` / `alpine-mate` | Alpine variants |

        After tag change: rebuild/activate so the container is recreated with the new image.
        Native package installs are **not** persistent across recreate; use `proot-apps install …` for durable apps.

        ## Credentials
        - tinyauth at the edge (keep enabled — desktop has passwordless sudo)
        - Optional container basic auth: CUSTOM_USER / PASSWORD env (not set by Neo by default)

        ## Procedures
        1. Open `https://webtop.<domain>/` (or configured subdomain)
        2. Health-check: `systemctl status docker-webtop`
        3. Mount host data via `additionalMountPoints` if needed
        4. Switch DE: change container image tag → activate → pull/recreate

        ## Pitfalls
        - Do not expose without auth; treat as root-capable session
        - Heavy DEs (KDE) need more RAM/CPU than XFCE/i3
        - Alpine tags: no Nvidia GPU path per LinuxServer docs
        - Blurry text: enable FullColor 4:4:4 in Selkies sidebar
        - VPN on without a healthy `docker-vpn` breaks outbound browser/traffic and may block the session

        ## Verification
        - Desktop loads over HTTPS; keyboard/mouse work; session survives refresh
        - With VPN: open a browser inside the desktop and confirm public IP matches the VPN exit
      '';
    };
  };
}
