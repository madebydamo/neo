{...}: {
  flake.modules.nixos.guide-sign-in-with-google = {
    neo.guides.sign-in-with-google = {
      title = "Sign in with Google or Microsoft";
      summary = "A \"Sign in with Microsoft\" or Google button on the login page, so family and friends use the account they already have.";
      services = ["tinyauth"];
      optional = ["authentik"];
      privacy = [];
      difficulty = "advanced";
      minutes = 40;
      tags = ["login" "security" "family"];
      rank = 110;
      body = ./guide.md;
    };
  };
}
