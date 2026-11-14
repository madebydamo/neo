# Gitea service implementation (official image, SQLite).
{...}: {
  flake.modules.nixos.gitea = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.gitea;
      appdata = cfg.appdata;
      rootUrl =
        if cfg.rootUrl != null && cfg.rootUrl != ""
        then cfg.rootUrl
        else if (cfg.customDomains or []) != []
        then "https://${builtins.head cfg.customDomains}"
        else lib.defaultTo "http://localhost:${toString cfg.port}" (lib.neo.publicUrl config cfg);
      rootHost = let
        stripped = lib.removePrefix "https://" (lib.removePrefix "http://" rootUrl);
      in
        builtins.head (lib.splitString "/" stripped);
      sshDomain =
        if cfg.ssh.domain != null && cfg.ssh.domain != ""
        then cfg.ssh.domain
        else rootHost;
      # The rootful gitea/gitea image always runs its bundled OpenSSH (s6) on
      # container :22 with host keys in /data/ssh. Gitea's built-in Go SSH server
      # would also bind :22 and crash ("address already in use"), so keep it off
      # and let OpenSSH serve git over the gitea-managed authorized_keys file.
      # Only GITEA__server__* vars are set: plain SSH_PORT/SSH_LISTEN_PORT would
      # move the bundled sshd off :22 (see /etc/s6/openssh/setup).
      sshEnv =
        if cfg.ssh.enable
        then {
          GITEA__server__START_SSH_SERVER = "false";
          GITEA__server__DISABLE_SSH = "false";
          GITEA__server__SSH_DOMAIN = sshDomain;
          GITEA__server__SSH_PORT = toString cfg.ssh.port;
          GITEA__server__SSH_LISTEN_PORT = "22";
          GITEA__server__SSH_CREATE_AUTHORIZED_KEYS_FILE = "true";
        }
        else {
          GITEA__server__DISABLE_SSH = "true";
        };
      sshPorts =
        optional cfg.ssh.enable "127.0.0.1:${toString cfg.ssh.listenPort}:22";
    in {
      config = mkIf cfg.enabled {
        systemd.services.docker-gitea.preStart = lib.neo.mkEnsureDirs config [
          "${appdata}/data"
        ];

        virtualisation.oci-containers.containers.gitea = {
          image = cfg.containers.gitea;
          environment =
            {
              USER_UID = "1000";
              USER_GID = "1000";
              GITEA__server__DOMAIN = rootHost;
              GITEA__server__ROOT_URL = "${rootUrl}/";
              GITEA__server__HTTP_PORT = toString cfg.port;
              GITEA__database__DB_TYPE = "sqlite3";
              GITEA__service__DISABLE_REGISTRATION = lib.boolToString cfg.disableRegistration;
              GITEA__security__INSTALL_LOCK = "true";
            }
            // sshEnv;
          volumes = [
            "${appdata}/data:/data"
          ];
          ports = sshPorts;
          networks = [(lib.neo.docker.serviceNetwork "gitea")];
        };
      };
    };
}
