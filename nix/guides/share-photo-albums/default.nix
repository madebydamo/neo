{...}: {
  flake.modules.nixos.guide-share-photo-albums = {
    neo.guides.share-photo-albums = {
      title = "Share photo albums with friends and family";
      summary = "Share an album with family members, send friends a link that needs no account, and let party guests add their own photos.";
      services = ["immich"];
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
