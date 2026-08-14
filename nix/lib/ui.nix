# UI metadata constructors for the neo web config editor.
# Like helpers/rank: declared on options, never executed during NixOS eval.
# Extract serializes `ui` into the option schema; the form dispatches on ui.widget
# and applies keysFrom / choices / save rules generically.
#
# ## Choice providers (resolved at extract time)
#
# Named strings under `ui.choices` map to dynamic lists in extract_service_options.nix:
#   - "authApps" — enabled services with reverse-proxy auth (tinyauth ACL multi-select)
# Inline `ui.choices = [ "a" "b" ]` is also allowed (static multi-select).
#
# ## Widgets
#
#   exclusiveListPair — attrsOf submodule with exclusive list fields + open mode
#     (see tinyauth.access). Modes declare which list fields are active per mode.
#   pluginList — listOf flake URLs with add/remove cards and per-remove uninstall
#     confirm (core.plugins). Ownership is inferred from option declarations.
#
# ## keysFrom
#
# attrsOf keys follow another option's derived values (e.g. usernames from users).
# extract: "identity" | "beforeColon"
{lib, ...}: let
  inherit (lib) types;

  mkKeysFrom = {
    option,
    extract ? "identity",
  }:
    assert lib.assertMsg (builtins.isString option && option != "")
    "neo.ui.keysFrom: option must be a non-empty string";
    assert lib.assertMsg (extract == "identity" || extract == "beforeColon")
    "neo.ui.keysFrom: extract must be identity|beforeColon"; {
      inherit option extract;
    };

  mkMode = {
    id,
    label,
    active ? [],
    listLabel ? null,
    hintEmpty ? null,
    hintFilled ? null,
    badge ? null,
  }:
    assert lib.assertMsg (builtins.isString id && id != "")
    "neo.ui.mode: id required";
    assert lib.assertMsg (builtins.isString label && label != "")
    "neo.ui.mode: label required";
      {
        inherit id label active;
      }
      // lib.optionalAttrs (listLabel != null) {inherit listLabel;}
      // lib.optionalAttrs (hintEmpty != null) {inherit hintEmpty;}
      // lib.optionalAttrs (hintFilled != null) {inherit hintFilled;}
      // lib.optionalAttrs (badge != null) {inherit badge;};

  mkSave = {
    pruneEmptyEntries ? false,
    omitIfEmpty ? false,
  }: {
    inherit pruneEmptyEntries omitIfEmpty;
  };

  # Full ui attrset; attach via mkOption { ui = lib.neo.ui.mkUi { ... }; }.
  mkUi = {
    widget ? null,
    choices ? null,
    keysFrom ? null,
    modes ? null,
    save ? null,
    emptyHint ? null,
    entryLabel ? null,
    choiceEmptyHint ? null,
  }:
    lib.filterAttrs (_: v: v != null) {
      inherit
        widget
        choices
        keysFrom
        modes
        save
        emptyHint
        entryLabel
        choiceEmptyHint
        ;
    };
in {
  libExtensions.ui = {
    neo = {
      ui = {
        inherit
          mkUi
          mkKeysFrom
          mkMode
          mkSave
          ;
      };
    };
  };
}
