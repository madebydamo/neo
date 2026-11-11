{...}: {
  flake.modules.nixos.guide-private-traffic-netbird = {
    neo.guides.private-traffic-netbird = {
      title = "Keep your app traffic private with NetBird";
      summary = "Your phone, laptop and other servers join one private, encrypted network and open your apps at the addresses you already use.";
      services = ["netbird"];
      optional = ["netbird-server" "rathole"];
      privacy = [];
      difficulty = "medium";
      minutes = 25;
      tags = ["network" "remote access" "privacy"];
      rank = 21;
      body = ./guide.md;
    };
  };
}
