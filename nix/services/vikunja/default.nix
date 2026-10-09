# Vikunja implementation.
# Uses the nixpkgs package via services.vikunja (nixos-26.05, Neo's pin).
# Binds loopback only; SWAG reaches it via host.docker.internal + DNAT.
{...}: {
  flake.modules.nixos.vikunja = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.vikunja;
      domain = config.neo.services.swag.domain or "localhost";
      publicHost = "${cfg.subdomain}.${domain}";
      appdata = "${config.neo.core.volumes.appdata}/vikunja";
    in {
      config = mkIf cfg.enabled (mkMerge [
        (lib.neo.mkDockerToLocalhostForward cfg.port)
        {
          services.vikunja = {
            enable = true;
            frontendScheme = "https";
            frontendHostname = publicHost;
            # Not on LAN. SWAG uses host.docker.internal + the DNAT above.
            address = "127.0.0.1";
            port = cfg.port;
            # publicurl comes from frontendScheme + frontendHostname.
            database.path = "${appdata}/vikunja.db";
            settings = {
              files.basepath = mkForce "${appdata}/files";
              service = {
                enableregistration = cfg.allowRegistration;
                enableemailreminders = false;
              };
            };
          };

          # Neo-owned like the other services' appdata (homeserver = core.uid/gid).
          systemd.tmpfiles.rules = [
            "d ${appdata} 0755 homeserver homeserver -"
          ];

          systemd.services.vikunja.serviceConfig = {
            DynamicUser = mkForce false;
            User = "homeserver";
            Group = "homeserver";
            StateDirectory = mkForce [];
            ProtectSystem = "strict";
            ProtectHome = true;
            PrivateTmp = true;
            NoNewPrivileges = true;
            ReadWritePaths = [appdata];
          };
        }
      ]);
    };
}
