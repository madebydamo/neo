# Neo web UI service implementation.
# Launches the neo CLI with `web` subcommand (Rocket server) as a systemd service.
# Runs as homeserver user (write access to configPath/settings.toml).
# Listens on loopback only; SWAG reaches it via host.docker.internal + DNAT forward.
#
# neo-bootstrap creates that config repo. multi-user wants it so the web UI
# has a repo even when scheduled updates are off.
{self, ...}: {
  flake.modules.nixos.neo = {
    config,
    pkgs,
    lib,
    ...
  }: let
    cfg = config.neo.services.neo;
    neoPkg = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
    swagDomain = config.neo.services.swag.domain;
    serverCfg = config.neo.neo-cli.server;
    # ZFS snapshots of service appdata (and machine restore) in the web UI.
    zfsEnabled = config.boot.zfs.enabled;
    # AppData is root:root 0755 (volume / ZFS dataset). homeserver cannot
    # mkdir a child there. This runs as root and chowns on every run, so a
    # directory left behind by a failed mkdir is repaired.
    ensureConfigRepo = ''
      config_repo=${lib.escapeShellArg serverCfg.configPath}
      if [ ! -d "$config_repo" ]; then
        mkdir -p "$config_repo"
      fi
      chown homeserver:homeserver "$config_repo"
      chmod 0755 "$config_repo"
    '';
  in {
    config = lib.mkIf cfg.enabled (lib.mkMerge [
      (lib.neo.mkDockerToLocalhostForward cfg.port)
      {
        system.activationScripts.neo-bootstrap-config = ensureConfigRepo;

        # Blocking setup (lib.neo.mkSetupService): neo-web requires it and
        # starts only after init finished. One attempt, no restart: a failed
        # init fails neo-web's dependency instead of looping.
        #
        # stopIfChanged/restartIfChanged = false: neo activate holds the system
        # lock for the whole nixos-rebuild switch. A unit-file change would
        # otherwise restart this oneshot mid-switch; neo init then fails with
        # "Blocked: Activation in progress", neo-web (Requires=) stays down, and
        # the leftover failed state survives after the lock is released. Same
        # pattern as neo-auto-update. Re-run on boot or systemctl start.
        systemd.services.neo-bootstrap =
          lib.neo.mkSetupService {
            inherit pkgs;
            name = "neo-bootstrap";
            description = "Bootstrap nixos config git repo";
            blocking = true;
            retryInterval = null;
            wantedBy = ["multi-user.target"];
            before = ["neo-web.service" "multi-user.target"];
            after = ["network-online.target"];
            wants = ["network-online.target"];
            path = [
              neoPkg
              pkgs.git
              pkgs.nix
              pkgs.nixos-rebuild
              pkgs.nixos-install-tools
              pkgs.coreutils
              pkgs.bash
              pkgs.jq
            ];
            environment = {
              NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
              SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
            };
            serviceConfig = {
              Restart = "no";
              User = "homeserver";
              Group = "homeserver";
              ExecStartPre = [
                "+${pkgs.writeShellScript "neo-bootstrap-ensure-config" ''
                  set -euo pipefail
                  ${ensureConfigRepo}
                ''}"
              ];
            };
            script = ''
              ${neoPkg}/bin/neo --profile server init
            '';
          }
          // {
            stopIfChanged = false;
            restartIfChanged = false;
          };

        systemd.services.neo-web = {
          description = "Neo Homeserver Web UI (config editor)";
          wantedBy = ["multi-user.target"];
          requires = ["neo-bootstrap.service"];
          after = ["network-online.target" "neo-bootstrap.service"];
          wants = ["network-online.target"];

          serviceConfig = {
            User = "homeserver";
            Group = "homeserver";
            ExecStart = "${neoPkg}/bin/neo web";
            # /var/lib/neo-web ($STATE_DIRECTORY): version notes. On the root fs,
            # so a Neo data restore does not rewind them.
            StateDirectory = "neo-web";
            Restart = "always";
            RestartSec = 5;
          };
          preStart = lib.optionalString (cfg.iframeCookieSupport && swagDomain != null) (lib.neo.mkActivationScriptForFile config {
            filePath = "${config.neo.core.volumes.appdata}/swag/nginx/conf.d/neo-iframe-cookies.conf";
            content = ''
              # Auto-generated because the neo web UI is enabled with iframeCookieSupport.
              # This makes session/auth cookies from all your subdomains work when the pages
              # are loaded inside the neo dashboard iframes (different origin, same registrable domain).
              proxy_cookie_domain ~ .${swagDomain};
              proxy_cookie_flags ~ secure samesite=none;
            '';
          });

          environment = {
            NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
            SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
            # neo-web PATH is a closed list (no /run/current-system/sw/bin).
            # Container pull/inspect must not PATH-search a bare `docker`.
            DOCKER_BINARY_PATH = "${config.virtualisation.docker.package}/bin/docker";
            ROCKET_ADDRESS = "127.0.0.1";
            ROCKET_PORT = toString cfg.port;
            NEO_HELPER_BASH = "${pkgs.bash}/bin/bash";
            ZFS_BINARY_PATH = lib.optionalString zfsEnabled "${config.boot.zfs.package}/bin/zfs";
            RSYNC_BINARY_PATH = lib.optionalString zfsEnabled "${pkgs.rsync}/bin/rsync";
            # neo flake source this system was built from. neo-web's option extract
            # overrides the host config's locked `neo` input with this path so the
            # Settings UI matches the running generation (lab tests use
            # --override-input without rewriting flake.lock; plain getFlake would
            # still show the locked, pre-lab options).
            NEO_FLAKE_PATH = toString self;
            # Explicit tool dirs for option helpers (used ahead of ambient PATH).
            NEO_HELPER_PATH = lib.makeBinPath [
              pkgs.bash
              pkgs.coreutils
              pkgs.openssl
              pkgs.jq
              pkgs.apacheHttpd # htpasswd
              pkgs.whois # mkpasswd (bcrypt + sha-512)
            ];
          };

          path = [
            pkgs.nix
            pkgs.git
            pkgs.coreutils
            pkgs.bash
            pkgs.openssl
            pkgs.jq
            # secrets.sops.yaml: encrypt (public keys only) and read metadata
            pkgs.sops
            pkgs.apacheHttpd
            pkgs.whois
            config.virtualisation.docker.package
          ];
        };

        # neo-web runs as homeserver and uses `sudo -n` for privileged ops.
        # Keep in sync with every binary the web UI / activate path may invoke under sudo.
        security.sudo.extraRules = lib.neo.mkSudoExtraRules {
          users = ["homeserver"];
          commands =
            [
              # activate (web trigger → systemd-run → neo activate)
              {
                package = pkgs.nixos-rebuild;
                name = "nixos-rebuild";
              }
              # unit status / start / stop / restart (web UI + activate cleanup)
              {
                package = pkgs.systemd;
                name = "systemctl";
              }
              # live logs dialog (journalctl -f)
              {
                package = pkgs.systemd;
                name = "journalctl";
              }
              # web apply/update: spawn neo-activate@ / neo-update@ oneshots
              {
                package = pkgs.systemd;
                name = "systemd-run";
              }
              # clear appdata (stop → rm -rf → start)
              {
                package = pkgs.coreutils;
                name = "rm";
              }
              # store repair (`sudo -n nix-store --verify --repair`)
              {
                package = pkgs.nix;
                name = "nix-store";
              }
              # generation list/switch: `sudo -n nix-env -p … --switch-generation N`
              {
                package = pkgs.nix;
                name = "nix-env";
              }
              # web UI "Regenerate" on the root-owned machine git key (core/git.nix)
              {
                command = "/run/current-system/sw/bin/neo-git-ssh-key";
              }
              # decrypt secrets.sops.yaml / escrow the host key (core/secrets.nix)
              {
                command = "/run/current-system/sw/bin/neo-secrets-root";
              }
              # after profile switch: `/nix/var/nix/profiles/system/bin/switch-to-configuration`
              {
                command = "/nix/var/nix/profiles/system/bin/switch-to-configuration";
              }
              {
                command = "/nix/var/nix/profiles/system-*-link/bin/switch-to-configuration";
              }
            ]
            ++ lib.optionals zfsEnabled [
              # appdata / machine snapshots: snapshot, schedule restore (set/inherit)
              {
                package = config.boot.zfs.package;
                name = "zfs";
              }
              # restore service appdata from <mount>/.zfs/snapshot/<snap>/…
              {
                package = pkgs.rsync;
                name = "rsync";
              }
            ];
        };
      }
    ]);
  };
}
