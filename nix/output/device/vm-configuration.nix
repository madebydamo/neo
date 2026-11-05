# VM-specific hardware and virtualisation configuration.
# Stored under devices (not flake.modules.nixos) so it is NOT auto-included.
# Only configurations that explicitly import this module (e.g. QEMU via
# configurations.nixos.homeserver or nixosModules.vm) get these settings.
{
  config,
  self,
  ...
}: {
  devices.vm = {
    config,
    lib,
    pkgs,
    ...
  }: let
    # Same virtual size as the ext4 dev disk. The ZFS pool is created at this
    # size the first time the disko base image is formatted; a later change
    # does not resize an existing nixos-disko overlay.
    diskSizeMB = 1024000;
    # Directory of a local plugin flake (git+file:, path:, or an absolute path).
    # Must match the tag scheme in the justfile, which passes the virtfs devices.
    localDir = url: let
      base =
        if lib.hasPrefix "git+file://" url
        then lib.removePrefix "git+file://" url
        else if lib.hasPrefix "git+file:" url
        then lib.removePrefix "git+file:" url
        else if lib.hasPrefix "path:" url
        then lib.removePrefix "path:" url
        else if lib.hasPrefix "/" url
        then url
        else "";
      # Neither the flake output (#…) nor the query (?ref=…) is part of the directory.
      dir = lib.head (builtins.split "[?#]" base);
    in
      if dir == ""
      then null
      else dir;
    pluginDirs = lib.unique (lib.sort builtins.lessThan (
      lib.filter (d: d != null) (map (p: localDir p.url) (lib.attrValues (config.neo.core.plugins or {})))
    ));
    # A checkout inside another share is already visible there.
    shareRoots =
      lib.filter (
        dir:
          lib.all (
            other: other == dir || !lib.hasPrefix "${other}/" dir
          )
          pluginDirs
      )
      pluginDirs;
    shareTag = dir: "h" + builtins.substring 0 8 (builtins.hashString "sha256" dir);
    # Guest nix builds land on the tmpfs half of the 9p store overlay and
    # vanish on reboot, while /nix/var/nix/db stays on the persistent disk.
    # Nix then tries to open those registered paths and refuses to rebuild them.
    repairMissingStorePaths = pkgs.writeShellScript "repair-missing-store-paths" ''
      set -eu
      ${pkgs.systemd}/bin/systemctl stop nix-daemon.socket nix-daemon.service || true
      ${pkgs.python3}/bin/python3 - << 'PY'
      import os
      import sqlite3
      import sys

      db = "/nix/var/nix/db/db.sqlite"
      if not os.path.ismount("/nix/store"):
          sys.exit("nix-db: /nix/store is not mounted; refusing")
      con = sqlite3.connect(db)
      con.execute("pragma foreign_keys = off")
      missing = [
          row[0]
          for row in con.execute("select id, path from ValidPaths")
          if not os.path.lexists(row[1])
      ]
      if not missing:
          print("nix-db: no missing store paths")
          sys.exit(0)
      # A reboot drops guest builds that lived on the tmpfs store. If the
      # store itself is missing, nearly every row looks gone; do not wipe it.
      if len(missing) > 2000:
          sys.exit(f"nix-db: {len(missing)} missing paths is more than a reboot should drop; refusing")
      print(f"nix-db: dropping {len(missing)} missing store paths")

      def chunks(items, size=400):
          for i in range(0, len(items), size):
              yield items[i : i + size]

      for chunk in chunks(missing):
          marks = ",".join("?" * len(chunk))
          con.execute(
              f"delete from Refs where referrer in ({marks}) or reference in ({marks})",
              chunk + chunk,
          )
          con.execute(f"delete from DerivationOutputs where drv in ({marks})", chunk)
          con.execute(f"delete from ValidPaths where id in ({marks})", chunk)
      con.commit()
      PY
      ${pkgs.systemd}/bin/systemctl start nix-daemon.socket nix-daemon.service
    '';
  in {
    # Development SSH key for `just ssh` / tools/development_ed25519.
    # Merged with any keys from settings.toml (listOf concatenates definitions).
    # Not a default on neo.core.ssh.authorizedKeys so real deployments stay clean.
    neo.core.ssh.authorizedKeys = [
      (lib.fileContents (self + "/tools/development_ed25519.pub"))
    ];

    # `just ssh` is root@ with that key. Real installs do not import this module
    # and keep PermitRootLogin = no. prohibit-password is public-key only.
    services.openssh.settings.PermitRootLogin = lib.mkForce "prohibit-password";

    users.allowNoPasswordLogin = true;
    users.mutableUsers = false;
    # boot.loader.grub.device = "/dev/vda";
    fileSystems."/".device = lib.mkDefault "/dev/vda1";
    fileSystems."/".fsType = lib.mkDefault "ext4";
    # Host checkouts for git+file plugins. wantedBy (not required) so a VM
    # started without the virtfs still reaches multi-user. cache=none so nix
    # sees new commits. The justfile passes the matching virtfs devices.
    systemd.mounts =
      map (dir: {
        what = shareTag dir;
        where = dir;
        type = "9p";
        options = "trans=virtio,version=9p2000.L,msize=104857600,cache=none";
        wantedBy = ["local-fs.target"];
        before = ["neo-bootstrap.service"];
        mountConfig.DirectoryMode = "0755";
      })
      shareRoots;

    # Before neo-bootstrap evaluates git+file plugins and before it rebuilds
    # write-flake into the tmpfs store.
    systemd.services.nix-store-repair-missing = {
      description = "Drop Nix DB entries whose store files did not survive reboot";
      wantedBy = ["multi-user.target"];
      before = ["neo-bootstrap.service" "multi-user.target"];
      after = ["local-fs.target"];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = repairMissingStorePaths;
      };
    };
    systemd.services.neo-bootstrap = {
      wants = ["nix-store-repair-missing.service"];
      after = ["nix-store-repair-missing.service"];
    };
    virtualisation = {
      diskSize = diskSizeMB;
      docker.enable = true;
      oci-containers.backend = "docker";
    };

    # Disko's vmWithDisko runner (lib/interactive-vm.nix) is a test helper:
    # it formats a qcow2 overlay under mktemp and deletes it when QEMU exits,
    # so /, docker, and /var/neo are empty on every boot. The guest system
    # still comes from the host Nix store (direct kernel boot + 9p), same as
    # run-nixos-vm, so keeping that overlay is what makes `just launch` persist.
    # The ext4 image stays nixos.qcow2. This chain is nixos-disko/.
    # disko.testMode is set only inside the variant; the guard stops the
    # variant from extending itself.
    virtualisation.vmVariantWithDisko = lib.mkIf (config.neo.disko.enabled && !config.disko.testMode) ({
      config,
      lib,
      pkgs,
      ...
    }: {
      disko.devices.disk.main.imageSize = "${toString diskSizeMB}M";
      # The image builder formats the pool inside its own QEMU. 1G OOMs on a 1TB vdev.
      disko.memSize = 2048;
      system.build.vmWithDisko = lib.mkForce (
        pkgs.writers.writeDashBin "disko-vm" ''
          set -eu
          export PATH=${lib.makeBinPath [pkgs.coreutils pkgs.qemu]}

          # Absolute: run-*-vm cds away before exec, and its drive path is "$tmp"/<name>.qcow2.
          disk_dir=''${DISKO_VM_STATE_DIR:-$(pwd)/nixos-disko}
          mkdir -p "$disk_dir/base"
          tmp=$(cd "$disk_dir" && pwd)
          export tmp
          # Do not share the ext4 VM's nixos-efi-vars.fd.
          export NIX_EFI_VARS="$tmp/efi-vars.fd"

          images=${lib.escapeShellArg config.system.build.diskoImages}
          vm_bin=${lib.escapeShellArg (config.system.build.vm + "/bin")}
          found=0
          for img in "$images"/*.qcow2; do
            [ -f "$img" ] || continue
            found=1
            name=$(basename "$img")
            overlay="$tmp/$name"
            base="$tmp/base/$name"
            if [ -f "$overlay" ]; then
              if [ ! -f "$base" ]; then
                echo "disko-vm: $overlay exists but backing file $base is missing." >&2
                echo "disko-vm: refusing to format a new disk over an existing overlay." >&2
                exit 1
              fi
              echo "disko-vm: reusing $overlay"
              continue
            fi
            echo "disko-vm: creating $overlay (nixos.qcow2 is not used)"
            rm -f "$base.new" "$overlay.new"
            cp --reflink=auto --sparse=always "$img" "$base.new"
            mv "$base.new" "$base"
            # Backing path is relative to the overlay, so the directory can be moved.
            qemu-img create -f qcow2 -b "base/$name" -F qcow2 "$overlay.new"
            mv "$overlay.new" "$overlay"
          done
          if [ "$found" -eq 0 ]; then
            echo "disko-vm: no qcow2 images in $images" >&2
            exit 1
          fi
          exec "$vm_bin"/run-*-vm "$@"
        ''
      );
    });
  };
  flake.nixosModules.vm = config.devices.vm;
}
