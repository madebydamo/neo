{...}: {
  flake.modules.nixos.guide-tailscale-everywhere = {
    neo.guides.tailscale-everywhere = {
      title = "Use your apps from anywhere";
      summary = "Open your apps on the road with the same address as at home, without putting them on the public internet.";
      services = ["tailscale" "pihole"];
      optional = ["rathole"];
      privacy = [];
      difficulty = "medium";
      minutes = 30;
      tags = ["network" "remote access" "privacy"];
      rank = 20;
      body = ./guide.md;
    };
  };
}
