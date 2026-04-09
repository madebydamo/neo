{...}: {
  flake.nixosModules.base = {
    config,
    lib,
    pkgs,
    ...
  }: let
    deviceCfg = config.neo.device;
  in {
    boot.loader.grub.enable = false;
    boot.loader.systemd-boot.enable = true;
    boot.loader.efi.canTouchEfiVariables = true;

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
      openssh.authorizedKeys.keys = config.neo.ssh.authorizedKeys;
    };

    users.users.admin = {
      isNormalUser = true;
      extraGroups = [
        "wheel"
        "docker"
      ];
      openssh.authorizedKeys.keys = config.neo.ssh.authorizedKeys;
    };
    time.timeZone = config.neo.timeZone;

    nix.settings.experimental-features = [
      "nix-command"
      "flakes"
    ];
  };
}
