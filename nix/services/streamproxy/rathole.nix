{...}: {
  flake.modules.nixos.streamproxy-rathole = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; let
      cfg = config.neo.services.streamproxy;
      nonSwagEntries = filterAttrs (n: _: n != "swag-local") cfg.entries;
      configFile = pkgs.writeText "rathole-server.toml" ''
        [server]
        bind_addr = "0.0.0.0:2223"

        ${concatStringsSep "\n" (
          flatten (
            mapAttrsToList (name: entry: [
              "[server.services.${name}_http]"
              "token = \"${entry.token}\""
              "bind_addr = \"127.0.0.1:${toString (cfg.ports.${name}.http)}\""
              ""
              "[server.services.${name}_https]"
              "token = \"${entry.token}\""
              "bind_addr = \"127.0.0.1:${toString (cfg.ports.${name}.https)}\""
            ])
            nonSwagEntries
          )
        )}
      '';
    in
      mkIf (cfg.enabled && nonSwagEntries != {}) {
        networking.firewall.allowedTCPPorts = [2223];
        systemd.services.rathole-server = {
          description = "Rathole server";
          after = ["network.target"];
          wantedBy = ["multi-user.target"];
          serviceConfig = {
            ExecStart = "${pkgs.rathole}/bin/rathole --server ${configFile}";
            Restart = "always";
            DynamicUser = true;
          };
        };
      };
}
