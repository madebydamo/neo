# Example service implementation.
# One OCI container on its own network (app-<name>, the default for every container
# in mkContainerDefinitions); SWAG reaches it as http://<name>:80.
# Containers of other services cannot reach it (docs/design/network-isolation.md).
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
        # Created before every container start, owned by the neo uid/gid.
        neo.core.containerDirs.${name} = [appdata "${appdata}/www"];
        # Runs next: write files rendered from options.
        systemd.services."docker-${name}".preStart = concatStringsSep "\n" [
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
          environment = lib.neo.linuxserverEnv config;
          volumes = [
            "${appdata}:/config"
          ];
        };
      };
    };
}
