# Core NixOS configuration: openssh, users, activation scripts, timezone, stateVersion.
{...}: {
  flake.modules.nixos.core = {
    config,
    lib,
    pkgs,
    ...
  }: {
    # Keys-only when neo.core.ssh.authorizedKeys is non-empty (settings.toml).
    # Empty keys → password auth kept so first-boot / recovery is not locked out.
    services.openssh = let
      hasKeys = config.neo.core.ssh.authorizedKeys != [];
    in {
      enable = true;
      settings = {
        PermitRootLogin = "no";
        PasswordAuthentication = !hasKeys;
        KbdInteractiveAuthentication = !hasKeys;
      };
    };

    virtualisation.docker.enable = true;
    virtualisation.oci-containers.backend = "docker";

    users.allowNoPasswordLogin = true;
    users.mutableUsers = false;

    zramSwap = lib.mkIf config.neo.core.zramSwap.enabled {
      enable = true;
    };

    services.earlyoom = {
      enable = true;
      freeMemKillThreshold = 2;
    };

    boot.kernel.sysctl = {
      "net.ipv4.ip_forward" = 1;
      "net.ipv6.conf.all.forwarding" = 1;
    };

    systemd.services.NetworkManager-wait-online.enable = false;

    users.groups.homeserver.gid = config.neo.core.gid;

    users.users.homeserver = {
      uid = config.neo.core.uid;
      group = "homeserver";
      isNormalUser = true;
      home = "/home/homeserver";
      createHome = true;
      extraGroups = ["docker" "wheel" "systemd-journal"];
      # Machine keys log in here only (docs/design/ssh-key-roles.md).
      openssh.authorizedKeys.keys = config.neo.core.ssh.authorizedKeys ++ config.neo.core.ssh.machineKeys;
      # Empty means locked (no password login). SSH keys are the intended login.
      # A set hash is for local console / sudo, not a shipped default password.
      hashedPassword =
        if config.neo.core.hashedLinuxPassword == ""
        then "!"
        else config.neo.core.hashedLinuxPassword;
    };

    # Allow other neo hosts to use this machine as a remote Nix builder over SSH as homeserver.
    nix.settings.trusted-users = ["homeserver"];

    # A key is a person's (logs in, decrypts) or a machine's (homeserver only,
    # never decrypts), not both: compare type and key, not the comment.
    assertions = let
      norm = k: let
        p = lib.filter (x: x != "") (lib.splitString " " (lib.replaceStrings ["\t" "\n" "\r"] [" " " " " "] k));
      in
        lib.concatStringsSep " " (lib.take 2 p);
      people = map norm config.neo.core.ssh.authorizedKeys;
      both = lib.filter (k: lib.elem (norm k) people) config.neo.core.ssh.machineKeys;
    in [
      {
        assertion = both == [];
        message = "core.ssh.machineKeys: these keys are also in core.ssh.authorizedKeys; keep each key in one list (people: authorizedKeys, machines: machineKeys): ${lib.concatStringsSep ", " both}";
      }
    ];

    system.activationScripts.create-volumes = lib.neo.mkEnsureDirs config [
      config.neo.core.volumes.root
      config.neo.core.volumes.data
      config.neo.core.volumes.appdata
      config.neo.core.volumes.media
      config.neo.core.volumes.documents
    ];

    system.stateVersion = "24.11";
  };
}
