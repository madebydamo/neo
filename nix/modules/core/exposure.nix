# Everything that is reachable from outside this server, for the Access page
# (docs/design/access-and-defaults.md): apps behind SWAG with where they open
# from and how people log in, every open port with what protects it, and the
# logins of the server itself (SSH keys, console password, login page users).
#
# Ports are discovered from the evaluated system (NixOS firewall, per-interface
# rules and published Docker ports not bound to loopback) and described by the
# module that opens them in neo.exposure.ports. The flake check
# exposure-complete fails on an open port that no module describes, so the page
# never hides one.
{...}: {
  flake.modules.nixos.exposure = {
    config,
    lib,
    options,
    ...
  }: let
    inherit (lib) mkOption types;
    neo = config.neo;
    fw = config.networking.firewall;
    services = neo.services;
    tinyauth = services.tinyauth or {};
    domain = services.swag.domain or null;

    # --- Discovered ports -------------------------------------------------
    expand = proto: iface: ports: ranges:
      map (port: {inherit proto port iface;}) ports
      ++ lib.concatMap (r:
        map (port: {inherit proto port iface;}) (lib.range r.from r.to))
      ranges;
    firewallPorts =
      expand "tcp" null fw.allowedTCPPorts fw.allowedTCPPortRanges
      ++ expand "udp" null fw.allowedUDPPorts fw.allowedUDPPortRanges
      ++ lib.concatLists (lib.mapAttrsToList (
          iface: v:
            lib.optionals (!(lib.elem iface fw.trustedInterfaces))
            (expand "tcp" iface v.allowedTCPPorts v.allowedTCPPortRanges
              ++ expand "udp" iface v.allowedUDPPorts v.allowedUDPPortRanges)
        )
        fw.interfaces);

    # "[ip:]host:container[/proto]" → { proto; port; container; address; }.
    parsePublished = container: spec: let
      protoSplit = lib.splitString "/" spec;
      proto =
        if lib.length protoSplit > 1
        then lib.last protoSplit
        else "tcp";
      parts = lib.splitString ":" (lib.head protoSplit);
      n = lib.length parts;
      hostPort = lib.elemAt parts (n - 2);
      address =
        if n > 2
        then lib.concatStringsSep ":" (lib.take (n - 2) parts)
        else "";
    in
      if n < 2 || builtins.match "[0-9]+" hostPort == null
      then []
      else [
        {
          inherit proto container address;
          port = lib.toInt hostPort;
          iface = null;
        }
      ];
    loopback = a: lib.hasPrefix "127." a || a == "::1" || a == "[::1]" || a == "localhost";
    dockerPorts = lib.filter (p: !(loopback p.address)) (lib.concatLists (lib.mapAttrsToList (
        name: c: lib.concatMap (parsePublished name) (c.ports or [])
      )
      config.virtualisation.oci-containers.containers));

    # Same port twice (firewall and Docker) shows once.
    discovered = let
      key = p: "${p.proto}/${toString p.port}/${toString p.iface}";
      byKey = lib.foldl' (acc: p:
        if acc ? ${key p}
        then acc // {${key p} = acc.${key p} // lib.optionalAttrs (p ? container) {inherit (p) container;};}
        else acc // {${key p} = p;}) {} (firewallPorts ++ dockerPorts);
    in
      lib.attrValues byKey;

    declared = lib.attrValues (lib.mapAttrs (id: d: d // {inherit id;}) neo.exposure.ports);
    describe = p:
      lib.findFirst (d: d.protocol == p.proto && d.port == p.port && (d.interface == null || d.interface == p.iface)) null declared;
    portRows =
      map (p: let
        d = describe p;
      in {
        protocol = p.proto;
        inherit (p) port;
        interface = p.iface;
        container = p.container or null;
        described = d != null;
        id =
          if d == null
          then null
          else d.id;
        label =
          if d == null
          then "Unknown"
          else d.label;
        service =
          if d == null
          then null
          else d.service;
        login =
          if d == null
          then ""
          else d.login;
        open =
          if d == null
          then false
          else d.open;
      })
      discovered;

    # --- Apps -------------------------------------------------------------
    isOption = o: builtins.isAttrs o && (o._type or null) == "option";
    subOptions = name: let
      o = options.neo.services.${name} or null;
    in
      if isOption o && (o.type.name or "") == "submodule"
      then o.type.getSubOptions []
      else {};
    inheritsFrom = o: (o.inherits or {}).path or null;

    # tinyauth per-app ACLs: who may open `app` (users, then provider groups).
    userNames = map (u: lib.head (lib.splitString ":" u)) (tinyauth.users or []);
    grants = rule: app:
      if rule == null
      then true
      else if (rule.allow or []) != []
      then lib.elem app rule.allow
      else !(lib.elem app (rule.block or []));
    whoMayOpen = app: {
      users = lib.filter (u: grants ((tinyauth.access or {}).${u} or null) app) userNames;
      groups = lib.filter (g: grants ((tinyauth.groupAccess or {}).${g} or null) app) (tinyauth.oauthGroups or []);
    };

    proxied = lib.neo.getProxiedServices config;
    appRow = name: svc: let
      subs = subOptions name;
      authSubs =
        if subs ? auth
        then subs.auth.type.getSubOptions []
        else {};
      meta = svc.meta or {};
      hasSso = subs ? sso && (subs.sso.type.name or "") == "bool";
      authAvailable = !(subs.auth.internal or false);
    in {
      inherit name;
      title = meta.title or name;
      icon = meta.icon or null;
      url = lib.neo.publicUrl config svc;
      hosts =
        lib.optional (lib.neo.publicHost config svc != null) (lib.neo.publicHost config svc)
        ++ (svc.customDomains or []);
      ingress = svc.ingress or lib.neo.ingressVariants;
      # null = the module fixes the value (no Default access link).
      ingressFrom = inheritsFrom (subs.ingress or {});
      auth = {
        available = authAvailable;
        enabled = authAvailable && (svc.auth.enabled or false);
        from = inheritsFrom (authSubs.enabled or {});
        publicPaths = svc.auth.publicPaths or [];
      };
      sso =
        if hasSso
        then {
          enabled = svc.sso;
          from = inheritsFrom subs.sso;
        }
        else null;
      ownLogin = meta.ownLogin or null;
      who = whoMayOpen name;
    };
    apps = lib.mapAttrsToList appRow (removeAttrs proxied ["swag"]);

    keys = neo.core.ssh.authorizedKeys;
  in {
    options.neo.exposure = {
      ports = mkOption {
        type = types.attrsOf (types.submodule {
          options = {
            port = mkOption {
              type = types.port;
              description = "Port number on the server.";
            };
            protocol = mkOption {
              type = types.enum ["tcp" "udp"];
              default = "tcp";
              description = "tcp or udp.";
            };
            interface = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Network interface the port is open on; null for every interface.";
            };
            label = mkOption {
              type = types.str;
              description = "What the port is for, in plain words (\"SSH\", \"DNS for your home\").";
            };
            service = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Service that opens it (a neo.services name), for the link on the Access page.";
            };
            login = mkOption {
              type = types.str;
              description = "What protects it, in plain words (\"SSH keys\", \"Each app's login\", \"None: answers anyone\").";
            };
            open = mkOption {
              type = types.bool;
              default = false;
              description = "Anyone who reaches the port may use it, on purpose (DNS, time, connection helpers).";
            };
          };
        });
        default = {};
        description = "Ports this module opens to the outside, by id, with what protects them. Every port in the firewall or published by a container needs one (flake check exposure-complete).";
      };

      report = mkOption {
        type = types.attrs;
        readOnly = true;
        internal = true;
        description = "What the Access page shows: apps, ports and logins.";
        default = {
          inherit apps;
          defaults = neo.access;
          ports = portRows;
          places = {
            local = true;
            tailscale = services.tailscale.enabled or false;
            netbird = services.netbird.enabled or false;
            # Public HTTPS arrives through the relay tunnel, or directly when the
            # router forwards 80/443 (not known here).
            relay = services.rathole.enabled or false;
          };
          ssh = {
            ports = config.services.openssh.ports;
            users = ["admin" "homeserver"];
            inherit keys;
            passwordLogin = keys == [];
            consolePassword = neo.core.hashedLinuxPassword != "";
            netbirdSsh = (services.netbird.enabled or false) && (services.netbird.ssh or false);
          };
          loginPage = {
            enabled = tinyauth.enabled or false;
            url = lib.neo.publicUrl config tinyauth;
            users = userNames;
            providers = lib.attrNames (lib.filterAttrs (_: p: p.enabled or true) (tinyauth.oauth or {}));
            groups = tinyauth.oauthGroups or [];
            oidcApps = lib.attrNames (tinyauth.oidcClients or {});
          };
          inherit domain;
          undescribed = map (p: "${p.protocol}/${toString p.port}") (lib.filter (p: !p.described) portRows);
        };
      };
    };

    config.neo.exposure.ports = {
      ssh = lib.mkIf config.services.openssh.enable {
        port = lib.head (config.services.openssh.ports ++ [22]);
        label = "SSH (server login)";
        login =
          if keys == []
          then "Password (no SSH key set yet)"
          else "SSH keys";
      };
    };
  };
}
