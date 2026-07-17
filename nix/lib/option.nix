# Helpers for mkOption / mkEnableOption that support optional `rank` (UI order)
# and optional `helper` (UI fill-assist scripts) in the neo web config editor.
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
#   110     vpn                (mkVpnOptions group)
#   120     auth               (mkReverseProxyOptions group)
#   130     customDomains      (mkReverseProxyOptions)
#   200     skill              (mkSkillOptions group)
#   300     containers         (mkContainerDefinitions group)
#
# Within a group, use local ranks again (0, 10, 20, …). SSH fields use
# mkSshConnectionOptions { rankBase = N; } → host/user/sshKey/extra at N..N+30.
#
# Usage in option.nix files:
#   with lib;
#   with { inherit (lib.neo) mkOption mkEnableOption; };
#   enabled = mkEnableOption "my service" { rank = 0; };
#   foo = mkOption {
#     type = types.str;
#     default = "";
#     description = "...";
#     rank = 10;
#     helper = lib.neo.helpers.randomToken;
#   };
# Also valid: `mkOption { ... } // { helper = ...; }` (same as rank attachments).
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
