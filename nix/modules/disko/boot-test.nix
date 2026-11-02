# Boot test of the disko layout on real firmware: `just test-disko-boot`.
#
# The dev VM (`just launch`) boots its kernel directly and never runs GRUB, so
# it cannot catch boot loader problems. This builds disk images the way an
# install does (disko + nixos-install, inside a VM with OVMF) and boots them
# from the disk under UEFI (OVMF) and BIOS (SeaBIOS) firmware:
#
#   uefi       hybrid layout, fresh firmware variables: boots, and the NixOS
#              firmware boot entry exists, first in the boot order
#   uefi-again same variables: the firmware boots through that entry
#   bios       hybrid layout on BIOS firmware
#   legacy     layout without the BIOS boot partition (disks partitioned
#              before it existed): switching still works, UEFI boot
#
# In each boot the system re-runs its boot loader install (as a switch does)
# and reports to the serial console; the runner checks the report. Not part of
# `nix flake check`: it needs KVM and takes a few minutes.
{
  config,
  inputs,
  ...
}: let
  flakeConfig = config;
in {
  perSystem = {
    pkgs,
    lib,
    system,
    ...
  }: let
    mkSystem = biosBoot:
      inputs.nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs.lib = flakeConfig.flake.lib;
        modules = [
          flakeConfig.flake.nixosModules.default
          flakeConfig.flake.nixosModules.base
          ({modulesPath, ...}: {
            imports = [(modulesPath + "/profiles/qemu-guest.nix")];
            neo.core.hostname = "neo-boot-test";
            neo.disko = {
              enabled = true;
              mainDisk = "/dev/vda";
              inherit biosBoot;
            };
            disko.imageBuilder.imageFormat = "qcow2";
            disko.devices.disk.main.imageSize = "12G";
            disko.memSize = 2048;
            boot.kernelParams = ["console=ttyS0,115200"];
            boot.loader.timeout = lib.mkForce 1;
            documentation.enable = false;

            systemd.services.neo-boot-test = {
              description = "Neo boot test report";
              wantedBy = ["multi-user.target"];
              after = ["multi-user.target"];
              path = with pkgs; [efibootmgr gnugrep gnused coreutils systemd];
              serviceConfig = {
                Type = "oneshot";
                StandardOutput = "tty";
                StandardError = "tty";
                TTYPath = "/dev/ttyS0";
              };
              script = ''
                set -u
                ok=1
                fail() { echo "NEO-BOOT-TEST check failed: $*"; ok=0; }
                if [ -d /sys/firmware/efi ]; then fw=uefi; else fw=bios; fi
                echo "NEO-BOOT-TEST firmware=$fw"
                current=""
                if [ "$fw" = uefi ]; then
                  efibootmgr -v || true
                  current=$(efibootmgr | sed -n 's/^BootCurrent: //p')
                fi
                # What a switch does: rerun the boot loader install (+ boot-install.sh).
                if ! /run/current-system/bin/switch-to-configuration boot; then
                  fail "switch-to-configuration boot"
                fi
                if [ "$fw" = uefi ]; then
                  efibootmgr -v || true
                  n=$(efibootmgr | grep -cE '^Boot[0-9A-F]{4}\*? NixOS([[:space:]]|$)' || true)
                  [ "$n" = 1 ] || fail "expected one NixOS boot entry, found $n"
                  ours=$(efibootmgr | grep -E '^Boot[0-9A-F]{4}\*? NixOS([[:space:]]|$)' | head -1 | cut -c5-8)
                  first=$(efibootmgr | sed -n 's/^BootOrder: //p' | cut -d, -f1)
                  [ -n "$ours" ] && [ "$first" = "$ours" ] || fail "NixOS entry ($ours) is not first in the boot order ($first)"
                  if [ "$current" = "$ours" ]; then
                    echo "NEO-BOOT-TEST booted-via-entry=yes"
                  else
                    echo "NEO-BOOT-TEST booted-via-entry=no ($current)"
                  fi
                  [ -f /boot/startup.nsh ] || fail "no startup.nsh on the ESP"
                fi
                if [ "$ok" = 1 ]; then echo "NEO-BOOT-TEST result=ok"; else echo "NEO-BOOT-TEST result=fail"; fi
                sync
                systemctl --no-block poweroff
              '';
            };
          })
        ];
      };
    image = biosBoot: (mkSystem biosBoot).config.system.build.diskoImages;
  in {
    packages.disko-boot-test = pkgs.writeShellApplication {
      name = "disko-boot-test";
      runtimeInputs = with pkgs; [qemu coreutils gnugrep];
      text = ''
        # Host libraries built against another glibc break QEMU (as in just launch).
        unset LD_LIBRARY_PATH
        hybrid=$(echo ${image true}/*.qcow2)
        legacy=$(echo ${image false}/*.qcow2)
        work=$(mktemp -d "''${TMPDIR:-/tmp}/disko-boot-test.XXXXXX")
        trap 'rm -rf "$work"' EXIT
        failed=0

        # boot NAME BASE_IMAGE FIRMWARE(uefi|bios) VARS EXPECT...
        # Boots a fresh overlay $work/NAME.qcow2 on BASE_IMAGE.
        # The serial gives the disk a /dev/disk/by-id name, where the initrd
        # looks for ZFS pools (as on real hardware).
        boot() {
          local name=$1 base=$2 fw=$3 vars=$4
          shift 4
          local disk=$work/$name.qcow2 log=$work/$name.log
          qemu-img create -q -f qcow2 -b "$base" -F qcow2 "$disk"
          local fwopts=()
          if [ "$fw" = uefi ]; then
            fwopts=(-drive "if=pflash,format=raw,unit=0,readonly=on,file=${pkgs.OVMF.firmware}"
              -drive "if=pflash,format=raw,unit=1,file=$vars")
          fi
          echo "== $name: booting ($fw)"
          timeout 900 qemu-system-x86_64 -enable-kvm -cpu host -m 3072 -smp 2 \
            -display none -monitor none -serial "file:$log" -nic none \
            "''${fwopts[@]}" -drive "file=$disk,if=virtio,format=qcow2,serial=neo-boot-test" || true
          grep -a 'NEO-BOOT-TEST\|neo-boot:' "$log" || true
          local want
          for want in "NEO-BOOT-TEST result=ok" "$@"; do
            if ! grep -aq "$want" "$log"; then
              echo "== $name: FAILED (missing \"$want\"); serial log tail:"
              tail -40 "$log"
              failed=1
              return
            fi
          done
          echo "== $name: ok"
        }

        install -m600 ${pkgs.OVMF.variables} "$work/vars-hybrid.fd"
        install -m600 ${pkgs.OVMF.variables} "$work/vars-legacy.fd"
        boot uefi "$hybrid" uefi "$work/vars-hybrid.fd" "firmware=uefi"
        # Same disk and firmware variables as the first boot.
        boot uefi-again "$work/uefi.qcow2" uefi "$work/vars-hybrid.fd" "booted-via-entry=yes"
        boot bios "$hybrid" bios - "firmware=bios"
        boot legacy "$legacy" uefi "$work/vars-legacy.fd" "firmware=uefi" "no BIOS boot partition"
        if [ "$failed" = 0 ]; then echo "all boots ok"; else echo "some boots FAILED"; exit 1; fi
      '';
    };
  };
}
