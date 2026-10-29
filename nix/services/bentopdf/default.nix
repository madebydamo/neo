# BentoPDF service implementation (single stateless nginx container).
# Runtime tool hiding goes through a read-only config.json mount.
{...}: {
  flake.modules.nixos.bentopdf = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.bentopdf;
      runtimeConfig = pkgs.writeText "bentopdf-config.json" (builtins.toJSON {
        inherit (cfg) disabledTools editorDisabledCategories;
      });
    in {
      config = mkIf cfg.enabled {
        virtualisation.oci-containers.containers.bentopdf = {
          image = cfg.containers.bentopdf;
          autoStart = true;
          environment.PORT = toString cfg.port;
          volumes = ["${runtimeConfig}:/usr/share/nginx/html/config.json:ro"];
          networks = ["internal"];
        };
      };
    };
}
