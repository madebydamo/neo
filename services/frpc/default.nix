{
  config,
  lib,
  pkgs,
  ...
}:

with lib;

let
  cfg = config.neo.services.frpc;
  frpcConfig = pkgs.writeText "frpc.toml" ''
    [common]
    server_addr = ${cfg.serverAddr}
    server_port = ${toString cfg.serverPort}
    auth.token = ${cfg.token}

    [[proxies]]
    name = "http-proxy"
    type = "http"
    custom_domains = ["filebrowser.damianmoser.ch"]
    localPort = 80

    [[proxies]]
    name = "https-proxy"
    type = "https"
    custom_domains = ["filebrowser.damianmoser.ch"]
    localPort = 443
  '';
in
{
  imports = [ ./option.nix ];

  systemd.tmpfiles.rules = mkIf cfg.enabled [
    "d ${config.neo.volumes.appdata}/frpc 0755 0 0 -"
  ];
  systemd.services.frpc = mkIf cfg.enabled {
    description = "FRP Client";
    wantedBy = [ "multi-user.target" ];
    after = [ "network.target" ];
    serviceConfig = {
      ExecStart = "${pkgs.frp}/bin/frpc -c ${frpcConfig}";
      Restart = "on-failure";
    };
  };
}
