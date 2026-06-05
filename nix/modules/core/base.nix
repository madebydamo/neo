# Base system configuration: bootloader, networking, i18n, users (admin), nix settings.
{...}: {
  flake.nixosModules.base = {
    config,
    lib,
    pkgs,
    ...
  }: let
    deviceCfg = config.neo.core;
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

    networking.hostName = deviceCfg.hostname;

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
      docker
      netcat
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

    nix.settings.experimental-features = [
      "nix-command"
      "flakes"
    ];
  };
}
