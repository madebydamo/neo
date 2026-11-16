# VPN-routed containers keep their Docker connections (docs/design/network-isolation.md → VPN):
# a routed container has no networks of its own (container:vpn), so gluetun
# stands in for it on its service network, on `internal` for a plugin that
# still uses it, on every link it is a member of, and on neo-host when it has
# host access, each time with only the routed names that belong there. App
# metrics of a routed container are scraped through gluetun, and app alert
# rules render into one group per app. Also checks the same config with the
# VPN off: the link member joins the link itself.
{
  config,
  inputs,
  lib,
  ...
}: let
  neoLib = config.flake.lib;
  inherit (neoLib.neo) docker;

  evalWith = vpnOn:
    inputs.nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      specialArgs.lib = neoLib;
      modules = [
        config.flake.nixosModules.default
        ({lib, ...}: {
          boot.isContainer = true;
          fileSystems."/" = {
            device = "x";
            fsType = "ext4";
          };
          system.stateVersion = "24.11";
          neo.services = {
            swag = {
              enabled = true;
              domain = "example.com";
              email = "admin@example.com";
            };
            vpn = {
              enabled = vpnOn;
              wireguardPrivateKey = "x";
              wireguardPresharedKey = "x";
              wireguardAddresses = "10.0.0.2/32";
            };
            searxng = {
              enabled = true;
              vpn.enabled = true;
            };
            changedetection = {
              enabled = true;
              vpn.enabled = true;
            };
            pastebin.enabled = true;
          };
        })
        # A plugin-like service on the legacy shared network, routed, with a
        # link to a built-in container, host access and a metrics endpoint.
        ({lib, ...}: {
          options.neo.services.oldplug = lib.mkOption {
            type = lib.types.submodule {
              options =
                {
                  enabled = lib.mkEnableOption "test plugin";
                }
                // lib.neo.mkVpnOptions {
                  containers = ["oldplug"];
                  networks = [docker.sharedNetwork];
                  ports = [9999];
                  enabled = true;
                }
                // lib.neo.mkContainerDefinitions {oldplug = "busybox";};
            };
            default = {};
          };
          config = lib.mkMerge [
            {
              neo.services.oldplug.enabled = true;
              virtualisation.oci-containers.containers.oldplug = {
                image = "busybox";
                networks = [docker.sharedNetwork];
              };
              neo.monitoring.apps.oldplug = {
                metrics.main = {
                  container = "oldplug";
                  port = 9100;
                };
                alerts.stale = {
                  title = "Feed stale";
                  expr = ''time() - oldplug_last_poll_timestamp_seconds{service="oldplug"}'';
                  above = 300;
                  summary = "No data for {{ $values.B.Value }} s.";
                };
              };
            }
            (lib.neo.docker.mkLink {
              name = "oldplug-paste";
              containers = ["oldplug" "pastebin"];
            })
            (lib.neo.docker.mkHostAccess "oldplug")
          ];
        })
      ];
    };

  on = (evalWith true).config;
  off = (evalWith false).config;
  cOn = on.virtualisation.oci-containers.containers;
  cOff = off.virtualisation.oci-containers.containers;
  gluetun = cOn.vpn.networks or [];
  has = entry: lib.elem entry gluetun;
  failedAssertions = cfg: map (a: a.message) (lib.filter (a: !a.assertion) cfg.assertions);
  target = lib.findFirst (t: t.service == "oldplug") {} on.neo.monitoring.scrapeTargets;
  group = on.neo.monitoring.alertGroups.oldplug or {};

  problems =
    lib.optional (cOn.oldplug.networks != ["container:vpn"]) "routed oldplug should only be in container:vpn, is ${toString cOn.oldplug.networks}"
    ++ lib.optional (lib.elem docker.hostAlias (cOn.oldplug.extraOptions or [])) "routed oldplug must not get --add-host (Docker refuses it with container:vpn)"
    ++ lib.optional (!has "name=internal,alias=oldplug") "gluetun is not on internal as oldplug: ${toString gluetun}"
    ++ lib.optional (!has "name=app-oldplug,alias=oldplug") "gluetun is not on app-oldplug as oldplug: ${toString gluetun}"
    ++ lib.optional (!has "name=link-oldplug-paste,alias=oldplug") "gluetun does not stand in for oldplug on its link: ${toString gluetun}"
    ++ lib.optional (!has "name=app-searxng,alias=searxng") "gluetun is not on app-searxng as searxng: ${toString gluetun}"
    ++ lib.optional (lib.any (lib.hasInfix "alias=searxng") (lib.filter (lib.hasInfix "link-") gluetun)) "searxng resolves on a link it is not part of"
    ++ lib.optional (!has docker.hostNetwork) "gluetun is not on ${docker.hostNetwork} for the routed host access"
    ++ lib.optional (!(lib.elem docker.hostAlias (cOn.vpn.extraOptions or []))) "gluetun lacks host.docker.internal"
    ++ lib.optional (!(lib.elem (docker.linkNetwork "oldplug-paste") (docker.containerNetworks cOn.pastebin))) "pastebin is not on its link"
    ++ map (m: "VPN on: ${m}") (failedAssertions on)
    ++ lib.optional ((target.container or null) != "vpn" || (target.network or null) != "app-oldplug") "routed metrics should be scraped from vpn on app-oldplug, got ${target.container or "-"} on ${target.network or "-"}"
    # VPN off: everything joins its networks itself.
    ++ lib.optional (cOff ? vpn) "VPN off: no gluetun container expected"
    ++ lib.optional (!(lib.elem (docker.linkNetwork "oldplug-paste") (docker.containerNetworks cOff.oldplug))) "VPN off: oldplug is not on its link"
    ++ lib.optional (!(lib.elem docker.hostNetwork (docker.containerNetworks cOff.oldplug))) "VPN off: oldplug is not on ${docker.hostNetwork}"
    ++ lib.optional (!(lib.elem docker.hostAlias (cOff.oldplug.extraOptions or []))) "VPN off: oldplug lacks host.docker.internal"
    ++ map (m: "VPN off: ${m}") (failedAssertions off)
    # App alert rules.
    ++ lib.optional ((group.folderUid or null) != "neo-apps" || lib.length (group.rules or []) != 1) "alert group oldplug not rendered: ${builtins.toJSON group}"
    ++ lib.optional (((lib.head (group.rules or [{}])).labels.service or null) != "oldplug") "alert rule lacks the service label";
in {
  perSystem = {pkgs, ...}: {
    checks.vpn-networks = pkgs.runCommand "vpn-networks" {} ''
      ${lib.concatMapStrings (p: "echo ${lib.escapeShellArg "FAIL ${p}"} >&2\n") problems}
      ${lib.optionalString (problems != []) "exit 1"}
      echo ok > $out
    '';
  };
}
