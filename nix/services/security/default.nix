# Security scan implementation.
#
# neo-security-scan (root, timer): `neo security scan` runs vulnxscan over
# /run/current-system and Trivy over every local Docker image, filters and
# groups the findings, then has the AI review the groups it has not seen yet
# (cli/src/commands/security/). Root because it reads the Docker socket and
# /proc/<pid>/maps of every process (what a vulnerable library is loaded by).
#
# neo-security-web (homeserver): `neo security serve`, the read-only report
# page on 127.0.0.1:port; SWAG + tinyauth in front. It only reads report.json.
#
# Scanner databases (NVD for vulnix, grype, Trivy, OSV cache) are caches in
# /var/cache/neo-security, not appdata: no snapshots or backups of them.
{self, ...}: {
  flake.modules.nixos.security = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.security;
    neoPkg = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
    hermes = config.neo.services.hermes;
    appdata = cfg.appdata;
    cacheDir = "/var/cache/neo-security";

    # Hermes' API server runs when its gateway token is set (≥16 chars).
    hermesApiOn =
      (hermes.enabled or false)
      && hermes.gatewayToken != null
      && builtins.stringLength hermes.gatewayToken >= 16;
    aiSource =
      if cfg.ai.source == "auto"
      then
        (
          if hermesApiOn
          then "hermes"
          else "off"
        )
      else cfg.ai.source;
    useHermes = aiSource == "hermes" && hermesApiOn;
    useCustom = aiSource == "custom" && lib.neo.isSet cfg.ai.baseUrl;
    customKeyOn = useCustom && lib.neo.isSet cfg.ai.apiKey;

    # Which service owns which unit and container, its address and how it is
    # reached: the context the review weighs a finding against.
    serviceContext = lib.mapAttrs (name: svc: let
      units = svc.systemdUnits or [];
      containerNames = map (u: lib.removePrefix "docker-" u) (builtins.filter (lib.hasPrefix "docker-") units);
    in {
      inherit units;
      url = lib.neo.publicUrl config svc;
      ingress = svc.ingress or [];
      auth = (svc.subdomain or null) != null && (svc.auth.enabled or false);
      containers = lib.listToAttrs (lib.concatMap (c: let
        oci = config.virtualisation.oci-containers.containers.${c} or null;
      in
        lib.optional (oci != null) (lib.nameValuePair c oci.image))
      containerNames);
    }) (lib.filterAttrs (_: svc: builtins.isAttrs svc && (svc.enabled or false)) config.neo.services);

    securityConfig = pkgs.writeText "neo-security.json" (builtins.toJSON {
      inherit (cfg) minSeverity;
      dataDir = appdata;
      theme = config.neo.services.neo.theme or "light";
      ai = {
        source =
          if useHermes
          then "hermes"
          else if useCustom
          then "custom"
          else "off";
        baseUrl =
          if useHermes
          then "http://127.0.0.1:${toString hermes.gatewayPort}/v1"
          else cfg.ai.baseUrl;
        model = cfg.ai.model;
        keyFile =
          if useHermes || customKeyOn
          then config.neo.core.secrets.templates."neo-security-ai-key".path
          else null;
        inherit (cfg.ai) maxReviews;
      };
      services = serviceContext;
    });

    environment = {
      NEO_SECURITY_CONFIG = toString securityConfig;
      NEO_SECURITY_DIR = appdata;
      DOCKER_BINARY_PATH = "${config.virtualisation.docker.package}/bin/docker";
    };
  in {
    config = lib.mkIf cfg.enabled (lib.mkMerge [
      (lib.neo.mkDockerToLocalhostForward cfg.port)
      {
        assertions = [
          {
            assertion = cfg.ai.source != "hermes" || hermesApiOn;
            message = "services.security.ai.source = \"hermes\" needs Hermes enabled with a gateway token (its API server).";
          }
          {
            assertion = cfg.ai.source != "custom" || lib.neo.isSet cfg.ai.baseUrl;
            message = "services.security.ai.source = \"custom\" needs services.security.ai.baseUrl.";
          }
        ];

        # Root only: the scan sends it as Bearer to the AI service.
        neo.core.secrets.templates."neo-security-ai-key" = lib.mkIf (useHermes || customKeyOn) {
          content =
            if useHermes
            then hermes.gatewayToken
            else cfg.ai.apiKey;
          owner = "root";
          group = "root";
          mode = "0400";
        };

        systemd.tmpfiles.rules = [
          "d ${appdata} 0755 homeserver homeserver -"
        ];

        # `neo security show|accept|triage` from a root shell finds the data
        # and the AI settings here (the units get it through the env).
        environment.etc."neo/security.json".source = securityConfig;

        systemd.services.neo-security-scan = {
          description = "Neo security scan (system + container images)";
          after = ["network-online.target" "docker.service"] ++ lib.optional useHermes "hermes-agent.service";
          wants = ["network-online.target"];
          inherit environment;
          path = [
            neoPkg
            pkgs.sbomnix
            pkgs.vulnix
            pkgs.grype
            pkgs.trivy
            # OSV records and the AI endpoint (key on stdin).
            pkgs.curl
            pkgs.nix
            pkgs.git
            pkgs.coreutils
            config.virtualisation.docker.package
          ];
          serviceConfig = {
            Type = "oneshot";
            User = "root";
            # vulnix, grype and the OSV cache write below $HOME / the cwd.
            CacheDirectory = "neo-security";
            WorkingDirectory = cacheDir;
            Environment = [
              "HOME=${cacheDir}"
              "XDG_CACHE_HOME=${cacheDir}"
              "GRYPE_DB_CACHE_DIR=${cacheDir}/grype"
              "TRIVY_CACHE_DIR=${cacheDir}/trivy"
            ];
            ExecStart = "${neoPkg}/bin/neo security scan";
            Nice = 10;
            IOSchedulingClass = "idle";
            TimeoutStartSec = "4h";
          };
        };

        systemd.timers.neo-security-scan = {
          description = "Neo security scan schedule";
          wantedBy = ["timers.target"];
          timerConfig = {
            OnCalendar = cfg.schedule;
            Persistent = true;
            RandomizedDelaySec = "1h";
          };
        };

        systemd.services.neo-security-web = {
          description = "Neo security report page";
          wantedBy = ["multi-user.target"];
          after = ["network-online.target"];
          wants = ["network-online.target"];
          environment =
            environment
            // {
              ROCKET_ADDRESS = "127.0.0.1";
              ROCKET_PORT = toString cfg.port;
            };
          serviceConfig = {
            User = "homeserver";
            Group = "homeserver";
            ExecStart = "${neoPkg}/bin/neo security serve";
            Restart = "always";
            RestartSec = 5;
            ProtectSystem = "strict";
            ProtectHome = true;
            PrivateTmp = true;
            NoNewPrivileges = true;
            ReadOnlyPaths = [appdata];
          };
        };
      }
    ]);
  };
}
