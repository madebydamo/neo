# Guides: outcome-focused how-tos ("Block ads for the whole home") shown in the
# neo web UI and published to Hermes. Metadata lives here in Nix, the text in
# a Markdown file next to the declaring module (docs/design/descriptions-and-guides.md §3).
#
#   neo.guides.adblock-at-home = {
#     title = "Block ads for the whole home";
#     summary = "Every phone, TV and laptop at home without ads.";
#     services = ["pihole"];
#     body = ./guide.md;
#   };
#
# Plugins add guides the same way. Links inside the Markdown use
# service:<name>, option:<service>.<path>, guide:<id>, privacy:<category>.
{...}: {
  flake.modules.nixos.guides = {lib, ...}: let
    inherit (lib) mkOption types;
  in {
    options.neo.guides = mkOption {
      internal = true;
      default = {};
      description = "Guides shown in the neo web UI (title, services, Markdown body).";
      type = types.attrsOf (types.submodule ({name, ...}: {
        options = {
          id = mkOption {
            type = types.str;
            default = name;
            readOnly = true;
            description = "Guide id (the attribute name); used in guide: links and URLs.";
          };
          title = mkOption {
            type = types.str;
            description = "Short outcome, e.g. Block ads for the whole home.";
          };
          summary = mkOption {
            type = types.str;
            default = "";
            description = "One or two sentences shown on the guide card.";
          };
          icon = mkOption {
            type = types.nullOr (types.either types.str types.path);
            default = null;
            description = "Card icon: a URL or a file path, like mkServiceMeta.icon. Empty: the first service's icon.";
          };
          services = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Services the guide sets up; the guide page offers to enable them.";
          };
          optional = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Services the guide mentions but does not need.";
          };
          privacy = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Privacy map categories this guide brings home, e.g. calendar.";
          };
          difficulty = mkOption {
            type = types.enum ["easy" "medium" "advanced"];
            default = "easy";
            description = "How much experience the guide expects.";
          };
          minutes = mkOption {
            type = types.nullOr types.ints.positive;
            default = null;
            description = "Rough time to complete, in minutes.";
          };
          tags = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Free-form tags for filtering, e.g. network, family.";
          };
          rank = mkOption {
            type = types.nullOr types.int;
            default = null;
            description = "Sort order on the guides page (lower first; unranked guides follow by title).";
          };
          body = mkOption {
            type = types.path;
            description = "Markdown file with the guide text.";
          };
        };
      }));
    };
  };
}
