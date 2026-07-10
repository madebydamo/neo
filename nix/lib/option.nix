# Helpers for mkOption / mkEnableOption that support optional `rank` (UI order)
# and optional `helper` (UI fill-assist scripts) in the neo web config editor.
# Usage in option.nix files:
#   with lib;
#   with { inherit (lib.neo) mkOption mkEnableOption; };
# Then pass `rank = N;` and/or `helper = lib.neo.helpers.randomToken;` inside the
# attrset for mkOption, or as second arg for mkEnableOption:
#   enabled = mkEnableOption "my service" { rank = 0; };
#   foo = mkOption { type = types.str; default = ""; description = "..."; rank = 10; helper = lib.neo.helpers.randomToken; };
# Also valid: `mkOption { ... } // { helper = ...; }` (same as existing rank attachments).
{lib, ...}: {
  libExtensions.option = {
    neo = {
      mkOption = {
        rank ? null,
        helper ? null,
        ...
      } @ args: let
        o = lib.mkOption (removeAttrs args ["rank" "helper"]);
        extra =
          (lib.optionalAttrs (rank != null) {inherit rank;})
          // (lib.optionalAttrs (helper != null) {inherit helper;});
      in
        if extra == {}
        then o
        else o // extra;

      mkEnableOption = description: args:
        lib.mkEnableOption description // args;
    };
  };
}
