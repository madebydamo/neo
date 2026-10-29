{...}: {
  flake.modules.nixos.guide-location-timeline = {
    neo.guides.location-timeline = {
      title = "Your private location history";
      summary = "A map of where you have been, like Google Timeline, with your phone sending its location to your own server.";
      services = ["dawarich"];
      optional = ["tinyauth" "tailscale" "rathole"];
      privacy = ["location"];
      difficulty = "easy";
      minutes = 20;
      tags = ["location" "privacy" "phone"];
      rank = 80;
      body = ./guide.md;
    };
  };
}
