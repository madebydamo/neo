{...}: {
  flake.modules.nixos.guide-private-traffic-tailscale = {
    neo.guides.private-traffic-tailscale = {
      title = "Keep your app traffic private with Tailscale";
      summary = "Your phone and laptop talk to the server directly over a private, encrypted connection, at the same addresses you already use.";
      services = ["tailscale"];
      optional = ["rathole" "pihole"];
      privacy = [];
      difficulty = "medium";
      minutes = 20;
      tags = ["network" "remote access" "privacy"];
      rank = 20;
      body = ./guide.md;
    };
  };
}
