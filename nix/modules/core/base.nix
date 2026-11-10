# Base system configuration: bootloader, networking, i18n, users (admin), nix settings.
{...}: {
  flake.nixosModules.base = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.core;
    uid = toString config.neo.core.uid;
    gid = toString config.neo.core.gid;
    # Shared by activation (ensure) and neo-web rotate (rm + ensure).
    homeserverSshKey = lib.neo.mkSshKeyScript pkgs {
      name = "neo-homeserver-ssh-key";
      path = lib.neo.defaultHomeserverSshKey;
      owner = uid;
      group = gid;
      comment = "homeserver@${cfg.hostname}";
      requireDir = "/home/homeserver";
    };
  in {
    # The lib.neo.mkSudoCommand rules glob /nix/store/*-<pname>-*/bin/… With
    # glob(3) sudo lists the whole store for each such rule on every call
    # (about 2 s per `sudo -n` from neo-web in the VM); fnmatch(3) matches the
    # same paths (`*` never crosses `/`) without reading the filesystem.
    security.sudo.extraConfig = ''
      Defaults fast_glob
    '';

    boot.loader = {
      grub = {
        enable = true;
        efiSupport = true;
        efiInstallAsRemovable = true;
        device = "nodev";
        default = "saved";
        configurationLimit = 10;
      };
      efi.canTouchEfiVariables = false;
    };

    networking.hostName = cfg.hostname;

    i18n = {
      defaultLocale = "en_US.UTF-8";
      supportedLocales = ["en_US.UTF-8/UTF-8"];
    };

    console = {
      font = "Lat2-Terminus16";
      keyMap = "sg";
    };

    environment.systemPackages =
      (with pkgs; [
        vim
        btop
        docker_29
        netcat
        curl
        dnsutils
        iputils
        iproute2
        traceroute
        mtr
      ])
      ++ [homeserverSshKey];

    users.users.root = {
      openssh.authorizedKeys.keys = config.neo.core.ssh.authorizedKeys;
    };

    users.users.admin = {
      uid = config.neo.core.uid + 1;
      isNormalUser = true;
      extraGroups = [
        "wheel"
        "docker"
      ];
      openssh.authorizedKeys.keys = config.neo.core.ssh.authorizedKeys;
      # Same hash as homeserver. Empty hashedLinuxPassword locks password login (`!`).
      hashedPassword =
        if cfg.hashedLinuxPassword == ""
        then "!"
        else cfg.hashedLinuxPassword;
    };

    time.timeZone = config.neo.core.timeZone;

    system.activationScripts.homeserver-ssh-key = ''
      ${homeserverSshKey}/bin/neo-homeserver-ssh-key ensure
    '';

    # Which system ran when (boot, switch, test; not `boot`-only switches), so
    # ZFS data snapshots map to the generation that was actually running
    # (cli/src/utils/generation.rs). On the root fs: a data restore keeps it.
    system.activationScripts.neo-system-history = ''
      mkdir -p /var/lib/neo
      log=/var/lib/neo/system-activations
      echo "$(date +%s) $systemConfig" >> "$log"
      if [ "$(wc -l < "$log")" -gt 5000 ]; then
        tail -n 2000 "$log" > "$log.tmp" && mv "$log.tmp" "$log"
      fi
    '';

    nix = let
      rb = cfg.nix.remoteBuild;
    in
      lib.mkMerge [
        {
          settings = lib.mkMerge [
            (lib.optionalAttrs (cfg.nix.maxJobs != null) {
              max-jobs = cfg.nix.maxJobs;
            })
            (lib.optionalAttrs (cfg.nix.cores != null) {
              cores = cfg.nix.cores;
            })
            (lib.optionalAttrs rb.enabled {
              # Let remotes fetch from binary caches themselves instead of the weak local box
              # uploading every source/input (see nix.conf "builders-use-substitutes").
              builders-use-substitutes = true;
            })
            {
              # admin is defined in this module; allow it as a remote-build SSH user.
              trusted-users = ["admin"];
              experimental-features = [
                "nix-command"
                "flakes"
              ];
            }
          ];
        }
        (lib.mkIf rb.enabled {
          distributedBuilds = true;
          buildMachines = [
            (
              {
                hostName = rb.host;
                sshUser = rb.user;
                sshKey = rb.sshKey;
                system = rb.system;
                protocol = "ssh-ng";
                maxJobs = rb.maxJobs;
                speedFactor = rb.speedFactor;
                supportedFeatures = rb.supportedFeatures;
                mandatoryFeatures = [];
              }
              // lib.optionalAttrs (rb.publicHostKey != null) {
                publicHostKey = rb.publicHostKey;
              }
            )
          ];
        })
      ];

    # nix-daemon invokes ssh for remote builders; forward extra SSH flags.
    systemd.services.nix-daemon = lib.mkIf (cfg.nix.remoteBuild.enabled && cfg.nix.remoteBuild.extraOptions != []) {
      environment.NIX_SSHOPTS = lib.concatStringsSep " " cfg.nix.remoteBuild.extraOptions;
    };
  };
}
