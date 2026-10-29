{...}: {
  flake.modules.nixos.guide-party-photo-drop = {
    neo.guides.party-photo-drop = {
      title = "Collect photos from guests after a party";
      summary = "Send one link after a wedding or party, and every guest's photos land in an album on your server, without accounts.";
      services = ["immich" "immich-drop"];
      optional = [];
      privacy = ["photos"];
      difficulty = "easy";
      minutes = 15;
      tags = ["photos" "family" "events"];
      rank = 50;
      body = ./guide.md;
    };
  };
}
