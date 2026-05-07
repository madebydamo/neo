# Nextcloud service options. Web UI protected with tinyauth forward auth.
{...}: {
  flake.modules.nixos.nextcloud-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.collabora = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption ("Collabora real time collaboration platform for Nextcloud. Needs nextcloud to be enabled");
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "collabora";
              auth.enabled = false;
            };
        };
        default = {};
        description = "Collabora service configuration";
      };
    };
}
