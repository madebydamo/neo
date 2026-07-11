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
    homeserverSshKey = pkgs.writeShellScriptBin "neo-homeserver-ssh-key" ''
      set -euo pipefail
      SSH_DIR=/home/homeserver/.ssh
      KEY="$SSH_DIR/id_ed25519"
      OWNER_UID=${uid}
      OWNER_GID=${gid}
      COMMENT="homeserver@${cfg.hostname}"

      mode="''${1:-ensure}"
      case "$mode" in
        ensure|rotate) ;;
        *)
          echo "usage: neo-homeserver-ssh-key [ensure|rotate]" >&2
          exit 2
          ;;
      esac

      if [ ! -d /home/homeserver ]; then
        echo "homeserver home missing at /home/homeserver" >&2
        exit 1
      fi

      if [ ! -d "$SSH_DIR" ]; then
        mkdir -p "$SSH_DIR"
        chmod 700 "$SSH_DIR"
        if [ "$(id -u)" -eq 0 ]; then
          chown "$OWNER_UID:$OWNER_GID" "$SSH_DIR"
        fi
      fi

      if [ "$mode" = rotate ]; then
        rm -f "$KEY" "$KEY.pub"
      fi

      if [ ! -f "$KEY" ]; then
        ${pkgs.openssh}/bin/ssh-keygen -t ed25519 -N "" -f "$KEY" -C "$COMMENT"
        chmod 600 "$KEY"
        chmod 644 "$KEY.pub"
        if [ "$(id -u)" -eq 0 ]; then
          chown "$OWNER_UID:$OWNER_GID" "$KEY" "$KEY.pub"
        fi
      fi
    '';
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
      hashedPassword = config.neo.core.hashedLinuxPassword;
    };
    time.timeZone = config.neo.core.timeZone;

    system.activationScripts.homeserver-ssh-key = ''
      ${homeserverSshKey}/bin/neo-homeserver-ssh-key ensure
    '';

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
