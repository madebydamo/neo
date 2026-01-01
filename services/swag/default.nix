{
  config,
  lib,
  pkgs,
  ...
}:

with lib;

{
  imports = [ ./option.nix ];

  config =
    let
      cfg = config.neo.services.swag;
      appServices = filterAttrs (
        n: v: v.enabled && builtins.hasAttr "subdomain" v && v.subdomain != null && n != "swag"
      ) config.neo.services;
      subdomains = catAttrs "subdomain" (attrValues appServices);
      tmpfilesRules = flatten (
        attrValues (
          mapAttrs (n: svc: [
            "d ${config.neo.volumes.appdata}/swag/proxy-confs 0755 1000 1000 -"
            "C+ ${config.neo.volumes.appdata}/swag/proxy-confs/${svc.subdomain}.subdomain.conf - - - - ${pkgs.writeText "${svc.subdomain}.subdomain.conf" svc.proxyConf}"
          ]) appServices
        )
      );
    in
    mkIf cfg.enabled {
      systemd.tmpfiles.rules = tmpfilesRules;
      systemd.services.docker-internal-network = {
        description = "Create docker internal network";
        wantedBy = [ "docker.service" ];
        before = [ "docker.service" ];
        serviceConfig = {
          Type = "oneshot";
          ExecStart = "${pkgs.docker}/bin/docker network create internal";
          RemainAfterExit = true;
        };
      };
      virtualisation.oci-containers.containers.swag = {
        image = "lscr.io/linuxserver/swag:latest";
        autoStart = true;
        environment = {
          PUID = "1000";
          PGID = "1000";
          TZ = "Europe/Zurich";
          URL = cfg.domain;
          SUBDOMAINS = concatStringsSep "," subdomains;
          VALIDATION = "http";
          EMAIL = cfg.email;
          ONLY_SUBDOMAINS = "true";
          EXTRA_DOMAINS = concatStringsSep "," cfg.extraDomains;
        };
        volumes = [
          "${config.neo.volumes.appdata}/swag:/config"
          "${config.neo.volumes.appdata}/swag/proxy-confs:/config/nginx/proxy-confs"
        ];
        ports = [
          "80:80"
          "443:443"
        ];
        extraOptions = [
          "--cap-add=NET_ADMIN"
          "--network=internal"
        ];
      };
    };
}
