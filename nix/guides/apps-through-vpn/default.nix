{...}: {
  flake.modules.nixos.guide-apps-through-vpn = {
    neo.guides.apps-through-vpn = {
      title = "Send your apps through a VPN";
      summary = "Connect your server to a VPN provider such as AirVPN, then pick the apps that go online through it, starting with a private browser you open from any device.";
      services = ["vpn" "firefox"];
      optional = ["webtop" "searxng" "karakeep" "changedetection"];
      privacy = ["browsing"];
      difficulty = "medium";
      minutes = 20;
      tags = ["privacy" "browser" "vpn"];
      rank = 150;
      body = ./guide.md;
    };
  };
}
