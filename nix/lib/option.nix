# Helpers for mkOption / mkEnableOption that support an optional `rank` for UI ordering in the neo web config editor.
# Usage in option.nix files:
#   with lib;
#   with { inherit (lib.neo) mkOption mkEnableOption; };
# Then pass `rank = N;` inside the attrset for mkOption, or as second arg for mkEnableOption:
#   enabled = mkEnableOption "my service" { rank = 0; };
#   foo = mkOption { type = types.str; default = ""; description = "..."; rank = 10; };
{lib, ...}: {
  libExtensions.option = {
    neo = {
      mkOption = {rank ? null, ...} @ args: let
        o = lib.mkOption (removeAttrs args ["rank"]);
      in
        if rank != null
        then o // {inherit rank;}
        else o;

      mkEnableOption = description: args:
        lib.mkEnableOption description // args;
    };
  };
}
