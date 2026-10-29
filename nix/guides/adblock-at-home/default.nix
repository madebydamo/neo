{...}: {
  flake.modules.nixos.guide-adblock-at-home = {
    neo.guides.adblock-at-home = {
      title = "Block ads for the whole home";
      summary = "Every phone, TV and laptop at home without ads and trackers, without installing anything on them.";
      services = ["pihole"];
      optional = ["tailscale"];
      privacy = ["browsing"];
      difficulty = "easy";
      minutes = 15;
      tags = ["network" "privacy" "family"];
      rank = 10;
      body = ./guide.md;
    };
  };
}
