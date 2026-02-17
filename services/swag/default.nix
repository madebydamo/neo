{
  config,
  lib,
  pkgs,
  ...
}:
with lib; {
  imports = [./option.nix];

  config = let
    cfg = config.neo.services.swag;
    appServices =
      filterAttrs (
        n: v: v.enabled && builtins.hasAttr "subdomain" v && v.subdomain != null && n != "swag"
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
      system.activationScripts.create-swag-dirs = lib.concatStringsSep "\n" [
        (lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/swag/nginx/proxy-confs";
        })
        (lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/swag/nginx";
        })
        (lib.neo.mkActivationScriptForDir config {
          dirPath = "${config.neo.volumes.appdata}/swag";
        })
      ];

      system.activationScripts.swag-proxy-confs = lib.concatStringsSep "\n" proxyConfScripts;

      systemd.services.oci-internal-network = {
        description = "Create oci internal network";
        wantedBy = ["multi-user.target"];
        after = ["podman.service"];
        serviceConfig = {
          Type = "oneshot";
          ExecStart = "/bin/sh -c '${pkgs.docker}/bin/docker network ls --format \"{{.Name}}\" | grep -q \"^internal$\" || ${pkgs.docker}/bin/docker network create internal'";
          RemainAfterExit = true;
        };
      };
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
          ONLY_SUBDOMAINS = "true";
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
        ];
      };
    };
}
