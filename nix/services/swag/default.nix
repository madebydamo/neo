# SWAG reverse proxy service implementation.
{...}: {
  flake.modules.nixos.swag = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib; {
      config = let
        cfg = config.neo.services.swag;
        appServices =
          filterAttrs (
            n: v: v.enabled && v.subdomain or null != null && n != "swag"
          )
          config.neo.services;
        subdomains = catAttrs "subdomain" (attrValues appServices);
        proxyConfScripts = map (
          svc:
            lib.neo.mkActivationScriptForFile config {
              filePath = "${config.neo.volumes.appdata}/swag/nginx/proxy-confs/${svc.subdomain}.subdomain.conf";
              content = svc.proxyConf;
            }
        ) (attrValues appServices);
      in
        mkIf cfg.enabled {
          networking.firewall.extraCommands = "
            iptables -I nixos-fw 1 -i br+ -j ACCEPT
          ";
          networking.firewall.extraStopCommands = "
            iptables -D nixos-fw -i br+ -j ACCEPT
          ";
          systemd.services.docker-swag.preStart = lib.concatStringsSep "\n" ([
              "/bin/sh -c '${pkgs.docker}/bin/docker network ls --format \"{{.Name}}\" | grep -q \"^internal$\" || ${pkgs.docker}/bin/docker network create internal'"
              (lib.neo.mkActivationScriptForDir config {
                dirPath = "${config.neo.volumes.appdata}/swag/nginx/proxy-confs";
              })
              (lib.neo.mkActivationScriptForDir config {
                dirPath = "${config.neo.volumes.appdata}/swag/nginx";
              })
              (lib.neo.mkActivationScriptForDir config {
                dirPath = "${config.neo.volumes.appdata}/swag";
              })
            ]
            ++ proxyConfScripts);
          virtualisation.oci-containers.containers.swag = {
            image = "lscr.io/linuxserver/swag:latest";
            autoStart = true;
            environment = {
              PUID = toString config.neo.uid;
              PGID = toString config.neo.gid;
              TZ = "Europe/Zurich";
              URL = cfg.domain;
              SUBDOMAINS = concatStringsSep "," subdomains;
              VALIDATION = "http";
              EMAIL = cfg.email;
              ONLY_SUBDOMAINS = boolToString cfg.onlySubdomains;
              EXTRA_DOMAINS = concatStringsSep "," cfg.extraDomains;
            };
            volumes = [
              "${config.neo.volumes.appdata}/swag:/config"
            ];
            ports = [
              "80:80"
              "443:443"
            ];
            capabilities = {
              NET_ADMIN = true;
            };
            extraOptions = [
              "--network=internal"
              "--add-host=host.docker.internal:host-gateway"
            ];
          };
        };
    };
}
