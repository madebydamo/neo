# iSponsorBlockTV service implementation with ttyd-based setup UI.
# Setup unit is temporary: named container, max 1h runtime, cleaned on stop.
# ttyd binds loopback only; SWAG reaches it via host.docker.internal + DNAT
# (same pattern as neo-web).
{...}: {
  flake.modules.nixos.isponsorblocktv = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.isponsorblocktv;
      dataDir = "${config.neo.core.volumes.appdata}/isponsorblocktv";
      image = cfg.containers.isponsorblocktv;
      setupPort = 7681;
      setupContainerName = "isponsorblocktv-setup";
      cleanupScript = pkgs.writeShellScript "isponsorblocktv-setup-cleanup" ''
        ${pkgs.docker}/bin/docker rm -f ${setupContainerName} 2>/dev/null || true
        ${pkgs.systemd}/bin/systemctl restart --no-block docker-isponsorblocktv.service || true
      '';
    in {
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkDockerToLocalhostForward setupPort)
        {
          systemd.services.docker-isponsorblocktv.preStart = lib.neo.mkEnsureDirs config [
            dataDir
          ];

          # Main headless container (waits until devices are configured)
          virtualisation.oci-containers.containers.isponsorblocktv = {
            inherit image;
            volumes = [
              "${dataDir}:/app/data"
            ];
            networks = ["internal"];
          };

          # Setup UI: start with `systemctl start isponsorblocktv-setup` (or after boot).
          # --once: ttyd exits after one client disconnects.
          # RuntimeMaxSec: hard stop after 1h even if left open.
          # Named container + ExecStopPost: no orphan setup containers.
          # -i 127.0.0.1: not on LAN; SWAG uses host.docker.internal + DNAT.
          systemd.services.isponsorblocktv-setup = {
            description = "iSponsorBlockTV setup UI (ttyd on 127.0.0.1, auto-stops after 1h)";
            after = ["docker.service"];
            wants = ["docker.service"];
            # Available after boot for pairing; does not restart forever.
            wantedBy = ["multi-user.target"];

            serviceConfig = {
              Type = "simple";
              RuntimeMaxSec = 3600;
              Restart = "no";
              ExecStart = ''
                ${pkgs.ttyd}/bin/ttyd \
                  --once \
                  --writable \
                  -i 127.0.0.1 \
                  -p ${toString setupPort} \
                  -t titleFixed="iSponsorBlockTV Setup" \
                  ${pkgs.docker}/bin/docker run --rm -it \
                    --name ${setupContainerName} \
                    -v ${dataDir}:/app/data \
                    --network=host \
                    ${image} --setup
              '';
              ExecStopPost = "${cleanupScript}";
            };
          };
        }
      ]);
    };
}
