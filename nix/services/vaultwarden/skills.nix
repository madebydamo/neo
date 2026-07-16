# Hermes skill for vaultwarden.
{...}: {
  flake.modules.nixos.vaultwarden-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vaultwarden;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.vaultwarden.skill.conf = lib.neo.mkServiceSkill {
      service = "vaultwarden";
      inherit cfg domain;
      description = "Vaultwarden Bitwarden-compatible vault + admin";
      tags = ["neo" "vaultwarden" "passwords"];
      body = ''
        ## When to Use
        Password vault admin, Bitwarden clients, admin token, backups of vault data.

        ## Architecture notes
        - Appdata holds sqlite/attachments — **critical secrets**
        - Admin panel uses `adminToken`

        ## Credentials
        - Neo: `services.vaultwarden.adminToken` (admin UI)
        - Vault users: Bitwarden clients / web vault (app-managed)
        - Edge: tinyauth; client API paths public for apps

        ## Procedures
        1. Health-check container
        2. Admin: use adminToken from settings (handle carefully — never paste into public chat)
        3. Backup: ensure backup service covers vaultwarden appdata

        ## Pitfalls
        - Clearing appdata **destroys the password vault**
        - Never log adminToken or vault exports into Hermes chat/history carelessly

        ## Verification
        - Clients sync; admin panel accepts token
      '';
    };
  };
}
