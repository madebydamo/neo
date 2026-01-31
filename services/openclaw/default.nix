{
  config,
  lib,
  ...
}:
with lib;
let
  cfg = config.neo.services.openclaw;
  # Generate token if not provided
  gatewayToken =
    if cfg.gatewayToken != null then cfg.gatewayToken else "auto-generated-token-placeholder"; # In practice, this should be set manually
in
{
  imports = [
    ./option.nix
    ./swag.nix
  ];

  system.activationScripts.create-openclaw-dirs = lib.concatStringsSep "\n" [
    (lib.neo.mkActivationScriptForDir {
      dirPath = "${config.neo.volumes.appdata}/openclaw";
      user = "0";
      group = "0";
    })
    (lib.neo.mkActivationScriptForDir {
      dirPath = "${config.neo.volumes.appdata}/openclaw/config";
      user = "1000";
      group = "1000";
    })
    (lib.neo.mkActivationScriptForDir {
      dirPath = "${config.neo.volumes.appdata}/openclaw/workspace";
      user = "1000";
      group = "1000";
    })
  ];
}
// (mkIf cfg.enabled {
  virtualisation.oci-containers.containers.openclaw-gateway = {
    # user = "0:0";
    environment = {
      HOME = "/home/node";
      TERM = "xterm-256color";
      OPENCLAW_GATEWAY_TOKEN = gatewayToken;
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
    volumes = [
      "${config.neo.volumes.appdata}/openclaw/config:/home/node/.openclaw"
      "${config.neo.volumes.appdata}/openclaw/workspace:/home/node/.openclaw/workspace"
    ]
    ++ (lib.flatten (
      lib.attrValues (
        lib.mapAttrs (
          hostVol: containerPaths: lib.map (p: "${config.neo.volumes.${hostVol}}:${p}") containerPaths
        ) cfg.additionalMountPoints
      )
    ));
    ports = [
      "${toString cfg.gatewayPort}:18789"
      "${toString cfg.bridgePort}:18790"
    ];
    extraOptions = [
      "--network=internal"
    ];
    cmd = [
      "gateway"
      "start"
    ];
  };
})
