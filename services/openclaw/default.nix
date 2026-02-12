{
  config,
  lib,
  ...
}:
with lib; let
  cfg = config.neo.services.openclaw;
  # Generate token if not provided
  gatewayToken =
    if cfg.gatewayToken != null
    then cfg.gatewayToken
    else "auto-generated-token-placeholder"; # In practice, this should be set manually
in
  {
    imports = [
      ./option.nix
      ./swag.nix
    ];

    system.activationScripts.create-openclaw-dirs = lib.concatStringsSep "\n" [
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/openclaw";
        user = toString config.neo.uid;
        group = toString config.neo.gid;
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/openclaw/config";
        user = toString config.neo.uid;
        group = toString config.neo.gid;
      })
      (lib.neo.mkActivationScriptForDir config {
        dirPath = "${config.neo.volumes.appdata}/openclaw/workspace";
        user = toString config.neo.uid;
        group = toString config.neo.gid;
      })
    ];
  }
  // (mkIf cfg.enabled {
    virtualisation.oci-containers.containers.openclaw-gateway = {
      user = "0:0";
      environment =
        {
          HOME = "/home/node";
          TERM = "xterm-256color";
          CLAWDBOT_GATEWAY_TOKEN = gatewayToken;
          OPENCLAW_TRUSTED_PROXY_NETWORK = "172.21.0.0/19";
        }
        // (lib.optionalAttrs (cfg.claudeAiSessionKey != null) {
          CLAUDE_AI_SESSION_KEY = cfg.claudeAiSessionKey;
        })
        // (lib.optionalAttrs (cfg.claudeWebSessionKey != null) {
          CLAUDE_WEB_SESSION_KEY = cfg.claudeWebSessionKey;
        })
        // (lib.optionalAttrs (cfg.claudeWebCookie != null) {
          CLAUDE_WEB_COOKIE = cfg.claudeWebCookie;
        });
      image = cfg.image;
      autoStart = true;
      volumes =
        [
          "${config.neo.volumes.appdata}/openclaw/node:/home/node/"
        ]
        ++ (lib.flatten (
          lib.attrValues (
            lib.mapAttrs (
              hostVol: containerPaths: lib.map (p: "${config.neo.volumes.${hostVol}}:${p}") containerPaths
            )
            cfg.additionalMountPoints
          )
        ));
      ports = [
        "${toString cfg.gatewayPort}:18789"
        "${toString cfg.bridgePort}:18790"
      ];
      extraOptions = [
        "--network=internal"
        # "--entrypoint"
        # "sh"
      ];
      # cmd = [
      #   "-c"
      #   "node /app/dist/index.js gateway start & sleep infinity"
      # ];
      # cmd = [
      #   "gateway"
      #   "run"
      # ];
    };
  })
