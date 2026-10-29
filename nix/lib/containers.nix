# Container, systemd unit, and appdata helpers for image configurability, auto-updates, and neo web UI.
# containers.* uses rank 300 so the group sits after skill (200) in the service form.
{lib, ...}: let
  neoUi = (import ./ui.nix {inherit lib;}).libExtensions.ui.neo.ui;
in {
  libExtensions.containers = {
    neo = {
      # Declares a fixed set of container image options (containers.<name> : str).
      # Only the image string is overridable; keys cannot be added or removed from settings.
      # Optional `rank` (default 300) places the whole containers.* block among top-level siblings.
      # Pass extraUnits for non-docker systemd units.
      # Pass timerUnits for timer-backed oneshots: they stay in the unit list,
      # and the service target arms the timer instead of starting the job.
      # Do not call mkSystemdUnits after this.
      mkContainerDefinitions = argset:
        with lib; let
          extraUnits = argset.extraUnits or [];
          timerUnits = argset.timerUnits or [];
          rank = argset.rank or 300;
          containers = removeAttrs argset ["extraUnits" "rank" "timerUnits"];
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
                        # Docker image ("repo:tag") for container ${name}; read by the updater and web UI.
                        description = "Which version of the app to run (image name and tag). Change only if you need a specific version.";
                      }
                  )
                  containers;
              };
              default = {};
              # Docker image overrides for this service's declared containers.
              description = "Which version of each part of this app runs.";
            }
            // {
              inherit rank;
              ui = neoUi.mkUi {group = neoUi.groups.advanced;};
            };

          systemdUnits = mkOption {
            type = types.listOf types.str;
            default = dockerUnits ++ extraUnits ++ timerUnits;
            internal = true;
            description = "Systemd unit names (without .service) for this service's containers and extras; used by neo web UI.";
          };

          systemdTimers = mkOption {
            type = types.listOf types.str;
            default = timerUnits;
            internal = true;
            description = "Timer-backed units. The service target arms the matching .timer and does not start or restart the job.";
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

      # Subset of systemdUnits whose .timer the service target arms and stops.
      # The matching .service is not started or restarted with the target.
      mkSystemdTimers = timers:
        with lib; {
          systemdTimers = mkOption {
            type = types.listOf types.str;
            default = timers;
            internal = true;
            description = "Timer-backed units. The service target arms the matching .timer and does not start or restart the job.";
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
        # Host directories bind-mounted into the container (localPath:containerPath).
        description ? "Extra folders on the server this app can use. Each one needs the folder on the server and where the app sees it.",
      }:
        with lib; {
          additionalMountPoints =
            mkOption {
              type = types.listOf (types.submodule {
                options = {
                  localPath =
                    mkOption {
                      type = types.str;
                      description = "Full path of the folder on the server, e.g. /var/neo/DATA/Media/Movies.";
                    }
                    // {
                      rank = 0;
                      ui = neoUi.mkUi {label = "Folder on the server";};
                    };
                  containerPath =
                    mkOption {
                      type = types.str;
                      description = "Full path where the app sees this folder (inside the app, not on the server).";
                    }
                    // {
                      rank = 10;
                      ui = neoUi.mkUi {label = "Path in the app";};
                    };
                };
              });
              default = [];
              description = description;
            }
            // {
              inherit rank;
              ui = neoUi.mkUi {label = "Extra folders";};
            };
        };

      # Turn additionalMountPoints entries into docker/OCI volume specs ("host:container").
      toOciBindMounts = mounts:
        map (m: "${m.localPath}:${m.containerPath}") mounts;

      # Enabled services' container image registry for docker-updater.
      # Only containers whose docker-<name> unit is listed in systemdUnits are
      # included, so optional sidecars gated out of systemdUnits (e.g. tinyauth
      # Entra bridge when entra is off) are not pulled or restarted.
      # Prefer the systemdUnits intersection over the raw containers attrset: image
      # options may still exist for UI overrides while the unit is mkIf'd off.
      getAllContainers = config:
        lib.concatLists (
          lib.mapAttrsToList (
            sname: svc:
              if (svc.enabled or false)
              then let
                units = svc.systemdUnits or [];
              in
                lib.filter (c: lib.elem c.unit units) (
                  lib.mapAttrsToList (
                    cname: cimg: {
                      service = sname;
                      container = cname;
                      image = cimg;
                      unit = "docker-${cname}";
                    }
                  ) (svc.containers or {})
                )
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
