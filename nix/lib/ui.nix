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
#   primaryItemList — listOf scalars; first entry is the primary (badge from
#     entryLabel, e.g. Hermes telegramAllowedUserId "Home channel").
#   providerAuth — submodule of provider + (API key and/or OAuth) + model
#     (+ optional base URL). Catalog rows declare hasApiKey / hasOauth /
#     oauthFlow / needsBaseUrl. Child option descriptions render as ⓘ on
#     each input (from type.fields). OAuth status/login/refresh goes
#     through ui.oauth.script.
#   proxyRouteList — attrsOf str mapping a public domain to a plain-http
#     upstream (swag.proxyPass). Route cards (https://domain → TLS at SWAG →
#     upstream) with inline hostname / http:// URL checks; rows with errors
#     block save via widget.validate. entryLabel / emptyHint are optional.
#
# Browser JS for each widget is colocated with the Handlebars template
# (`cli/templates/options/widgets/<name>.js`) and registers on NeoWidgets.
#
# ## Presentation (settings-page layout)
#
#   group        — section the option (and every option below it) renders in.
#                  Use a catalog entry from `lib.neo.ui.groups` or `mkGroup`.
#                  Ungrouped options render in the always-open "General" section;
#                  groups render as collapsible sections ordered by `rank`.
#   label        — human label (default: humanized last path segment).
#   summary      — show this option's value as a chip on the collapsed group.
#   choiceLabels — display names for enum / ui.choices values ({ web = "Internet"; }).
#   visibleWhen  — sibling option rule: "enabled" hides the field while that bool is
#                  false (e.g. auth.publicPaths only while auth.enabled);
#                  "type=entra" hides it unless the sibling equals that value.
#                  Also applies to fields of attrsOf submodule entries.
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

  # Long-running OAuth actions for providerAuth (status / login / poll / refresh).
  # `script` is an absolute store path; extract serializes it like helpers.
  # `runAs` is a local username (e.g. hermes); the web UI sudoes the script.
  mkOauth = {
    script,
    runAs ? null,
    env ? {},
  }:
    assert lib.assertMsg (script != null)
    "neo.ui.oauth: script is required";
      {
        inherit script env;
      }
      // lib.optionalAttrs (runAs != null && runAs != "") {inherit runAs;};

  # Built-in section icons: every nix/lib/icons/<name>.svg as icons.<name>.
  icons = lib.mapAttrs' (file: _: lib.nameValuePair (lib.removeSuffix ".svg" file) (./icons + "/${file}")) (
    lib.filterAttrs (file: kind: kind == "regular" && lib.hasSuffix ".svg" file) (builtins.readDir ./icons)
  );

  # Collapsible settings section. Options inherit the group of their nearest
  # ancestor that declares one; `rank` orders groups (General is always first).
  # `icon` works like mkServiceMeta.icon: a file path (a catalog entry from
  # `icons` or a plugin's own ./icon.svg) or a URL. SVGs are tinted with the
  # theme text colour (drawn as a mask), other images render as-is.
  mkGroup = {
    id,
    label,
    description ? null,
    icon ? null,
    rank ? 100,
  }:
    assert lib.assertMsg (builtins.isString id && id != "")
    "neo.ui.group: id required";
    assert lib.assertMsg (icon == null || builtins.isPath icon || builtins.isString icon)
    "neo.ui.group: icon must be a path or URL string";
      {
        inherit id label rank;
      }
      // lib.optionalAttrs (description != null) {inherit description;}
      // lib.optionalAttrs (icon != null) {inherit icon;};

  # Shared groups for the option sets generated by lib.neo (reverse proxy, VPN,
  # Hermes skill, container images). Plugins may reuse these or mkGroup their own.
  groups = {
    access = mkGroup {
      id = "access";
      label = "Access & security";
      description = "Web address, where the service can be reached from, and login protection";
      icon = icons.globe;
      rank = 100;
    };
    assistant = mkGroup {
      id = "assistant";
      label = "AI assistant";
      description = "Let the Hermes assistant use this service";
      icon = icons.sparkles;
      rank = 200;
    };
    advanced = mkGroup {
      id = "advanced";
      label = "Advanced";
      description = "Container images and other expert settings";
      icon = icons.wrench;
      rank = 300;
    };
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
    catalog ? null,
    oauth ? null,
    group ? null,
    label ? null,
    summary ? null,
    choiceLabels ? null,
    visibleWhen ? null,
  }:
    lib.filterAttrs (_: v: v != null) {
      inherit
        group
        label
        summary
        choiceLabels
        visibleWhen
        widget
        choices
        keysFrom
        modes
        save
        emptyHint
        entryLabel
        choiceEmptyHint
        catalog
        oauth
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
          mkOauth
          mkGroup
          groups
          icons
          ;
      };
    };
  };
}
