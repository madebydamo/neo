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
            # database.* keeps the module defaults (sqlite under StateDirectory).
            # Do not set those here: same priority conflicts if they drift.
            settings.service = {
              enableregistration = cfg.allowRegistration;
              enableemailreminders = false;
            };
          };
        }
      ]);
    };
}
