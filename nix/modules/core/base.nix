# Base system configuration: bootloader, networking, i18n, users (admin), nix settings.
{...}: {
  flake.nixosModules.base = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.core;
  in {
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

    environment.systemPackages = with pkgs; [
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
    ];

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
      hashedPassword = config.neo.core.hashedLinuxPassword;
    };
    time.timeZone = config.neo.core.timeZone;

    nix.settings = lib.mkMerge [
      (lib.optionalAttrs (cfg.nix.maxJobs != null) {
        max-jobs = cfg.nix.maxJobs;
      })
      (lib.optionalAttrs (cfg.nix.cores != null) {
        cores = cfg.nix.cores;
      })
      {
        experimental-features = [
          "nix-command"
          "flakes"
        ];
      }
    ];
  };
}
