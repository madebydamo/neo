# Secret options (docs/design/secrets.md).
#
# A secret option is a plain string option tagged `secret = true` (web UI:
# masked input). Its value is the secret itself on an unencrypted config, or a
# placeholder `@NEO_SECRET:<path>:<stamp>@` once the value comes from an
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

        # `path` mirrors the settings path below neo (services/dufs/password);
        # `stamp` changes whenever the stored value changes (restart trigger).
        mkPlaceholder = path: stamp: "${placeholderPrefix}${path}:${stamp}@";

        isPlaceholder = v: builtins.isString v && lib.hasPrefix placeholderPrefix v;

        # True when a string (any config text) still carries a placeholder.
        hasPlaceholder = v: builtins.isString v && lib.hasInfix placeholderPrefix v;
      };
    };
  };
}
