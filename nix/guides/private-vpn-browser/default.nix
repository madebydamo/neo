{...}: {
  flake.modules.nixos.guide-private-vpn-browser = {
    neo.guides.private-vpn-browser = {
      title = "Browse through a VPN without a VPN app";
      summary = "A browser that runs on your server and goes online through your VPN provider, opened from any device in a normal tab.";
      services = ["vpn" "firefox"];
      optional = ["webtop"];
      privacy = ["browsing"];
      difficulty = "medium";
      minutes = 20;
      tags = ["privacy" "browser" "vpn"];
      rank = 150;
      body = ./guide.md;
    };
  };
}
