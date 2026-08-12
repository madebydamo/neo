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
        Login gate, add users, per-user app ACLs, session issues, publicPaths bypasses.

        ## Architecture notes
        - Used by SWAG via `include /config/nginx/tinyauth-location.conf` (location) + `tinyauth-server.conf` (server)
        - Users option: list of `username:bcrypt_hash`
        - Per-service `auth.enabled` / `auth.publicPaths` control protection
        - `access.<username>.allow` / `.block`: per-user app ACLs (neo service names)
          - empty both → full access
          - non-empty allow → only those apps
          - non-empty block (allow empty) → all apps except those
          - both non-empty → Nix assertion failure
        - Compiled to Tinyauth `TINYAUTH_APPS_*_USERS_BLOCK` with allow-by-default policy
        - Orphan access keys for deleted users are ignored at eval; UI save drops them

        ## Credentials
        - Settings: `services.tinyauth.users` (bcrypt hashes — not plaintext passwords)
        - Neo UI "Add user" helper generates bcrypt entries
        - To reset access: update users in settings + activate

        ## Procedures
        1. Add user in Neo UI → activate
        2. Restrict a user: set access.<user>.allow or .block (UI multi-select of enabled apps) → activate
        3. If locked out: ensure at least one valid users entry; check tinyauth logs
        4. Service unreachable after login: check that service's publicPaths vs protected paths; check USERS_BLOCK env

        ## Pitfalls
        - publicPaths are regexes consumed by tinyauth env, not only nginx
        - Tinyauth's own UI is not behind itself (`auth.available = false`)
        - Do not set both allow and block for the same user
        - Mistyped access username (not in users) is ignored → user stays unrestricted

        ## Verification
        - Login page works; protected app redirects to tinyauth; session persists
        - Restricted user blocked on non-allowed app after login
      '';
    };
  };
}
