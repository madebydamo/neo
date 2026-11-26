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
    # Active remote builders ordered by weight (highest speedFactor first,
    # tie-breaking by name) so /etc/nix/machines is deterministic and prefers
    # higher-weight machines.
    activeBuilders =
      lib.sort (a: b:
        if a.value.speedFactor != b.value.speedFactor
        then a.value.speedFactor > b.value.speedFactor
        else a.name < b.name)
      (lib.filter (entry: entry.value.enabled)
        (lib.mapAttrsToList (name: value: {
            inherit name value;
            alias = "neo-builder-${lib.strings.sanitizeDerivationName name}";
            needsSshConfig = value.port != null || value.sshOptions != {};
          })
          cfg.nix.builders));

    buildersWithSshConfig = lib.filter (b: b.needsSshConfig) activeBuilders;
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
        "systemd-journal"
      ];
      openssh.authorizedKeys.keys = config.neo.core.ssh.authorizedKeys;
      # Same hash as homeserver. Empty hashedLinuxPassword locks password login (`!`).
      hashedPassword =
        if cfg.hashedLinuxPassword == ""
        then "!"
        else cfg.hashedLinuxPassword;
    };

    time.timeZone = config.neo.core.timeZone;
    networking.timeServers = config.neo.core.timeServers;

    system.activationScripts.homeserver-ssh-key = ''
      ${homeserverSshKey}/bin/neo-homeserver-ssh-key ensure
    '';

    # Which system ran when (boot, switch, test; not `boot`-only switches), so
    # ZFS data snapshots map to the generation that was actually running
    # (cli/src/utils/generation/timeline.rs). On the root fs: a data restore keeps it.
    system.activationScripts.neo-system-history = ''
      mkdir -p /var/lib/neo
      log=/var/lib/neo/system-activations
      echo "$(date +%s) $systemConfig" >> "$log"
      if [ "$(wc -l < "$log")" -gt 5000 ]; then
        tail -n 2000 "$log" > "$log.tmp" && mv "$log.tmp" "$log"
      fi
    '';

    assertions =
      lib.concatMap (b: [
        {
          assertion = b.value.host != "";
          message = "core.nix.builders.${b.name}: host must not be empty when enabled.";
        }
        {
          assertion = b.value.user != "";
          message = "core.nix.builders.${b.name}: user must not be empty when enabled.";
        }
        {
          assertion = b.value.systems != [];
          message = "core.nix.builders.${b.name}: systems must not be empty when enabled.";
        }
      ])
      activeBuilders;

    nix = lib.mkMerge [
      {
        settings = lib.mkMerge [
          (lib.optionalAttrs (cfg.nix.maxJobs != null) {
            max-jobs = cfg.nix.maxJobs;
          })
          (lib.optionalAttrs (cfg.nix.cores != null) {
            cores = cfg.nix.cores;
          })
          (lib.optionalAttrs (activeBuilders != []) {
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
      (lib.mkIf (activeBuilders != []) {
        distributedBuilds = true;
        buildMachines = map (b:
          {
            hostName =
              if b.needsSshConfig
              then b.alias
              else b.value.host;
            sshUser = b.value.user;
            sshKey = b.value.sshKey;
            systems = b.value.systems;
            protocol = "ssh-ng";
            maxJobs = b.value.maxJobs;
            speedFactor = b.value.speedFactor;
            supportedFeatures = b.value.supportedFeatures;
            mandatoryFeatures = b.value.mandatoryFeatures;
          }
          // lib.optionalAttrs (b.value.publicHostKey != null) {
            publicHostKey = b.value.publicHostKey;
          })
        activeBuilders;
      })
    ];

    # Per-builder SSH options live in /etc/ssh/ssh_config under a dedicated
    # Host alias so multiple builders never collide in NIX_SSHOPTS.
    # `Match all` closes the block so later ssh_config lines stay global.
    programs.ssh.extraConfig = lib.mkIf (buildersWithSshConfig != []) (
      lib.concatMapStrings (b: ''
        Host ${b.alias}
          HostName ${b.value.host}
        ${lib.optionalString (b.value.port != null) "  Port ${toString b.value.port}\n"}${lib.optionalString (b.value.publicHostKey != null) "  HostKeyAlias ${b.alias}\n"}${lib.concatStrings (lib.mapAttrsToList (k: v: "  ${k} ${v}\n") b.value.sshOptions)}
      '')
      buildersWithSshConfig
      + "Match all\n"
    );
  };
}
