# Every built-in service enabled at once, with the few settings they cannot
# start without (domain, relay, Pi-hole address, VPN address, a tunnel entry).
# Not a module (import-tree skips `_` files): the checks that need a whole
# configuration import it.
#   import ./_all-enabled.nix {inherit inputs lib; neoFlake = config.flake; modules = [ … ];}
{
  inputs,
  lib,
  neoFlake,
  modules ? [],
}: let
  names = lib.attrNames (lib.filterAttrs (_: t: t == "directory") (builtins.readDir ../services));
in {
  inherit names;
  eval = inputs.nixpkgs.lib.nixosSystem {
    system = "x86_64-linux";
    specialArgs.lib = neoFlake.lib;
    modules =
      [
        neoFlake.nixosModules.default
        {
          boot.isContainer = true;
          fileSystems."/" = {
            device = "x";
            fsType = "ext4";
          };
          system.stateVersion = "24.11";
          neo.services =
            lib.genAttrs names (_: {enabled = true;})
            // {
              swag = {
                enabled = true;
                domain = "example.com";
                email = "admin@example.com";
                crowdsec.enabled = true;
              };
              rathole = {
                enabled = true;
                remoteAddr = "relay.example.net:2333";
                name = "home";
              };
              pihole = {
                enabled = true;
                localIP = "192.168.1.10";
              };
              vpn = {
                enabled = true;
                wireguardAddresses = "10.0.0.2/32";
              };
              searxng = {
                enabled = true;
                vpn.enabled = true;
              };
              streamproxy = {
                enabled = true;
                entries.example.url = "home.example.org";
              };
            };
        }
      ]
      ++ modules;
  };
}
