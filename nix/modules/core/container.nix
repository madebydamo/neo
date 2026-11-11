# Shared policy for every OCI container unit.
#
# Host DNS. Every container snapshots host DNS at start and then keeps that copy.
#
# Docker writes /etc/resolv.conf from the host file when the container is
# created. On bridge networks it then points the container at 127.0.0.11 and
# stores the host nameservers as the embedded resolver's upstreams. That
# rewrite does not update the file hash, so Docker treats the file as
# user-modified and will not pick up a later change to the host file.
# Host-network containers keep the copied nameservers directly.
#
# Tailscale accept-dns and NetBird DNS install those nameservers through
# resolvconf after network-online. Until then the host file is only "options
# edns0" or the DHCP resolver. Containers started in that window (Jellyfin on
# the host network, and any other local container) never learn a nameserver,
# or never learn the private names.
#
# neo-host-dns blocks OCI container start until the host file has a nameserver.
# The same wait runs before every container start, so a later restart cannot
# snapshot a hole while resolvconf is rewriting the file. With Tailscale
# accept-dns or NetBird DNS, the wait also requires that client's resolvconf
# registration (key `tailscale` or `wt0`), so the snapshot is its resolver. Not
# on the machine that hosts the NetBird network: its client connects only after
# SWAG and the control plane run. No DNS option is exposed: local containers
# use the host resolver.
#
# Stop. `docker stop` delivers SIGTERM. A container that does not exit 0 makes
# `docker run` return 143, and systemd records the stop as failure. 143 is
# SIGTERM, so it counts as a clean stop. Other crashes still restart.
{lib, ...}: let
  # Polled by neo-host-dns.service. Tests set the NEO_* variables.
  waitBody = ''
    set -euo pipefail
    resolv="''${NEO_RESOLV_CONF:-/etc/resolv.conf}"
    # Empty: no private network client to wait for.
    key="''${NEO_RESOLVCONF_KEY:-}"
    deadline_sec="''${NEO_KEY_DEADLINE_SEC:-90}"
    max_wait="''${NEO_MAX_WAIT_SEC:-}"

    has_nameserver() {
      grep -qE '^[[:space:]]*nameserver[[:space:]]+[^[:space:]#]+' "$resolv"
    }

    ready() {
      has_nameserver || return 1
      if [ -n "$key" ] && [ ! -e "$key" ]; then
        return 1
      fi
      return 0
    }

    deadline=$((SECONDS + deadline_sec))
    start=$SECONDS
    while ! ready; do
      if [ -n "$key" ] && [ "$SECONDS" -ge "$deadline" ]; then
        if has_nameserver; then
          echo "neo-host-dns: $key did not register within ''${deadline_sec}s; containers will use the current host resolv.conf" >&2
          exit 0
        fi
      fi
      if [ -n "$max_wait" ] && [ $((SECONDS - start)) -ge "$max_wait" ]; then
        echo "neo-host-dns: no nameserver in $resolv" >&2
        exit 1
      fi
      sleep 1
    done
  '';

  nixosModule = {
    config,
    lib,
    pkgs,
    ...
  }: let
    ts = config.neo.services.tailscale;
    # accept-dns is what installs the host nameservers (MagicDNS).
    waitForTailscale = ts.enabled && ts.acceptDns;
    # Only one of them may own host DNS (asserted in the netbird service).
    waitForNetbird =
      (config.neo.services.netbird.enabled or false)
      && (config.neo.services.netbird.acceptDns or false)
      && !(config.neo.services.netbird-server.enabled or false);
    key =
      if waitForTailscale
      then "/run/resolvconf/keys/tailscale"
      else if waitForNetbird
      then "/run/resolvconf/keys/wt0"
      else "";
    waitScriptText = ''
      export NEO_RESOLVCONF_KEY=${key}
      ${waitBody}
    '';
    # Separate process so this does not change shell options in other preStart scripts.
    waitScript = pkgs.writeShellScript "neo-host-dns-wait" waitScriptText;
  in {
    systemd.services = lib.mkMerge [
      {
        neo-host-dns = {
          description = "Wait until host DNS is installed for containers";
          wantedBy = ["multi-user.target"];
          after =
            ["network-online.target"]
            ++ lib.optional waitForTailscale "tailscale-up.service"
            ++ lib.optional waitForNetbird "netbird.service";
          wants =
            ["network-online.target"]
            ++ lib.optional waitForTailscale "tailscale-up.service"
            ++ lib.optional waitForNetbird "netbird.service";
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
            TimeoutStartSec = "infinity";
          };
          script = waitScriptText;
        };
      }
      # serviceName follows the OCI backend (docker-jellyfin, podman-…).
      # ExecStartPre runs on every start, including a later restart.
      (lib.mapAttrs' (
          _: container:
            lib.nameValuePair container.serviceName {
              after = ["neo-host-dns.service"];
              wants = ["neo-host-dns.service"];
              serviceConfig = {
                ExecStartPre = [waitScript];
                # `docker stop` delivers SIGTERM. A container that does not exit 0
                # (selenium, supervisord) makes `docker run` return 143, and systemd
                # records the stop as failure even though the container is gone.
                # 143 is SIGTERM, so this is a clean stop. Other crashes still restart.
                SuccessExitStatus = "143";
              };
            }
        )
        config.virtualisation.oci-containers.containers)
    ];
  };
