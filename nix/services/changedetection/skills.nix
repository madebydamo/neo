# Hermes skill for changedetection.
{...}: {
  flake.modules.nixos.changedetection-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.changedetection;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.changedetection.skill.conf = lib.neo.mkServiceSkill {
      service = "changedetection";
      inherit cfg domain;
      description = "Website change detection and alerts";
      tags = ["neo" "changedetection"];
      body = ''
        ## When to Use
        URL watches, notifications, browser fetch failures, REST API.

        ## Credentials
        - App users/API in app; edge tinyauth
        - Notification tokens configured in app UI

        ## Verification
        - Create watch; receive notification on change
      '';
    };
  };
}
