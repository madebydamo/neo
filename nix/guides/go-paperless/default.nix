{...}: {
  flake.modules.nixos.guide-go-paperless = {
    neo.guides.go-paperless = {
      title = "Scan letters and find them again";
      summary = "Turn letters, bills and PDFs into a searchable archive at home, and find any paper again by typing a word from it.";
      services = ["paperless"];
      optional = [];
      privacy = ["documents"];
      difficulty = "medium";
      minutes = 30;
      tags = ["documents" "family" "privacy"];
      rank = 70;
      body = ./guide.md;
    };
  };
}
