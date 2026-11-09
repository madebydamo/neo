# Hermes skill for Authentik.
{...}: {
  flake.modules.nixos.authentik-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.authentik.skill.conf = lib.neo.mkServiceSkill {
      service = "authentik";
      inherit config;
      description = "Authentik identity provider, SSO, OIDC/SAML apps, users, groups";
      tags = ["neo" "authentik" "sso" "oidc" "identity"];
      title = "Neo · Authentik";
      body = ''
        ## When to Use
        Single sign-on for self-hosted apps: OAuth2/OIDC, SAML, LDAP or proxy providers, users, groups, MFA, enrollment and recovery flows.

        ## Architecture notes
        - Containers: `authentik` (server, HTTP :9000), `authentik-worker` (background tasks, blueprints, certs), `authentik-db` (Postgres 16). No Redis (removed upstream in 2025.10)
        - Server and worker share `/data` (media) and `/templates`; the worker also mounts `/certs`
        - No tinyauth in front: Authentik is the login. OIDC discovery lives at `https://authentik.<domain>/application/o/<slug>/.well-known/openid-configuration`
        - Worker has no Docker socket; outposts (LDAP, proxy) must be deployed manually
        - Image tag pinned; server and worker must use the same tag

        ## Credentials
        - Neo: `services.authentik.secretKey` (50+ chars), `dbPassword` (internal DB only)
        - Admin: `akadmin`, password from `services.authentik.bootstrapPassword` (first boot only) or set at `/if/flow/initial-setup/`
        - API: create tokens in Admin → Directory → Tokens; use `Authorization: Bearer <token>` against `/api/v3/`

        ## Procedures
        1. Health-check containers (derived cheatsheet)
        2. Probe `https://authentik.<domain>/-/health/live/` and `/-/health/ready/` for 200
        3. New app SSO: Admin → Applications → Create with provider (OIDC: copy client ID/secret and issuer URL into the app)

        ## Pitfalls
        - Changing `secretKey` invalidates sessions and tokens
        - Bootstrap email/password are ignored once the database exists
        - Without a bootstrap password, finish `/if/flow/initial-setup/` before exposing to the Internet
        - Upgrades migrate the DB; read release notes and do not skip major versions
        - Do not clear appdata without confirmation — destroys all users, providers and apps

        ## Verification
        - Units active; `/-/health/ready/` returns 200; `/` redirects to the login flow
      '';
    };
  };
}
