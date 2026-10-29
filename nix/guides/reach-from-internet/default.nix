{...}: {
  flake.modules.nixos.guide-reach-from-internet = {
    neo.guides.reach-from-internet = {
      title = "Make your apps reachable from the internet without opening your router";
      summary = "Open your apps from anywhere through a small rented server, even when your internet provider blocks incoming connections.";
      services = ["rathole"];
      optional = ["tailscale" "pihole"];
      privacy = [];
      difficulty = "advanced";
      minutes = 45;
      tags = ["network" "remote access"];
      rank = 100;
      body = ./guide.md;
    };
  };
}
