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

        # True when a string (any config text) still carries a placeholder.
        hasPlaceholder = v: builtins.isString v && lib.hasInfix placeholderPrefix v;

        # Secret values for the deploy template's settings.nix, merged over
        # settings.toml (a value in both wins from the secrets file):
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
            backend =
              if exists sopsFile
              then "sops"
              else "plain";
            sopsFile =
              if exists sopsFile
              then sopsFile
              else null;
          };
      };
    };
  };
}
