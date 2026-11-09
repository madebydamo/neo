# Hermes skill for changedetection.
{...}: {
  flake.modules.nixos.changedetection-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.changedetection.skill.conf = lib.neo.mkServiceSkill {
      service = "changedetection";
      inherit config;
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
