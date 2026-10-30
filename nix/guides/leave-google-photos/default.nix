{...}: {
  flake.modules.nixos.guide-leave-google-photos = {
    neo.guides.leave-google-photos = {
      title = "Leave Google Photos";
      summary = "Back up every phone's photos and videos to your own server, with search, faces and shared albums like before.";
      services = ["immich"];
      optional = [];
      privacy = ["photos"];
      difficulty = "easy";
      minutes = 30;
      tags = ["photos" "family" "privacy"];
      rank = 40;
      body = ./guide.md;
    };
  };
}
