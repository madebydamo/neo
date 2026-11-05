# Example service implementation.
# One OCI container on the `internal` network; SWAG reaches it as http://<name>:80.
# `name` is "example", or "<prefix>-example" when the plugin is added under a prefix:
# use it for the service, container, unit and appdata names (never the literal).
# More units: add containers to mkContainerDefinitions (docker-<name> each), and list
# other systemd units in its extraUnits so the web UI and neo-<name>.target see them.
# Post-start configuration (API calls, config patches) belongs in lib.neo.mkSetupService.
{...}: {
  neoPlugin.modules.example = {
    config,
    lib,
    plugin,
    ...
  }:
    with lib; let
      name = plugin.name "example";
      cfg = config.neo.services.${name};
      appdata = "${config.neo.core.volumes.appdata}/${name}";
      page = ''
        <!doctype html>
        <title>Example</title>
        <h1>${escapeXML cfg.greeting}</h1>
      '';
    in {
      config = mkIf cfg.enabled {
        # Runs before every container start: create appdata (owned by the neo uid/gid)
        # and write files rendered from options.
        systemd.services."docker-${name}".preStart = concatStringsSep "\n" [
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

        virtualisation.oci-containers.containers.${name} = {
          image = cfg.containers.${name};
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
