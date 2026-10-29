# Privacy map: the kinds of personal data people usually hand to cloud
# companies, and which services keep them at home
# (docs/design/descriptions-and-guides.md §4). Services tag themselves with
# mkServiceMeta { privacy = [ "calendar" ]; }, guides with privacy = [ … ].
# Plugins add categories under neo.privacy.categories.<id>.
{...}: {
  flake.modules.nixos.privacy = {
    config,
    lib,
    ...
  }: let
    inherit (lib) mkOption types;
    icons = lib.neo.ui.icons;
    cats = config.neo.privacy.categories;
    tagged = lib.flatten (lib.mapAttrsToList (name: svc:
      map (id: {inherit name id;}) ((svc.meta or {}).privacy or []))
    config.neo.services);
    unknown = builtins.filter (t: !(cats ? ${t.id})) tagged;
  in {
    options.neo.privacy.categories = mkOption {
      internal = true;
      default = {};
      description = "Privacy map categories (data people usually keep in the cloud).";
      type = types.attrsOf (types.submodule {
        options = {
          label = mkOption {
            type = types.str;
            description = "Short name, e.g. Calendar.";
          };
          icon = mkOption {
            type = types.nullOr (types.either types.str types.path);
            default = null;
            description = "Tile icon: a file path (SVGs follow the theme) or a URL.";
          };
          cloud = mkOption {
            type = types.str;
            description = "What the usual cloud services keep about you, in one or two plain sentences.";
          };
          home = mkOption {
            type = types.str;
            default = "";
            description = "What changes when this data stays at home, in one sentence.";
          };
          examples = mkOption {
            type = types.listOf types.str;
            default = [];
            description = "Familiar cloud services for this kind of data.";
          };
          rank = mkOption {
            type = types.int;
            default = 100;
            description = "Tile order (lower first).";
          };
        };
      });
    };

    config = {
      warnings =
        map (t: "neo.services.${t.name}.meta.privacy: unknown privacy category \"${t.id}\" (known: ${lib.concatStringsSep ", " (lib.attrNames cats)})")
        unknown;

      neo.privacy.categories = lib.mapAttrs (_: lib.mkDefault) {
        calendar = {
          label = "Calendar";
          icon = icons.calendar;
          rank = 10;
          cloud = "Your calendar shows who you meet, when you are away from home and where your children go to school.";
          home = "Your appointments sync to every phone, but only your own server knows them.";
          examples = ["Google Calendar" "iCloud Calendar" "Outlook"];
        };
        contacts = {
          label = "Contacts";
          icon = icons.user;
          rank = 20;
          cloud = "Your address book holds the names, numbers and addresses of everyone you know, including people who never agreed to share them.";
          home = "Your contacts sync to your phones without a company reading your friends' details.";
          examples = ["Google Contacts" "iCloud Contacts"];
        };
        photos = {
          label = "Photos & videos";
          icon = icons.camera;
          rank = 30;
          cloud = "Photos show your family, your home and where you have been. Cloud services scan them to recognise faces, places and things.";
          home = "Your phones still back up automatically and you can search by face or place, with the analysis running on your own server.";
          examples = ["Google Photos" "iCloud Photos"];
        };
        passwords = {
          label = "Passwords";
          icon = icons.key;
          rank = 40;
          cloud = "A password manager holds the keys to all your accounts. In the cloud, it is one big target for attackers.";
          home = "Your vault lives on your server and still fills in passwords on every device.";
          examples = ["1Password" "LastPass" "iCloud Keychain"];
        };
        location = {
          label = "Location history";
          icon = icons.map;
          rank = 50;
          cloud = "A location history records where you live, work, sleep and travel, minute by minute.";
          home = "You keep a private map of your trips that nobody else can look at.";
          examples = ["Google Maps Timeline"];
        };
        documents = {
          label = "Letters & documents";
          icon = icons.document;
          rank = 60;
          cloud = "Scanned letters, bills, contracts and payslips reveal your health, money and family situation.";
          home = "Your paperwork becomes a searchable archive that never leaves your home.";
          examples = ["Google Drive" "Adobe Document Cloud" "iLovePDF"];
        };
        files = {
          label = "Files";
          icon = icons.folder;
          rank = 70;
          cloud = "Everything in your cloud drive is stored, and often scanned, by the provider.";
          home = "Your files sync between your devices and stay on your own disks.";
          examples = ["Dropbox" "Google Drive" "OneDrive" "iCloud Drive"];
        };
        notes = {
          label = "Notes & wiki";
          icon = icons.note;
          rank = 80;
          cloud = "Notes collect plans, ideas, recipes and private thoughts.";
          home = "You write and share pages with your family on your own server.";
          examples = ["Notion" "Evernote" "Confluence"];
        };
        tasks = {
          label = "To-dos";
          icon = icons.check;
          rank = 90;
          cloud = "To-do lists show what you are working on, planning and worrying about.";
          home = "Your lists sync to your phone and stay private.";
          examples = ["Todoist" "Microsoft To Do" "Trello"];
        };
        bookmarks = {
          label = "Bookmarks & read-later";
          icon = icons.bookmark;
          rank = 100;
          cloud = "What you save to read later says a lot about your interests and opinions.";
          home = "Your saved pages and notes are searchable and archived at home.";
          examples = ["Pocket" "Raindrop.io"];
        };
        search = {
          label = "Web search";
          icon = icons.search;
          rank = 110;
          cloud = "Every search is stored and tied to you: health questions, money worries, what you plan to buy.";
          home = "You search the web without a profile being built about you.";
          examples = ["Google Search" "Bing"];
        };
        browsing = {
          label = "Browsing & ads";
          icon = icons.shield;
          rank = 120;
          cloud = "Ads and trackers follow you from site to site and app to app, also on your TV.";
          home = "Ads and trackers are blocked for every device in your home.";
          examples = ["Ad networks" "Trackers in apps and smart TVs"];
        };
        recipes = {
          label = "Recipes & meal plans";
          icon = icons.utensils;
          rank = 130;
          cloud = "Recipe and meal apps learn what your family eats and buys.";
          home = "Your family cookbook and shopping lists stay with you.";
          examples = ["Paprika" "Recipe apps"];
        };
        code = {
          label = "Code & config";
          icon = icons.git;
          rank = 140;
          cloud = "Private code and configuration can contain addresses, keys and how your home is set up.";
          home = "Your repositories and images live on your own server.";
          examples = ["GitHub" "GitLab" "Docker Hub"];
        };
        automation = {
          label = "Automations";
          icon = icons.bolt;
          rank = 150;
          cloud = "Automation services hold logins to all the apps they connect.";
          home = "Your automations and the logins they use stay on your server.";
          examples = ["Zapier" "Make" "IFTTT"];
        };
        assistant = {
          label = "AI assistant";
          icon = icons.sparkles;
          rank = 160;
          cloud = "Chat assistants keep your conversations and learn about you.";
          home = "Your assistant's memory and tools live on your server; your messages go only to the AI provider you choose.";
          examples = ["ChatGPT" "Google Assistant"];
        };
      };
    };
  };
}
