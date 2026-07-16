# Hermes skill for tinyauth.
{...}: {
  flake.modules.nixos.tinyauth-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.tinyauth;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.tinyauth.skill.conf = lib.neo.mkServiceSkill {
      service = "tinyauth";
      inherit cfg domain;
      description = "tinyauth edge SSO, users, publicPaths";
      tags = ["neo" "tinyauth" "auth"];
      title = "Neo · tinyauth (forward auth)";
      body = ''
        ## When to Use
        Login gate, add users, session issues, publicPaths bypasses.

        ## Architecture notes
        - Used by SWAG via `auth_request /tinyauth`
        - Users option: list of `username:bcrypt_hash`
        - Per-service `auth.enabled` / `auth.publicPaths` control protection

        ## Credentials
        - Settings: `services.tinyauth.users` (bcrypt hashes — not plaintext passwords)
        - Neo UI "Add user" helper generates bcrypt entries
        - To reset access: update users in settings + activate

        ## Procedures
        1. Add user in Neo UI → activate
        2. If locked out: ensure at least one valid users entry; check tinyauth logs
        3. Service unreachable after login: check that service's publicPaths vs protected paths

        ## Pitfalls
        - publicPaths are regexes consumed by tinyauth env, not only nginx
        - Tinyauth's own UI is not behind itself (`auth.available = false`)

        ## Verification
        - Login page works; protected app redirects to tinyauth; session persists
      '';
    };
  };
}
