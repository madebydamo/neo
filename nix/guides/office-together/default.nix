{...}: {
  flake.modules.nixos.guide-office-together = {
    neo.guides.office-together = {
      title = "Edit office documents together";
      summary = "Write documents, spreadsheets and presentations together in the browser, stored on your own server instead of Google or Microsoft.";
      services = ["nextcloud" "collabora"];
      optional = [];
      privacy = ["documents" "files"];
      difficulty = "medium";
      minutes = 25;
      tags = ["office" "files" "family" "privacy"];
      rank = 140;
      body = ./guide.md;
    };
  };
}