in {
  flake.modules.nixos.container-host-dns = nixosModule;

  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    script = pkgs.writeShellScript "neo-host-dns-wait" waitBody;
    emptyResolv = pkgs.writeText "resolv-empty" ''
      # Generated by resolvconf
      options edns0
    '';
    magicResolv = pkgs.writeText "resolv-magic" ''
      search duck-squeaker.ts.net
      nameserver 100.100.100.100
      options edns0
    '';
    dhcpResolv = pkgs.writeText "resolv-dhcp" ''
      nameserver 192.168.178.1
      options edns0
    '';
    netbirdResolv = pkgs.writeText "resolv-netbird" ''
      search netbird.selfhosted
      nameserver 127.0.0.153
      options edns0
    '';
    tailscaleResolv = pkgs.writeText "resolv-tailscale" ''
      nameserver 100.100.100.100
      nameserver fd7a:115c:a1e0::53
      options edns0
    '';

    # Minimal NixOS eval: container units must wait for host DNS.
    eval = import "${pkgs.path}/nixos/lib/eval-config.nix" {
      system = "x86_64-linux";
      modules = [
        nixosModule
        {
          options.neo.services.tailscale = lib.mkOption {
            type = lib.types.submodule {
              options = {
                enabled = lib.mkEnableOption "tailscale";
                acceptDns = lib.mkOption {
                  type = lib.types.bool;
                  default = true;
                };
              };
            };
            default = {};
          };

          config = {
            boot.isContainer = true;
            fileSystems."/" = {
              device = "x";
              fsType = "ext4";
            };
            system.stateVersion = "24.11";
            documentation.enable = false;
            virtualisation.docker.enable = true;
            virtualisation.oci-containers.backend = "docker";
            virtualisation.oci-containers.containers.jellyfin.image = "jellyfin:test";
            virtualisation.oci-containers.containers.filebrowser.image = "filebrowser:test";
            neo.services.tailscale.enabled = true;
            neo.services.tailscale.acceptDns = true;
          };
        }
      ];
    };

    jellyfin = eval.config.systemd.services.docker-jellyfin;
    filebrowserWants = eval.config.systemd.services.docker-filebrowser.wants or [];
    dnsScript = eval.config.systemd.services.neo-host-dns.script or "";
    jellyfinPre = lib.concatStringsSep " " (map toString (jellyfin.serviceConfig.ExecStartPre or []));
    orderingOk =
      lib.elem "neo-host-dns.service" (jellyfin.after or [])
      && lib.elem "neo-host-dns.service" filebrowserWants
      && lib.hasInfix "NEO_RESOLVCONF_KEY=/run/resolvconf/keys/tailscale" dnsScript
      && lib.hasInfix "neo-host-dns-wait" jellyfinPre;
    # SIGTERM from docker stop must not leave the unit failed.
    stopOk = (jellyfin.serviceConfig.SuccessExitStatus or "") == "143";
  in {
    checks.container-host-dns = pkgs.runCommand "container-host-dns" {} ''
      set -euo pipefail
      ${lib.optionalString (!orderingOk) ''
        echo "FAIL container units must wait for neo-host-dns, and tailscale accept-dns must be required" >&2
        echo "jellyfin after: ${lib.concatStringsSep " " (jellyfin.after or [])}" >&2
        exit 1
      ''}
      ${lib.optionalString (!stopOk) ''
        echo "FAIL docker stop (SIGTERM, exit 143) must be a successful container stop" >&2
        echo "SuccessExitStatus: ${toString (jellyfin.serviceConfig.SuccessExitStatus or "")}" >&2
        exit 1
      ''}
      fail=0
      tmp=$(mktemp -d)
      try() {
        name=$1
        expect=$2
        shift 2
        echo "=== $name ==="
        set +e
        env \
          NEO_RESOLVCONF_KEY="$tmp/key" \
          NEO_MAX_WAIT_SEC=0 \
          "$@" \
          ${script}
        rc=$?
        set -e
        rm -f "$tmp/key"
        if [ "$expect" = ok ] && [ "$rc" -ne 0 ]; then
          echo "FAIL $name: expected success, got $rc" >&2
          fail=1
        elif [ "$expect" = fail ] && [ "$rc" -eq 0 ]; then
          echo "FAIL $name: expected failure" >&2
          fail=1
        fi
      }

      # The file Docker copied into long-running Jellyfin: options, no nameserver.
      # No key: only a nameserver is needed. NEO_RESOLVCONF_KEY= clears the
      # path that try() sets.
      try empty fail NEO_RESOLVCONF_KEY= NEO_RESOLV_CONF=${emptyResolv}
      try magic ok NEO_RESOLVCONF_KEY= NEO_RESOLV_CONF=${magicResolv}
      try tailscale-not-registered fail \
        NEO_KEY_DEADLINE_SEC=0 NEO_RESOLV_CONF=${emptyResolv}
      # A nameserver that arrived before tailscale is not MagicDNS yet.
      # After the deadline, containers still get that current host file.
      try tailscale-deadline ok \
        NEO_KEY_DEADLINE_SEC=0 NEO_RESOLV_CONF=${dhcpResolv}
      touch "$tmp/key"
      try tailscale-registered ok \
        NEO_RESOLV_CONF=${tailscaleResolv}
      touch "$tmp/key"
      try netbird-registered ok \
        NEO_RESOLV_CONF=${netbirdResolv}

      rm -rf "$tmp"
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi
      touch "$out"
    '';
  };
}
