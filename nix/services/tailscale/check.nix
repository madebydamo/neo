# tailscale-split-dns is Before=dnsmasq. A synchronous systemctl
# restart/start of dnsmasq from that oneshot deadlocks: dnsmasq's start
# job waits on After=tailscale-split-dns, and the oneshot waits on the
# restart. Cold boot hides it because dnsmasq is still inactive.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    tailscaleModule = (import ./default.nix {}).flake.modules.nixos.tailscale;
    secretsModule = (import ../../modules/core/secrets.nix {}).flake.modules.nixos.core-secrets;
    extendedLib = lib.extend (
      _: super: {
        neo =
          (super.neo or {})
          // (import ../../lib/setup-service.nix {inherit lib;}).libExtensions.setup-service.neo
          // (import ../../lib/secrets.nix {inherit lib;}).libExtensions.secrets.neo
          // {
            localDnsNamesFromConfig = _: ["app.example.test"];
          };
      }
    );
    eval = import "${pkgs.path}/nixos/lib/eval-config.nix" {
      system = "x86_64-linux";
      specialArgs = {lib = extendedLib;};
      modules = [
        tailscaleModule
        secretsModule
        {
          options.neo.services.tailscale = lib.mkOption {
            type = lib.types.submodule {
              options = {
                enabled = lib.mkEnableOption "tailscale";
                authKey = lib.mkOption {
                  type = lib.types.nullOr lib.types.str;
                  default = null;
                };
                splitDns = lib.mkOption {
                  type = lib.types.bool;
                  default = false;
                };
              };
            };
            default = {};
          };
          options.neo.services.swag.domain = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
          };
          config = {
            boot.isContainer = true;
            fileSystems."/" = {
              device = "x";
              fsType = "ext4";
            };
            system.stateVersion = "24.11";
            documentation.enable = false;
            neo.services.tailscale.enabled = true;
            neo.services.tailscale.splitDns = true;
            neo.services.swag.domain = "example.test";
          };
        }
      ];
    };

    unit = eval.config.systemd.services.tailscale-split-dns;
    dnsmasq = eval.config.systemd.services.dnsmasq;
    script = unit.script or "";
    systemctlLines = lib.filter (line: line != "") (map (line: let
      trimmed = lib.trim line;
    in
      if
        !(lib.hasPrefix "#" trimmed)
        && lib.hasInfix "systemctl" trimmed
        && lib.hasInfix "dnsmasq" trimmed
      then trimmed
      else "") (lib.splitString "\n" script));
    allowed = [
      "if systemctl is-active --quiet dnsmasq.service; then"
      "systemctl --no-block try-restart dnsmasq.service"
    ];
    systemctlOk = systemctlLines == allowed;
    beforeOk = lib.elem "dnsmasq.service" (unit.before or []);
    dnsmasqAfterOk = lib.elem "tailscale-split-dns.service" (dnsmasq.after or []);
    timeout = toString (unit.serviceConfig.TimeoutStartSec or "");
    timeoutOk = timeout == "1min";
  in {
    checks.tailscale-split-dns-restart = pkgs.runCommand "tailscale-split-dns-restart" {} ''
      set -euo pipefail
      ${lib.optionalString (!systemctlOk) ''
        echo "FAIL tailscale-split-dns must queue a non-blocking try-restart of dnsmasq" >&2
        echo "systemctl lines:" >&2
        echo ${lib.escapeShellArg (lib.concatStringsSep "\n" systemctlLines)} >&2
        exit 1
      ''}
      ${lib.optionalString (!beforeOk) ''
        echo "FAIL tailscale-split-dns must stay Before=dnsmasq.service so boot writes the zone first" >&2
        exit 1
      ''}
      ${lib.optionalString (!dnsmasqAfterOk) ''
        echo "FAIL dnsmasq must stay After=tailscale-split-dns.service" >&2
        exit 1
      ''}
      ${lib.optionalString (!timeoutOk) ''
        echo "FAIL tailscale-split-dns TimeoutStartSec must be 1min (oneshot default is infinity)" >&2
        echo "actual: ${timeout}" >&2
        exit 1
      ''}
      touch "$out"
    '';
  };
}
