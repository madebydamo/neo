# iSponsorBlockTV service implementation with ttyd-based setup UI.
# ttyd binds loopback only; SWAG reaches it via host.docker.internal + DNAT
# (same pattern as neo-web). Setup container is named for reliable cleanup.
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

          # Setup service using host ttyd
          # -i 127.0.0.1: not on LAN; SWAG uses host.docker.internal + DNAT.
          # Named container + ExecStopPost: no orphan setup containers.
          systemd.services.isponsorblocktv-setup = {
            description = "iSponsorBlockTV setup UI (ttyd)";
            after = ["docker.service"];
            wants = ["docker.service"];
            wantedBy = ["multi-user.target"];

            serviceConfig = {
              ExecStart = ''
                ${pkgs.ttyd}/bin/ttyd \
                  --once \
                  --writable \
                  -i 127.0.0.1 \
                  -p ${toString setupPort} \
                  -t titleFixed="iSponsorBlockTV Setup" \
                  timeout 3600 \
                  ${pkgs.docker}/bin/docker run --rm -it \
                    --name ${setupContainerName} \
                    -v ${dataDir}:/app/data \
                    --network=host \
                    ${image} --setup
              '';
              ExecStopPost = "${cleanupScript}";
              Restart = "always";
            };
          };
        }
      ]);
    };
}
