# Secret options (docs/design/secrets.md).
#
# A secret option is a plain string option tagged `secret = true` (web UI:
# masked input). Its value is the secret itself on an unencrypted config, or a
# placeholder `@NEO_SECRET:<path>@` once the value comes from an
# encrypted file. Services never put a secret into the store-visible unit
# environment directly; they go through neo.core.secrets (nix/modules/core/secrets.nix):
#
#   neo.core.secrets.containerEnv.<container> = { VAR = "${cfg.user}:${cfg.password}"; };
#   neo.core.secrets.templates."<name>" = { content = …; owner = …; restartUnits = […]; };
#   …config.neo.core.secrets.templates."<name>".path   # rendered at activation
#
# Placeholders are replaced from /run/neo-secrets/raw/<path> when templates are
# rendered, so the same service code works for both backends.
#
# Usage in option.nix (same extra keys as lib.neo.mkOption):
#   password = lib.neo.mkSecretOption {
#     default = "";
#     description = "WebDAV password";
#     rank = 20;
#   };
{lib, ...}: {
  libExtensions.secrets = {
    neo = {
      secretsDir = "/run/neo-secrets";

      mkSecretOption = {
        rank ? null,
        helper ? null,
        ui ? null,
        ...
      } @ args:
        lib.mkOption ({type = lib.types.str;} // removeAttrs args ["rank" "helper" "ui"])
        // lib.optionalAttrs (rank != null) {inherit rank;}
        // lib.optionalAttrs (helper != null) {inherit helper;}
        // lib.optionalAttrs (ui != null) {inherit ui;}
        // {secret = true;};

      secrets = rec {
        # Marker that every placeholder starts with (leak guard, web UI masking).
        placeholderPrefix = "@NEO_SECRET:";

        # `path` mirrors the settings path below neo (services/dufs/password).
        mkPlaceholder = path: "${placeholderPrefix}${path}@";

        isPlaceholder = v: builtins.isString v && lib.hasPrefix placeholderPrefix v;

        # True when a string (any config text) still carries a placeholder
        # (also the JSON variant below).
        hasPlaceholder = v: builtins.isString v && lib.hasInfix "@NEO_SECRET" v;

        # RFC 4648 base64 of a (byte) string; nixpkgs lib has no toBase64.
        toBase64 = str: let
          inherit (lib) stringToCharacters concatStrings elemAt;
          inherit (lib.strings) charToInt;
          alphabet = stringToCharacters "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
          bytes = map charToInt (stringToCharacters str);
          len = builtins.length bytes;
          at = i:
            if i < len
            then elemAt bytes i
            else 0;
          chunk = i: let
            n = (at (i * 3)) * 65536 + (at (i * 3 + 1)) * 256 + (at (i * 3 + 2));
            c = k: elemAt alphabet (lib.mod (n / k) 64);
          in
            c 262144
            + c 4096
            + (
              if i * 3 + 1 < len
              then c 64
              else "="
            )
            + (
              if i * 3 + 2 < len
              then c 1
              else "="
            );
        in
          concatStrings (builtins.genList chunk ((len + 2) / 3));

        # Values derived from text that may hold secrets: computed now for plain
        # text, else a placeholder the renderer computes after filling it in.
        derived = fn: compute: text:
          if hasPlaceholder text
          then "@NEO_SECRET_${fn}:${toBase64 text}@"
          else compute text;
        # Hex SHA-256 of text (e.g. keys derived from an app secret).
        sha256 = derived "SHA256" (builtins.hashString "sha256");
        # Base64 of text (e.g. an HTTP Basic header).
        base64 = derived "BASE64" toBase64;

        # nginx lines holding secrets for a SWAG conf: rendered as a snippet the
        # conf includes. SWAG's preStart rewrites proxy-confs/ from the store on
        # every start, but leaves nginx/neo-secrets/ alone. A change restarts SWAG.
        #   s = lib.neo.secrets.swagSnippet config "dufs-auth" ''proxy_set_header …;'';
        #   neo.core.secrets.templates = s.templates;   … ''${s.include} in the conf
        swagSnippet = config: name: lines: let
          file = "neo-secrets/${name}.conf";
        in {
          templates."swag-${name}.conf" = {
            content = lines;
            owner = toString config.neo.core.uid;
            group = toString config.neo.core.gid;
            mode = "0440";
            copies = ["${config.neo.core.volumes.appdata}/swag/nginx/${file}"];
            restartUnits = ["docker-swag.service"];
          };
          include = "include /config/nginx/${file};";
        };

        # A secret as a JSON string literal (quotes included), for JSON or a
        # <script>: toJSON of a plain value; for a placeholder the renderer
        # inserts the JSON-escaped value ("</" escaped too).
        jsonString = v:
          if isPlaceholder v
          then "@NEO_SECRET_JSON:${lib.removeSuffix "@" (lib.removePrefix placeholderPrefix v)}@"
          else builtins.replaceStrings ["</"] ["<\\/"] (builtins.toJSON v);

        # Secret values (and the matching neo.core.secrets settings) for the
        # deploy template's settings.nix, merged over settings.toml (a value in
        # both wins from the secrets file):
        #   secrets.yaml       plain values (unencrypted repo)
        #   secrets.sops.yaml  placeholders; sops-nix decrypts at activation
        # Only one of the two may exist.
        load = {
          plainFile ? null,
          sopsFile ? null,
        }: let
          exists = f: f != null && builtins.pathExists f;
          # `lib` here is plain nixpkgs lib, so take the reader from its file.
          inherit ((import ./secrets-yaml.nix {inherit lib;}).libExtensions.secrets-yaml.neo.secrets) readYaml;
          checkSegment = path: k:
            if builtins.match "[A-Za-z0-9._:-]+" k != null
            then k
            else throw "secrets: key `${k}` under ${lib.concatStringsSep "." path} may only use letters, digits and . _ : -";
          walk = leaf: path: v:
            if builtins.isAttrs v
            then lib.mapAttrs (k: walk leaf (path ++ [(checkSegment path k)])) v
            else leaf path v;
          plain =
            if exists plainFile
            then walk (_: v: v) [] (readYaml (builtins.readFile plainFile))
            else {};
          encrypted =
            if exists sopsFile
            then walk (path: _: mkPlaceholder (lib.concatStringsSep "/" path)) [] (readYaml (builtins.readFile sopsFile))
            else {};
        in
          if exists plainFile && exists sopsFile
          then throw "secrets: both secrets.yaml and secrets.sops.yaml exist; keep only one (neo secrets enable removes the plain file)"
          else {
            values = lib.recursiveUpdate plain encrypted;
            # Options to merge too; built here so they always match this neo version.
            settings = lib.optionalAttrs (exists sopsFile) {
              core.secrets = {
                backend = "sops";
                inherit sopsFile;
              };
            };
          };
      };
    };
  };
}
