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
        n: v: v.enabled && v.subdomain != null && n != "swag"
      ) config.neo.services;
      subdomains = catAttrs "subdomain" (attrValues appServices);
      proxyVolumes = flatten (
        attrValues (
          mapAttrs (n: svc: [
            "${config.neo.volumes.appdata}/swag/nginx/proxy-confs/${svc.subdomain}.subdomain.conf:/config/nginx/proxy-confs/${svc.subdomain}.subdomain.conf"
          ]) appServices
        )
      );
      tmpfilesRules = flatten (
        attrValues (
          mapAttrs (n: svc: [
            "Z /DATA/appdata/swag/nginx/proxy-confs 0755 1000 1000 -"
            "L+ /DATA/appdata/swag/nginx/proxy-confs/${svc.subdomain}.subdomain.conf - - - - ${pkgs.writeText "${svc.subdomain}.subdomain.conf" svc.proxyConf}"
          ]) appServices
        )
      );
    in
    mkIf cfg.enabled {
      systemd.tmpfiles.rules = tmpfilesRules;
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
        ]
        ++ proxyVolumes;
        ports = [
          "80:80"
          "443:443"
        ];
        extraOptions = [
          "--cap-add=NET_ADMIN"
        ];
      };
    };
}
