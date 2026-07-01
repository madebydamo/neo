# Container and systemd unit helpers for image configurability, auto-updates, and neo web UI status/logs.
{lib, ...}: {
  libExtensions.containers = {
    neo = {
      mkContainerDefinitions = argset:
        with lib; let
          extraUnits = argset.extraUnits or [];
          containers = removeAttrs argset ["extraUnits"];
          dockerUnits = map (n: "docker-${n}") (attrNames containers);
        in {
          containers = mkOption {
            type = types.attrsOf types.str;
            default = containers;
            description = "Docker container name to image mapping (name = \"repo:tag\"). Enables image switching via settings, auto-updates, and UI.";
          };
          systemdUnits = mkOption {
            type = types.listOf types.str;
            default = dockerUnits ++ extraUnits;
            internal = true;
            description = "Systemd unit names (without .service) for this service's containers and extras; used by neo web UI.";
          };
        };

      mkSystemdUnits = units:
        with lib; {
          systemdUnits = mkOption {
            type = types.listOf types.str;
            default = units;
            internal = true;
            description = "Systemd unit names (without .service) managed by this service for neo web UI status/logs/control.";
          };
        };

      getAllContainers = config:
        lib.concatLists (
          lib.mapAttrsToList (
            sname: svc:
              if (svc.enabled or false)
              then
                lib.mapAttrsToList (
                  cname: cimg: {
                    service = sname;
                    container = cname;
                    image = cimg;
                    unit = "docker-${cname}";
                  }
                ) (svc.containers or {})
              else []
          ) (config.neo.services or {})
        );

      getEnabledServiceUnits = config:
        lib.mapAttrsToList (sname: svc: {
          service = sname;
          units = svc.systemdUnits or [];
        }) (lib.filterAttrs (n: v: v.enabled or false) (config.neo.services or {}));
    };
  };
}
