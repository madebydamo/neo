# Hermes skill for tinyauth.
{...}: {
  flake.modules.nixos.tinyauth-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.tinyauth.skill.conf = lib.neo.mkServiceSkill {
      service = "tinyauth";
      inherit config;
      description = "tinyauth edge SSO, users, publicPaths";
      tags = ["neo" "tinyauth" "auth"];
      title = "Neo · tinyauth (forward auth)";
      body = ''
        ## When to Use
        Login gate, add users, per-user app ACLs, session issues, publicPaths bypasses.

        ## Architecture notes
        - Used by SWAG via `include /config/nginx/tinyauth-location.conf` (location) + `tinyauth-server.conf` (server). 401 and 403 follow Tinyauth's `X-Tinyauth-Location` header.
        - Users option: list of `username:bcrypt_hash`
        - Per-service `auth.enabled` / `auth.publicPaths` control protection
        - `backgroundImage` (default true): random lipsum.app login wallpaper; off = Tinyauth default
        - `access.<username>.allow` / `.block`: per-user app ACLs (neo service names)
          - empty both → full access
          - non-empty allow → only those apps
          - non-empty block (allow empty) → all apps except those
          - both non-empty → Nix assertion failure
        - Compiled to Tinyauth `TINYAUTH_APPS_*_USERS_BLOCK` with allow-by-default policy
        - Orphan access keys for deleted users are ignored at eval; UI save drops them
        - OAuth sign-in (`oauth.<id>`): any number of providers in parallel, next to local users.
          Redirect URI per provider: `https://<tinyauth subdomain>.<domain>/api/oauth/callback/<id>`.
          IDs are lowercase a-z0-9 (they become `TINYAUTH_OAUTH_PROVIDERS_<ID>_*`).
          - `type = "generic"`: explicit `authUrl` / `tokenUrl` / `userinfoUrl` + `scopes`
            (Authentik: `/application/o/authorize/`, `/token/`, `/userinfo/`; groups come with the profile scope)
          - `type = "entra"`: `tenantId`; userinfo goes through the `tinyauth-entra` bridge container
            (entra_userinfo.py: Graph `/me` + `/me/transitiveMemberOf` → email + groups as names and object IDs)
          - `oauthAutoRedirect`: provider ID to skip the login page
          - `oauthGroups`: groups (from any provider) whose members may use apps; OAuth users in none of them are denied everywhere
          - `groupAccess.<group>.allow` / `.block`: narrow what a group grants (same rules as per-user access)
          - Compiled to `TINYAUTH_APPS_*_OAUTH_GROUPS` (union of granting groups; `neo-no-oauth-group` when none)
          - Groups are read at login and kept in the session; membership changes apply on next login

        ## Credentials
        - Settings: `services.tinyauth.users` (bcrypt hashes — not plaintext passwords)
        - OAuth: `services.tinyauth.oauth.<id>.clientId`, `.clientSecret` (+ `tenantId` for Entra: app registration with
          Graph delegated permissions openid, profile, email, User.Read, GroupMember.Read.All with admin consent)
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
        - Entra login error page: `just logs docker-tinyauth-entra` (Graph 403 = missing GroupMember.Read.All consent;
          set `oauth.<id>.groupNames = false` to match by object ID with User.Read only)
        - OAuth user denied everywhere: their groups claim has none of `oauthGroups` (check provider group names)

        ## Verification
        - Login page works; protected app redirects to tinyauth; session persists
        - Restricted user blocked on non-allowed app after login
      '';
    };
  };
}
