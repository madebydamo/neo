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
              enabled = mkEnableOption "iSponsorBlockTV - SponsorBlock client for YouTube TV" {rank = 0;};
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "isponsorblocktv";
            }
            // lib.neo.mkContainerDefinitions {
              isponsorblocktv = "ghcr.io/dmunozv04/isponsorblocktv:latest";
            }
            // lib.neo.mkServiceMeta {
              icon = "https://raw.githubusercontent.com/ajayyy/SponsorBlock/master/public/icons/LogoSponsorBlocker256px.png";
              description = ''
                iSponsorBlockTV is a self-hosted SponsorBlock client for YouTube TV apps on smart TVs, streaming devices, and game consoles.
                It pairs with devices over YouTube's Lounge API (the same used for casting and mobile remote control) to monitor playback and automatically issue skip and mute commands for sponsor segments, intros, outros, and interaction reminders using the crowdsourced SponsorBlock database.
                It additionally auto-mutes and activates the "Skip Ad" button for traditional YouTube advertisements the moment it appears.
                It supports Apple TV, Roku, Chromecast, Google TV, Fire TV, Android TV, Samsung Tizen, LG WebOS, Xbox, PlayStation, Nintendo Switch and more without requiring any apps, modifications, or Premium subscription on the TV.
                Device pairing and configuration uses an interactive setup UI (served via the reverse proxy); once configured the main container runs headless and automatically.
              '';
              projectUrl = "https://github.com/dmunozv04/iSponsorBlockTV";
              githubUrl = "https://github.com/dmunozv04/iSponsorBlockTV";
              releaseUrl = "https://github.com/dmunozv04/iSponsorBlockTV/releases";
            };
        };
        default = {};
        description = "iSponsorBlockTV configuration";
      };
    };
}
