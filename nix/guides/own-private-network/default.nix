{...}: {
  flake.modules.nixos.guide-own-private-network = {
    neo.guides.own-private-network = {
      title = "Run your own private network";
      summary = "One of your servers runs the network that your phones, laptops and other servers join, with no outside account.";
      services = ["netbird-server" "netbird"];
      optional = ["streamproxy" "rathole"];
      privacy = [];
      difficulty = "advanced";
      minutes = 40;
      tags = ["network" "remote access" "privacy"];
      rank = 22;
      body = ./guide.md;
    };
  };
}
