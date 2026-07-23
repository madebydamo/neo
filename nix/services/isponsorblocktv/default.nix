# iSponsorBlockTV service implementation with ttyd-based setup UI.
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
    in {
      config = mkIf cfg.enabled {
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
        systemd.services.isponsorblocktv-setup = {
          description = "iSponsorBlockTV setup UI (ttyd)";
          after = ["docker.service"];
          wants = ["docker.service"];
          wantedBy = ["multi-user.target"];

          serviceConfig = {
            # User = "root";
            # SupplementaryGroups = ["docker"];
            ExecStart = ''
              ${pkgs.ttyd}/bin/ttyd \
                --once \
                --writable \
                -p 7681 \
                -t titleFixed="iSponsorBlockTV Setup" \
                timeout 3600 \
                ${pkgs.docker}/bin/docker run --rm -it \
                  -v ${dataDir}:/app/data \
                  --network=host \
                  ${image} --setup
            '';
            ExecStopPost = ''
              ${pkgs.systemd}/bin/systemctl restart --no-block docker-isponsorblocktv.service || true
            '';
            Restart = "always";
          };
        };
      };
    };
}
