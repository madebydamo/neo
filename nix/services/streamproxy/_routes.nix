# Routing tables of the streamproxy nginx (not a module: imported by default.nix
# and check.nix; import-tree skips `_` files).
#
# nginx picks a name in the stream map (`hostnames;`) and in `server_name` in
# this order: exact name, longest `*.` wildcard, then regexes. So every name is
# exact or a `*.` wildcard, never a regex: a tunnel `*.dev1.example.com` then wins
# over the local SWAG `*.example.com`, as it is longer. A name that both a tunnel
# and the local SWAG claim goes to the tunnel (the local one is left out), and a
# name written twice is kept once (first tunnel wins), so nginx never warns
# "conflicting parameter" and picks one by file order.
{lib}: let
  inherit (lib) concatLists concatStringsSep elem foldl' optional sort;

  # [{name; backend}] → the same without later duplicates of a name.
  dedupe = foldl' (acc: r:
    if elem r.name acc.names
    then acc
    else {
      names = acc.names ++ [r.name];
      routes = acc.routes ++ [r];
    }) {
    names = [];
    routes = [];
  };
in {
  # Names an entry sends through its tunnel.
  entryDomains = entry:
    (optional entry.includeTopLevel entry.url)
    ++ (optional entry.wildcard "*.${entry.url}")
    ++ entry.customDomains;

  # tunnels: [{domains; https; http}] (backends "host:port"), in priority order.
  # swag: null, or {domains; https; http} for the local SWAG.
  # Returns the stream map lines and the port-80 server blocks.
  render = {
    tunnels,
    swag ? null,
  }: let
    # Tunnels first, so dedupe keeps their routes over the local SWAG.
    routes = proto:
      (dedupe (concatLists (map (t:
        map (name: {
          inherit name;
          backend = t.${proto};
        })
        t.domains) (tunnels ++ optional (swag != null) swag))))
      .routes;
    byName = sort (a: b: a.name < b.name);
    httpServerBlock = r: ''
      server {
        listen 80;
        server_name ${r.name};

        location / {
          proxy_pass http://${r.backend};
          proxy_set_header Host $host;
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Proto $scheme;
        }
      }
    '';
  in {
    streamMap = concatStringsSep "\n" (map (r: "${r.name} ${r.backend};") (byName (routes "https")));
    httpBlock = concatStringsSep "\n" (map httpServerBlock (byName (routes "http")));
  };
}
