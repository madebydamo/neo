# Docker registry service implementation (single container).
# Optional htpasswd auth comes from `users` (bcrypt lines, rendered to a store file
# mounted read-only). Without users the registry is open; a warning fires when it
# is also reachable from the internet.
{...}: {
  flake.modules.nixos.registry = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.registry;
      appdata = "${config.neo.core.volumes.appdata}/registry";
      dataDir = "${appdata}/data";
      authEnabled = cfg.users != [];
      htpasswd = pkgs.writeText "neo-registry-htpasswd" (concatStringsSep "\n" cfg.users + "\n");
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = all (u: builtins.match "[^:]+:\\$2[aby]?\\$.+" u != null) cfg.users;
            message = "neo.services.registry: users entries must be username:bcrypt_hash (htpasswd -nbB); the registry only accepts bcrypt.";
          }
        ];

        warnings = optional (!authEnabled && elem "web" cfg.ingress) ''
          neo.services.registry: no users are set and ingress includes "web", so anyone on the internet can push and pull images. Add a user or remove "web" from ingress.
        '';

        systemd.services.docker-registry.preStart = lib.neo.mkEnsureDirs config [
          appdata
          dataDir
        ];

        virtualisation.oci-containers.containers.registry = {
          image = cfg.containers.registry;
          user = "${toString config.neo.core.uid}:${toString config.neo.core.gid}";
          # Env overrides of /etc/distribution/config.yml (REGISTRY_<section>_<key>).
          environment =
            {
              REGISTRY_HTTP_ADDR = "0.0.0.0:${toString cfg.port}";
              # Behind SWAG: Location headers stay relative instead of http://registry:5000.
              REGISTRY_HTTP_RELATIVEURLS = "true";
              REGISTRY_STORAGE_FILESYSTEM_ROOTDIRECTORY = "/var/lib/registry";
              REGISTRY_STORAGE_DELETE_ENABLED = boolToString cfg.deleteEnabled;
            }
            // optionalAttrs authEnabled {
              REGISTRY_AUTH = "htpasswd";
              REGISTRY_AUTH_HTPASSWD_REALM = "Neo Registry";
              REGISTRY_AUTH_HTPASSWD_PATH = "/auth/htpasswd";
            };
          volumes =
            ["${dataDir}:/var/lib/registry"]
            ++ optional authEnabled "${htpasswd}:/auth/htpasswd:ro";
          networks = ["internal"];
        };
      };
    };
}
