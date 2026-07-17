# Container, systemd unit, and appdata helpers for image configurability, auto-updates, and neo web UI.
# containers.* uses rank 300 so the group sits after skill (200) in the service form.
{lib, ...}: {
  libExtensions.containers = {
    neo = {
      # Declares a fixed set of container image options (containers.<name> : str).
      # Only the image string is overridable; keys cannot be added or removed from settings.
      # Optional `rank` (default 300) places the whole containers.* block among top-level siblings.
      # Pass extraUnits for non-docker systemd units; do not call mkSystemdUnits after this.
      mkContainerDefinitions = argset:
        with lib; let
          extraUnits = argset.extraUnits or [];
          rank = argset.rank or 300;
          containers = removeAttrs argset ["extraUnits" "rank"];
          dockerUnits = map (n: "docker-${n}") (attrNames containers);
        in {
          containers =
            mkOption {
              type = types.submodule {
                options =
                  mapAttrs (
                    name: image:
                      mkOption {
                        type = types.str;
                        default = image;
                        description = "Docker image for container \"${name}\" (\"repo:tag\"). Enables image switching via settings, auto-updates, and UI.";
                      }
                  )
                  containers;
              };
              default = {};
              description = "Docker image overrides for this service's declared containers";
            }
            // {inherit rank;};

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

      # Extra host→container bind mounts as a list of { localPath, containerPath }.
      # Better UI/TOML than attrsOf string: each entry has two clear fields.
      # Merge into a service submodule:
      #   // lib.neo.mkAdditionalMountPoints { rank = 20; }
      # Apply in default.nix:
      #   volumes = [ ... ] ++ lib.neo.toOciBindMounts cfg.additionalMountPoints;
      mkAdditionalMountPoints = {
        rank ? 20,
        description ? ''
          Extra host directories to bind-mount into the container.
          Each entry is a localPath (absolute host path) and containerPath (path inside the container).
        '',
      }:
        with lib; {
          additionalMountPoints =
            mkOption {
              type = types.listOf (types.submodule {
                options = {
                  localPath =
                    mkOption {
                      type = types.str;
                      description = "Absolute path on the host to mount";
                    }
                    // {rank = 0;};
                  containerPath =
                    mkOption {
                      type = types.str;
                      description = "Absolute path inside the container";
                    }
                    // {rank = 10;};
                };
              });
              default = [];
              description = description;
            }
            // {inherit rank;};
        };

      # Turn additionalMountPoints entries into docker/OCI volume specs ("host:container").
      toOciBindMounts = mounts:
        map (m: "${m.localPath}:${m.containerPath}") mounts;

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
