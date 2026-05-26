# iSponsorBlockTV service options.
{...}: {
  flake.modules.nixos.isponsorblocktv-option = {
    config,
    lib,
    ...
  }:
    with lib; {
      options.neo.services.isponsorblocktv = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "iSponsorBlockTV - SponsorBlock client for YouTube TV";
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "isponsorblocktv";
            };
        };
        default = {};
        description = "iSponsorBlockTV configuration";
      };
    };
}
