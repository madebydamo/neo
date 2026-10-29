{...}: {
  flake.modules.nixos.guide-private-search = {
    neo.guides.private-search = {
      title = "Private search as your browser default";
      summary = "Search from the address bar on every computer and phone without Google or Bing building a profile of you.";
      services = ["searxng"];
      optional = ["vpn"];
      privacy = ["search"];
      difficulty = "easy";
      minutes = 10;
      tags = ["privacy" "search" "browser"];
      rank = 120;
      body = ./guide.md;
    };
  };
}
