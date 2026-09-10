# Unknown-host 404: default_server on 80/443/PROXY-protocol so unmatched
# SNI never lands on an arbitrary proxied service (wrong cert + wrong app).
{...}: {
  flake.modules.nixos.swag-not-found = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.swag;
      appdataSwag = "${config.neo.core.volumes.appdata}/swag";
      ppContainerPort = toString lib.neo.httpsProxyProtocolContainerPort;
      neoHome =
        if cfg.domain == null
        then "/"
        else "https://neo.${cfg.domain}";
      defaultConf =
        replaceStrings
        ["__PP_PORT__"]
        [ppContainerPort]
        (builtins.readFile ./not-found/default.conf);
      pages = ./not-found/p;
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-swag.preStart = mkAfter ''
          mkdir -p ${appdataSwag}/nginx/site-confs
          cat > ${appdataSwag}/nginx/site-confs/default.conf << 'ACTEOF'
          ${defaultConf}
          ACTEOF
          chown ${toString config.neo.core.uid}:${toString config.neo.core.gid} ${appdataSwag}/nginx/site-confs/default.conf
          chmod 0644 ${appdataSwag}/nginx/site-confs/default.conf

          rm -rf ${appdataSwag}/www/neo-404
          mkdir -p ${appdataSwag}/www/neo-404
          cp -a --no-preserve=mode,ownership ${pages} ${appdataSwag}/www/neo-404/p
          find ${appdataSwag}/www/neo-404 -type f -name '*.html' -exec sed -i 's|__NEO_HOME__|${neoHome}|g' {} +
          find ${appdataSwag}/www/neo-404 -type f -name '*.test.js' -delete
          chown -R ${toString config.neo.core.uid}:${toString config.neo.core.gid} ${appdataSwag}/www/neo-404
        '';
      };
    };
}
