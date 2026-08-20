# Tinyauth forward authentication service implementation.
{...}: {
  flake.modules.nixos.tinyauth = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.tinyauth;
      domain = config.neo.services.swag.domain;

      # Collect all services that have auth enabled.
      authServices = filterAttrs (n: v: v.enabled && (v.auth.enabled or false)) config.neo.services;

      # Usernames from "username:bcrypt_hash" entries (orphans in access are ignored).
      knownUsernames = map (u: head (splitString ":" u)) cfg.users;

      # Per-user ACL entries that match a configured user (safe if user deleted but access key remains).
      activeAccess =
        filterAttrs (username: _: elem username knownUsernames) (cfg.access or {});

      # Service names that exist under neo.services (enabled or not) — for typo checks.
      knownServiceNames = attrNames config.neo.services;

      # Users who may not access app `appName` under allow-by-default + USERS_BLOCK:
      # - blacklist mode: app is in block
      # - whitelist mode: allow is non-empty and app is not listed
      usersBlockedFromApp = appName:
        lib.sort (a: b: a < b) (
          lib.filter (
            username: let
              entry = activeAccess.${username};
              allow = entry.allow or [];
              block = entry.block or [];
            in
              if allow != []
              then !(elem appName allow)
              else elem appName block
          ) (attrNames activeAccess)
        );

      # Generate TINYAUTH_APPS_<NAME>_* env vars for services with publicPaths / domains / user ACLs.
      # Tinyauth treats `_` as nested config keys (e.g. APPS_FOO_BAR → apps.foo.bar), so
      # hyphenated service names must not become FOO_BAR — strip hyphens instead (stirling-pdf → STIRLINGPDF).
      appAclEnvVars =
        foldlAttrs (
          acc: name: svc:
            acc
            // (
              let
                appName = lib.toUpper (replaceStrings ["-"] [""] name);
                blocked = usersBlockedFromApp name;
              in
                optionalAttrs (svc.auth.publicPaths or [] != []) {
                  "TINYAUTH_APPS_${appName}_PATH_ALLOW" = "(${concatStringsSep "|" svc.auth.publicPaths})";
                }
                // optionalAttrs (svc.subdomain or null != null) {
                  "TINYAUTH_APPS_${appName}_CONFIG_DOMAIN" = "${svc.subdomain}.${domain}";
                }
                // optionalAttrs (blocked != []) {
                  "TINYAUTH_APPS_${appName}_USERS_BLOCK" = concatStringsSep "," blocked;
                }
            )
        ) {}
        authServices;

      # Flat list of service names referenced in allow/block (for assertions).
      referencedServiceNames = lib.unique (
        concatLists (
          mapAttrsToList (
            _: entry: (entry.allow or []) ++ (entry.block or [])
          )
          activeAccess
        )
      );

      mixedAllowBlockUsers = filter (
        username: let
          e = activeAccess.${username};
        in
          (e.allow or []) != [] && (e.block or []) != []
      ) (attrNames activeAccess);

      unknownServiceRefs =
        filter (n: !(elem n knownServiceNames)) referencedServiceNames;
    in {
      config = mkIf cfg.enabled {
        assertions = [
          {
            assertion = cfg.users != [];
            message = "neo.services.tinyauth: At least one user must be configured in 'users'.";
          }
          {
            assertion = mixedAllowBlockUsers == [];
            message = ''
              neo.services.tinyauth.access: users must not set both non-empty allow and block.
              Conflicting users: ${concatStringsSep ", " mixedAllowBlockUsers}
            '';
          }
          {
            assertion = unknownServiceRefs == [];
            message = ''
              neo.services.tinyauth.access: unknown service name(s) in allow/block: ${concatStringsSep ", " unknownServiceRefs}.
              Use neo.services.* keys (e.g. immich, searxng).
            '';
          }
        ];

        systemd.services.docker-tinyauth.preStart = lib.neo.mkEnsureDirs config [
          "${config.neo.core.volumes.appdata}/tinyauth"
        ];

        virtualisation.oci-containers.containers.tinyauth = {
          image = cfg.containers.tinyauth;
          autoStart = true;
          environment =
            {
              TINYAUTH_APPURL = "https://${cfg.subdomain}.${domain}";
              TINYAUTH_AUTH_USERS = concatStringsSep "," cfg.users;
              TINYAUTH_SERVER_PORT = toString cfg.port;
              TINYAUTH_AUTH_SESSIONEXPIRY = toString cfg.sessionExpiry;
              # Default ACL policy is allow-by-default (auth.acls.policy defaults to "allow").
              TINYAUTH_ANALYTICS_ENABLED = "false";
              TINYAUTH_DATABASE_PATH = "/data/tinyauth.db";
            }
            // optionalAttrs cfg.backgroundImage {
              TINYAUTH_UI_BACKGROUNDIMAGE = "https://lipsum.app/random/1920x1080";
            }
            // appAclEnvVars;
          volumes = [
            "${config.neo.core.volumes.appdata}/tinyauth:/data"
          ];
          networks = ["internal"];
        };
      };
    };
}
