# Backup store vhost for SWAG (docs/design/backup-store-rescue.md §1):
# - WebSocket requests: the tunnel on the host (host.docker.internal reaches it
#   via mkDockerToLocalhostForward), without tinyauth; SSH clients log in with
#   their key at the end of the tunnel.
# - Everything else: the rescue portal of the Neo web UI, behind the app's login.
#   X-Neo-Site makes neo web serve only the portal on this name.
# `rewrite … last` inside `if` is one of nginx's safe uses of `if`; the rewrite
# phase runs before auth_request, so the tunnel never sees tinyauth.
# proxy.conf already sets Upgrade/Connection and timeouts — do not re-set them.
{...}: {
  flake.modules.nixos.backup-store-swag = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.backup-store;
    webPort = toString config.neo.services.neo.port;
  in {
    config.neo.services.backup-store.proxyConf = lib.mkDefault (lib.neo.mkSubdomainProxyConf {
      inherit config cfg;
      proxyPass = "http://host.docker.internal:${webPort}";
      # mkSubdomainProxyConf adds these lines inside `location /` as they are.
      locationExtra = lib.concatMapStringsSep "\n" (l: "    ${l}") [
        ''if ($http_upgrade ~* "^websocket$") {''
        "  rewrite ^ /.neo-backup-store-tunnel last;"
        "}"
        "proxy_set_header X-Neo-Site backup-store;"
      ];
      extraLocations =
        "\n"
        + lib.concatMapStrings (l: "  ${l}\n") [
          "location = /.neo-backup-store-tunnel {"
          "  internal;"
          "  include /config/nginx/proxy.conf;"
          "  proxy_pass http://host.docker.internal:${toString cfg.tunnelPort}/;"
          "}"
        ];
    });
  };
}
