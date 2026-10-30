{...}: {
  flake.modules.nixos.guide-sign-up-with-approval = {
    neo.guides.sign-up-with-approval = {
      title = "Let family sign up, with your approval";
      summary = "Family and friends create their own account with their email address. They get into your apps once you approve them.";
      services = ["tinyauth" "authentik"];
      optional = [];
      privacy = [];
      difficulty = "advanced";
      minutes = 45;
      tags = ["login" "security" "family"];
      rank = 110;
      body = ./guide.md;
    };
  };
}
