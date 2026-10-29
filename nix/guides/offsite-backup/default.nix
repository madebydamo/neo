{...}: {
  flake.modules.nixos.guide-offsite-backup = {
    neo.guides.offsite-backup = {
      title = "Back up to a friend's server";
      summary = "Every night a copy of your photos, documents and app data goes to a server at a friend's or your parents' place.";
      services = ["backup"];
      optional = [];
      privacy = [];
      difficulty = "medium";
      minutes = 30;
      tags = ["backup" "files" "family"];
      rank = 90;
      body = ./guide.md;
    };
  };
}
