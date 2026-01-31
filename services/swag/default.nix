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
        lib.neo.mkActivationScriptForFile {
          filePath = "${config.neo.volumes.appdata}/swag/proxy-confs/${svc.subdomain}.subdomain.conf";
          content = svc.proxyConf;
          mode = "0644";
          user = toString config.neo.uid;
          group = toString config.neo.gid;
        }
    ) (attrValues appServices);
  in
    mkIf cfg.enabled {
      system.activationScripts.create-swag-dirs = lib.concatStringsSep "\n" [
        (lib.neo.mkActivationScriptForDir {
          dirPath = "${config.neo.volumes.appdata}/swag";
          user = toString config.neo.uid;
          group = toString config.neo.gid;
        })
        (lib.neo.mkActivationScriptForDir {
          dirPath = "${config.neo.volumes.appdata}/swag/proxy-confs";
          user = toString config.neo.uid;
          group = toString config.neo.gid;
        })
      ];

      system.activationScripts.swag-proxy-confs = concatStringsSep "\n" proxyConfScripts;
      systemd.services.docker-internal-network = {
        description = "Create docker internal network";
        wantedBy = ["multi-user.target"];
        after = ["docker.service"];
        serviceConfig = {
          Type = "oneshot";
          ExecStart = "/bin/sh -c '${pkgs.docker}/bin/docker network ls --format \"{{.Name}}\" | grep -q \"^internal$\" || ${pkgs.docker}/bin/docker network create internal'";
          RemainAfterExit = true;
        };
      };
      virtualisation.oci-containers.containers.swag = {
        user = "${toString config.neo.uid}:${toString config.neo.gid}";
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
