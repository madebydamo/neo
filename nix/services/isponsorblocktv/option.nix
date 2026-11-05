# iSponsorBlockTV service options.
{...}: {
  flake.modules.nixos.isponsorblocktv-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.isponsorblocktv = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Automatic skipping of sponsor segments and ads in YouTube on your TV" {rank = 0;};
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "isponsorblocktv";
            }
            // lib.neo.mkContainerDefinitions {
              isponsorblocktv = "ghcr.io/dmunozv04/isponsorblocktv:latest";
              extraUnits = ["isponsorblocktv-setup"];
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/isponsorblocktv"
            // lib.neo.mkServiceMeta {
              category = "Media";
              license = {
                spdx = "GPL-3.0";
                url = "https://github.com/dmunozv04/iSponsorBlockTV/blob/main/LICENSE.md";
              };
              icon = ./icon.png;
              tagline = "Skips sponsor segments in YouTube on your TV";
              description = ''
                Automatically skips sponsored segments, intros and subscribe reminders when you watch YouTube on a smart TV, streaming stick or game console, and skips ads as soon as that is possible.
                Nothing needs to be installed on the TV.
              '';
              notes = ''
                Pairs with devices over YouTube's Lounge API (the protocol used for casting) and uses the crowdsourced SponsorBlock database.
                Works with Apple TV, Roku, Chromecast, Google TV, Fire TV, Android TV, Samsung, LG, Xbox, PlayStation, Nintendo Switch and more.
                Pair devices in the setup UI (served through the reverse proxy); after that it runs on its own.
              '';
              projectUrl = "https://github.com/dmunozv04/iSponsorBlockTV";
              githubUrl = "https://github.com/dmunozv04/iSponsorBlockTV";
              releaseUrl = "https://github.com/dmunozv04/iSponsorBlockTV/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "iSponsorBlockTV configuration";
      };
    };
}
