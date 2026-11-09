# Setup units: one-time configuration that runs after a service's containers
# are up (occ calls, config patches, principal provisioning, `tailscale up`).
#
#   systemd.services.foo-setup = lib.neo.mkSetupService {
#     inherit pkgs;
#     name = "foo-setup";
#     description = "Foo post-install configuration";
#     containers = ["foo" "foo-db"];   # After= + Requires= docker-<c>.service
#     attachTo = ["foo"];              # docker-foo Wants= the setup (default: containers)
#     retryInterval = 30;              # seconds between attempts (null: one attempt)
#     script = ''
#       docker exec foo foo-cli configure   # one attempt; exit 0 = done
#     '';
#   };
#
# Also list the unit in the service's systemdUnits (mkContainerDefinitions
# extraUnits / mkSystemdUnits) so the web UI and neo-<service>.target see it.
#
# Semantics:
# - RemainAfterExit=yes on every setup unit. A setup that exited 0 stays
#   "active (exited)" instead of dropping to "inactive", so status shows it
#   as done. A non-zero exit is still "failed".
# - blocking = false (default): Type=simple. The unit is active as soon as the
#   script starts, so switch-to-configuration and boot never wait for a retry
#   loop (Type=oneshot would hold the start job until the loop succeeds).
#   With RemainAfterExit the exit 0 keeps it active; there is no inactive gap.
#   Nothing can order After= on a non-blocking setup and expect it finished.
# - blocking = true: Type=oneshot. Units ordered After= it wait until it
#   succeeded (tailscale-up before split DNS, neo-bootstrap before neo-web).
#   Start jobs block until done: give it a TimeoutStartSec or a bounded retry.
# - Retries: the script is one attempt. With retryInterval set, a wrapper
#   re-runs it every retryInterval seconds until it exits 0 (or maxAttempts
#   is reached, then the unit fails). Restart=on-failure covers crashes of
#   the wrapper itself; with retryInterval = null it is the retry mechanism.
# - Requires= on the containers propagates their restart, so the setup runs
#   again after an image update or a container restart. A changed script
#   changes the unit, and switch-to-configuration restarts it because it is
#   active.
# - The attempt runs under bash -e, like a NixOS `script`.
# - unitConfig.X-Neo-Setup marks the unit for the web UI (setup badge) and
#   for the consistency check in nix/modules/core/setup-units.nix. systemd
#   ignores X- keys.
{lib, ...}: let
  marker = "X-Neo-Setup";
in {
  libExtensions.setup-service = {
    neo = {
      # True when a NixOS systemd.services.<name> value was built by mkSetupService.
      isSetupService = unit: ((unit.unitConfig or {}).${marker} or null) == "yes";

      mkSetupService = {
        pkgs,
        name,
        description,
        script,
        containers ? [],
        attachTo ? containers,
        after ? [],
        wants ? [],
        requires ? [],
        bindsTo ? [],
        partOf ? [],
        before ? [],
        wantedBy ? [],
        path ? [],
        environment ? {},
        retryInterval ? 60,
        maxAttempts ? null,
        blocking ? false,
        serviceConfig ? {},
        unitConfig ? {},
      }: let
        dockerUnits = cs: map (c: "docker-${c}.service") cs;
        looped = retryInterval != null;
        interval = toString retryInterval;
        attempt = pkgs.writeShellScript "${name}-attempt" ''
          set -e
          ${script}
        '';
        giveUp = lib.optionalString (maxAttempts != null) ''
          if [ "$attempt" -ge ${toString maxAttempts} ]; then
            echo "${name}: giving up after $attempt attempts" >&2
            exit 1
          fi
        '';
        runner = ''
          attempt=0
          while :; do
            attempt=$((attempt + 1))
            if ${attempt}; then
              echo "${name}: done"
              exit 0
            fi
            ${giveUp}
            echo "${name}: not ready (attempt $attempt), retry in ${interval}s" >&2
            sleep ${interval}
          done
        '';
      in {
        inherit description path environment before bindsTo partOf;
        after = dockerUnits containers ++ after;
        requires = dockerUnits containers ++ requires;
        inherit wants;
        wantedBy = dockerUnits attachTo ++ wantedBy;
        script =
          if looped
          then runner
          else script;
        unitConfig = {${marker} = "yes";} // unitConfig;
        serviceConfig =
          {
            Type =
              if blocking
              then "oneshot"
              else "simple";
            RemainAfterExit = true;
            Restart =
              if looped && maxAttempts != null
              then "no"
              else "on-failure";
            RestartSec =
              if looped
              then retryInterval
              else 5;
          }
          // serviceConfig;
      };
    };
  };
}
