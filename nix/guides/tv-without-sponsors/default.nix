{...}: {
  flake.modules.nixos.guide-tv-without-sponsors = {
    neo.guides.tv-without-sponsors = {
      title = "YouTube on the TV without sponsor segments";
      summary = "Sponsored segments, intros and subscribe reminders are skipped automatically on the TV, with nothing installed on it.";
      services = ["isponsorblocktv"];
      optional = [];
      privacy = ["browsing"];
      difficulty = "easy";
      minutes = 10;
      tags = ["tv" "family" "privacy"];
      rank = 130;
      body = ./guide.md;
    };
  };
}
