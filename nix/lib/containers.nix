# Container, systemd unit, and appdata helpers for image configurability, auto-updates, and neo web UI.
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

      # Declares the host directory that holds this service's mutable app data.
      # Neo web uses it for "Clear appdata" (stop units → rm -rf → start units).
      # Pass an absolute path, typically "${config.neo.core.volumes.appdata}/<name>".
      mkAppdata = path:
        with lib; {
          appdata = mkOption {
            type = types.nullOr types.str;
            default = path;
            internal = true;
            description = "Host path of this service's appdata directory; used by neo web UI clear-appdata (stop related units, remove recursively, start again).";
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
