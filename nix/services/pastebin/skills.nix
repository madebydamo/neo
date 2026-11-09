# Hermes skill for pastebin.
{...}: {
  flake.modules.nixos.pastebin-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.pastebin.skill.conf = lib.neo.mkServiceSkill {
      service = "pastebin";
      inherit config;
      description = "Minimal pastebin (wantguns/bin)";
      tags = ["neo" "pastebin"];
      title = "Neo · Pastebin (bin)";
      body = ''
        ## When to Use
        Text/file pastes, CLI client usage, storage under appdata.

        ## Credentials
        - None by default — treat as public if exposed

        ## Pitfalls
        - Public pastebins can leak secrets; warn users

        ## Verification
        - Create and retrieve a paste
      '';
    };
  };
}
