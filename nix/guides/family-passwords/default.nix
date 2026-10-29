{...}: {
  flake.modules.nixos.guide-family-passwords = {
    neo.guides.family-passwords = {
      title = "A password manager for the whole family";
      summary = "Everyone gets their own password vault on your server, filled in by the Bitwarden apps, with shared passwords for the family.";
      services = ["vaultwarden"];
      optional = [];
      privacy = ["passwords"];
      difficulty = "easy";
      minutes = 30;
      tags = ["passwords" "family" "security" "privacy"];
      rank = 60;
      body = ./guide.md;
    };
  };
}
