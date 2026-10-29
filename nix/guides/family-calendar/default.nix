{...}: {
  flake.modules.nixos.guide-family-calendar = {
    neo.guides.family-calendar = {
      title = "A shared family calendar on every phone";
      summary = "Keep the family's calendars and contacts at home, synced to every phone, with school holidays added automatically.";
      services = ["rustical"];
      optional = ["calino" "ical-sync"];
      privacy = ["calendar" "contacts"];
      difficulty = "easy";
      minutes = 20;
      tags = ["family" "calendar" "privacy"];
      rank = 30;
      body = ./guide.md;
    };
  };
}
