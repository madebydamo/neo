# Every built-in service enabled at once (several routed through the VPN, the
# optional sidecars on): no container of one service shares a Docker network
# with a container of another, apart from SWAG, gluetun and declared links.
# No built-in container is left on the shared `internal` network, none lands
# on Docker's default bridge, and none names a network twice (Docker refuses
# that). Design: docs/design/network-isolation.md.
{
  config,
  inputs,
  lib,
  ...
}: let
  neoLib = config.flake.lib;
  inherit (neoLib.neo) docker;
  names = lib.attrNames (lib.filterAttrs (_: t: t == "directory") (builtins.readDir ../services));

  eval = inputs.nixpkgs.lib.nixosSystem {
    system = "x86_64-linux";
    specialArgs.lib = neoLib;
    modules = [
      config.flake.nixosModules.default
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
            searxng = {
              enabled = true;
              vpn.enabled = true;
            };
            changedetection = {
              enabled = true;
              vpn.enabled = true;
            };
          };
      }
    ];
  };

  cfg = eval.config;
  containers = cfg.virtualisation.oci-containers.containers;
  owner = docker.containerService cfg;
  links = cfg.neo.core.docker.links;
  shared = [docker.sharedNetwork neoLib.neo.swagIpv6Network];
  # Containers that may sit on several services' networks.
  hubs = ["swag" "vpn"];

  bridgeEntries = c: lib.filter (n: docker.isCreatedNetwork (docker.networkName n)) (map docker.networkName (c.networks or []));
  usesHost = c: lib.any (n: n == "host" || lib.hasPrefix "container:" n) (c.networks or []);

  members = net: lib.attrNames (lib.filterAttrs (_: c: lib.elem net (docker.containerNetworks c)) containers);
  networks = docker.usedNetworks containers;

  problems =
    # Every container belongs to a service.
    map (c: "container ${c} belongs to no enabled service (containers / systemdUnits)")
    (lib.filter (c: !(owner ? ${c})) (lib.attrNames containers))
    # Built-in containers do not use the shared network.
    ++ map (c: "container ${c} is on ${docker.sharedNetwork}; use lib.neo.docker.serviceNetwork")
    (lib.filter (c: c != "swag" && lib.elem docker.sharedNetwork (docker.containerNetworks containers.${c})) (lib.attrNames containers))
    # Docker's default bridge connects everything on it.
    ++ map (c: "container ${c} has no network (Docker's default bridge)")
    (lib.filter (c: !(usesHost containers.${c}) && docker.containerNetworks containers.${c} == []) (lib.attrNames containers))
    # Docker refuses a network listed twice.
    ++ lib.concatMap (c: let
      nets = docker.containerNetworks containers.${c};
    in
      lib.optional (lib.length nets != lib.length (lib.unique nets)) "container ${c} names a network twice: ${lib.concatStringsSep " " nets}")
    (lib.attrNames containers)
    # One service per network, plus SWAG and gluetun; a link holds exactly its containers.
    ++ lib.concatMap (net: let
      m = members net;
      linkName = lib.removePrefix "link-" net;
    in
      if lib.elem net shared
      then []
      else if lib.hasPrefix "link-" net
      then
        lib.optional (!(links ? ${linkName}) || lib.sort lib.lessThan m != lib.sort lib.lessThan links.${linkName})
        "network ${net}: members ${lib.concatStringsSep " " m} are not the declared link"
      else let
        services = lib.unique (map (c: owner.${c} or "?") (lib.filter (c: !(lib.elem c hubs)) m));
      in
        lib.optional (lib.length services > 1)
        "network ${net} mixes services: ${lib.concatStringsSep ", " (map (c: "${c} (${owner.${c} or "?"})") m)}")
    networks
    # SWAG reaches every proxied service that runs containers.
    ++ lib.concatMap (s: let
      net = docker.serviceNetwork s;
    in
      lib.optional (lib.elem net networks && !(lib.elem net (docker.containerNetworks containers.swag)))
      "SWAG is not on ${net}, the network of proxied service ${s}")
    (lib.attrNames (neoLib.neo.getProxiedServices cfg));

  # Each VPN-routed container answers by name on its service network only.
  gluetun = lib.concatStringsSep " " (containers.vpn.networks or []);
  aliasProblems =
    lib.optional (!(lib.hasInfix "name=app-searxng,alias=searxng" gluetun))
    "gluetun has no searxng alias on app-searxng: ${gluetun}"
    ++ lib.optional (lib.hasInfix "--network-alias" (lib.concatStringsSep " " (containers.vpn.extraOptions or [])))
    "gluetun uses --network-alias, which Docker applies to the first network only";

  all = problems ++ aliasProblems;
in {
  perSystem = {pkgs, ...}: {
    checks.docker-network-isolation = pkgs.runCommand "docker-network-isolation" {} ''
      ${lib.concatMapStrings (p: "echo ${lib.escapeShellArg "FAIL ${p}"} >&2\n") all}
      ${lib.optionalString (all != []) "exit 1"}
      echo "${toString (lib.length networks)} networks, ${toString (lib.length (lib.attrNames containers))} containers isolated" > $out
    '';
  };
}
