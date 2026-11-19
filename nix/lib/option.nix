# Helpers for mkOption / mkEnableOption that support optional UI metadata in the
# neo web config editor:
#   - rank   — sibling sort order (level-dependent)
#   - helper — fill-assist scripts (Generate / form dialogs)
#   - ui     — presentation: widgets, dynamic choices, keysFrom, save rules
#              (see nix/lib/ui.nix and AGENTS.md "Web UI option metadata")
#
# ## Level-dependent ranks (web UI)
#
# extract_service_options.nix sorts **siblings only** at each nesting level.
# Parent ranks place whole groups; child ranks never compete with other groups.
#
# Recommended top-level bands for neo.services.<name> (and similar merges):
#
#   0       enabled
#   10–89   service-specific options (API tokens, secrets, ports you care about, …)
#   100     subdomain          (mkReverseProxyOptions)
#   105     ingress            (mkReverseProxyOptions)
#   110     vpn                (mkVpnOptions group)
#   120     auth               (mkReverseProxyOptions group)
#   130     customDomains      (mkReverseProxyOptions)
#   200     skill              (mkSkillOptions group)
#   300     containers         (mkContainerDefinitions group)
#
# Within a group, use local ranks again (0, 10, 20, …). SSH fields use
# mkSshConnectionOptions { rankBase = N; } → host/user/sshKey/extra at N..N+30.
# Bind mounts: mkAdditionalMountPoints { rank = N; app = "Dufs"; example = "/data/…"; } → list of { localPath, containerPath }.
#
# Usage in option.nix files:
#   with lib;
#   with { inherit (lib.neo) mkOption mkEnableOption; };
#   enabled = mkEnableOption "private web search that does not track you" { rank = 0; };
#   foo = mkOption {
#     type = types.str;
#     default = "";
#     description = "...";
#     rank = 10;
#     helper = lib.neo.helpers.randomToken;
#     # ui = lib.neo.ui.mkUi { choices = "authApps"; };  # optional
#   };
# Also valid: `mkOption { ... } // { helper = ...; ui = ...; }` (same as rank).
#
# Default from another setting (docs/design/access-and-defaults.md):
#   name = mkInheritedOption {
#     type = types.str;
#     from = "core.hostname";
#     default = config.neo.core.hostname;
#     description = "Name of this server in Tailscale.";
#   };
# A default that is not another setting but is decided later (empty = off,
# empty = the program decides) stays nullOr with ui.emptyMeans.
{lib, ...}: let
  mkOption = {
    rank ? null,
    helper ? null,
    ui ? null,
    ...
  } @ args: let
    o = lib.mkOption (removeAttrs args ["rank" "helper" "ui"]);
    extra =
      (lib.optionalAttrs (rank != null) {inherit rank;})
      // (lib.optionalAttrs (helper != null) {inherit helper;})
      // (lib.optionalAttrs (ui != null) {inherit ui;});
  in
    if extra == {}
    then o
    else o // extra;
in {
  libExtensions.option = {
    neo = {
      inherit mkOption;

      # An option whose default is another setting (or computed from it), e.g.
      # a device name that follows the server name. `from` is the source's
      # settings path ("core.hostname", "access.defaultIngress"); the web UI
      # shows "From <label>" while the value follows it and "Use <label>" to go
      # back. `default` is the real value (computed from the source here), so
      # consumers read the option directly, never null. A key in
      # settings.toml overrides; removing it follows the source again.
      # `fromLabel` defaults to the source option's ui.label.
      # `fromModule = true`: a module sets the value from the source with
      # mkDefault (for options declared without `config`, like
      # mkReverseProxyOptions); `default` is then only the fallback and the web
      # UI reads the source value itself.
      mkInheritedOption = {
        from,
        fromLabel ? null,
        fromModule ? false,
        ...
      } @ args:
        assert lib.assertMsg (builtins.isString from && from != "")
        "lib.neo.mkInheritedOption: from must be a settings path such as \"core.hostname\"";
          (mkOption (
            {defaultText = "same as ${from}";}
            // removeAttrs args ["from" "fromLabel" "fromModule"]
          ))
          // {
            inherits =
              {path = from;}
              // lib.optionalAttrs (fromLabel != null) {label = fromLabel;}
              // lib.optionalAttrs fromModule {fromModule = true;};
          };

      # Like lib.mkEnableOption, but the text is the whole description (no
      # "Whether to enable …" prefix): say what the user gets. The first
      # letter is capitalised and a final full stop added if missing.
      mkEnableOption = text: args: let
        t = lib.trim text;
        first = builtins.substring 0 1 t;
        sentence = lib.toUpper first + builtins.substring 1 (-1) t;
        last = builtins.substring (builtins.stringLength t - 1) 1 t;
      in
        lib.mkOption {
          type = lib.types.bool;
          default = false;
          example = true;
          description =
            if builtins.elem last ["." "!" "?" ")"]
            then sentence
            else "${sentence}.";
        }
        // args;
    };
  };
}
