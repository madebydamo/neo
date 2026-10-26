# Example service implementation.
# One OCI container on the `internal` network; SWAG reaches it as http://example:80.
# More units: add containers to mkContainerDefinitions (docker-<name> each), and list
# other systemd units in its extraUnits so the web UI and neo-example.target see them.
# Post-start configuration (API calls, config patches) belongs in lib.neo.mkSetupService.
{...}: {
  flake.modules.nixos.example = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.example;
      appdata = "${config.neo.core.volumes.appdata}/example";
      page = ''
        <!doctype html>
        <title>Example</title>
        <h1>${escapeXML cfg.greeting}</h1>
      '';
    in {
      config = mkIf cfg.enabled {
        # Runs before every container start: create appdata (owned by the neo uid/gid)
        # and write files rendered from options.
        systemd.services.docker-example.preStart = concatStringsSep "\n" [
          (lib.neo.mkEnsureDirs config [appdata "${appdata}/www"])
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdata}/www/index.html";
            content = page;
          })
          (lib.neo.mkActivationScriptForFile config {
            filePath = "${appdata}/www/health";
            content = "ok";
          })
        ];

        virtualisation.oci-containers.containers.example = {
          image = cfg.containers.example;
          autoStart = true;
          environment = {
            PUID = toString config.neo.core.uid;
            PGID = toString config.neo.core.gid;
            TZ = config.neo.core.timeZone;
          };
          volumes = [
            "${appdata}:/config"
          ];
          networks = ["internal"];
        };
      };
    };
}
