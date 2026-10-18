# Tinyauth forward authentication service options.
{...}: {
  flake.modules.nixos.tinyauth-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      entraGroup = lib.neo.ui.mkGroup {
        id = "entra";
        label = "Microsoft Entra ID";
        description = "Sign in with Microsoft work accounts; app access per Entra group";
        icon = "key";
        rank = 50;
      };
    in {
      options.neo.services.tinyauth = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "tinyauth forward authentication service" {rank = 0;};
              users = mkOption {
                type = types.listOf types.str;
                default = [];
                description = ''
                  List of users in username:bcrypt_hash format.
                  Use "Add user" in the web UI, or generate with:
                  docker run -i -t --rm ghcr.io/tinyauthapp/tinyauth:v5.2.0 user create --interactive
                '';
                rank = 10;
                helper = lib.neo.helpers.bcryptUser;
              };
              # Per-user app ACLs. Keys = usernames (before ":" in users).
              # allow/block list service names; UI multi-select via ui.choices = "authApps".
              # Presentation: exclusiveListPair widget (mode + single grid), keysFrom users.
              access = mkOption {
                type = types.attrsOf (types.submodule {
                  options = {
                    allow = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = ''
                        Whitelist of neo service names this user may access.
                        If non-empty, the user may only use these apps (do not also set block).
                      '';
                      rank = 0;
                      ui.choices = "authApps";
                    };
                    block = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = ''
                        Blacklist of neo service names this user may not access.
                        Only applied when allow is empty.
                      '';
                      rank = 10;
                      ui.choices = "authApps";
                    };
                  };
                });
                default = {};
                description = ''
                  Per-user application access control. Attribute names are Tinyauth usernames
                  (the part before ":" in users). Empty allow and block means full access.
                  Orphan keys for deleted users are ignored at evaluation and dropped on save
                  when the UI only shows active users.
                '';
                rank = 15;
                ui = lib.neo.ui.mkUi {
                  widget = "exclusiveListPair";
                  keysFrom = lib.neo.ui.mkKeysFrom {
                    option = "users";
                    extract = "beforeColon";
                  };
                  modes = [
                    (lib.neo.ui.mkMode {
                      id = "open";
                      label = "All apps";
                      active = [];
                      badge = "success";
                      hintEmpty = "No restrictions — this user can open every app behind tinyauth.";
                      hintFilled = "No restrictions — this user can open every app behind tinyauth.";
                    })
                    (lib.neo.ui.mkMode {
                      id = "allow";
                      label = "Allow list";
                      active = ["allow"];
                      listLabel = "Allowed apps";
                      badge = "primary";
                      hintEmpty = "Pick at least one app. Saving with none selected keeps full access.";
                      hintFilled = "This user can only open the apps you check below.";
                    })
                    (lib.neo.ui.mkMode {
                      id = "block";
                      label = "Block list";
                      active = ["block"];
                      listLabel = "Blocked apps";
                      badge = "warning";
                      hintEmpty = "Pick apps to block, or stay on All apps.";
                      hintFilled = "This user can open every app except the ones you check below.";
                    })
                  ];
                  save = lib.neo.ui.mkSave {
                    pruneEmptyEntries = true;
                    omitIfEmpty = true;
                  };
                  entryLabel = "User";
                  emptyHint = "Add users above to configure per-user app access.";
                  choiceEmptyHint = "No enabled apps with tinyauth yet. Enable a service with edge auth first.";
                };
              };
              entra = mkOption {
                type = types.submodule {
                  options = {
                    enabled = mkEnableOption "Sign in with Microsoft (Entra ID)" {rank = 0;};
                    tenantId = mkOption {
                      type = types.str;
                      default = "";
                      description = "Directory (tenant) ID from the Entra app registration overview.";
                      rank = 10;
                      ui.visibleWhen = "enabled";
                    };
                    clientId = mkOption {
                      type = types.str;
                      default = "";
                      description = "Application (client) ID from the Entra app registration overview.";
                      rank = 20;
                      ui.visibleWhen = "enabled";
                    };
                    clientSecret = mkOption {
                      type = types.str;
                      default = "";
                      description = "Client secret value (Certificates & secrets), not the secret ID.";
                      rank = 30;
                      ui.visibleWhen = "enabled";
                    };
                    groupNames = mkOption {
                      type = types.bool;
                      default = true;
                      description = ''
                        Match groups by display name. Requests the GroupMember.Read.All
                        Graph permission (needs admin consent). Off: list groups by object ID.
                      '';
                      rank = 40;
                      ui.visibleWhen = "enabled";
                    };
                    buttonName = mkOption {
                      type = types.str;
                      default = "Microsoft";
                      description = "Label of the sign-in button on the tinyauth login page.";
                      rank = 50;
                      ui.visibleWhen = "enabled";
                    };
                    autoRedirect = mkOption {
                      type = types.bool;
                      default = false;
                      description = "Skip the tinyauth login page and go straight to Microsoft.";
                      rank = 60;
                      ui.visibleWhen = "enabled";
                    };
                  };
                };
                default = {};
                description = ''
                  Microsoft Entra ID sign-in. Redirect URI for the app registration:
                  https://<tinyauth subdomain>.<domain>/api/oauth/callback/entra
                '';
                rank = 50;
                ui = lib.neo.ui.mkUi {group = entraGroup;};
              };
              # Entra groups admitted to apps. Keys of groupAccess follow this list.
              entraGroups = mkOption {
                type = types.listOf types.str;
                default = [];
                description = ''
                  Entra groups (display name, or object ID) whose members may use apps.
                  Signed-in Microsoft users in none of these groups are denied everywhere.
                '';
                rank = 55;
                ui = lib.neo.ui.mkUi {group = entraGroup;};
              };
              groupAccess = mkOption {
                type = types.attrsOf (types.submodule {
                  options = {
                    allow = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = "Apps members of this group may open (only these).";
                      rank = 0;
                      ui.choices = "authApps";
                    };
                    block = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = "Apps this group does not grant. Only applied when allow is empty.";
                      rank = 10;
                      ui.choices = "authApps";
                    };
                  };
                });
                default = {};
                description = ''
                  Optional narrowing per Entra group: the apps members of that group may open.
                  A group without restrictions grants every app behind tinyauth.
                '';
                rank = 60;
                ui = lib.neo.ui.mkUi {
                  group = entraGroup;
                  widget = "exclusiveListPair";
                  keysFrom = lib.neo.ui.mkKeysFrom {option = "entraGroups";};
                  modes = [
                    (lib.neo.ui.mkMode {
                      id = "open";
                      label = "All apps";
                      active = [];
                      badge = "success";
                      hintEmpty = "Members of this group can open every app behind tinyauth.";
                      hintFilled = "Members of this group can open every app behind tinyauth.";
                    })
                    (lib.neo.ui.mkMode {
                      id = "allow";
                      label = "Allow list";
                      active = ["allow"];
                      listLabel = "Allowed apps";
                      badge = "primary";
                      hintEmpty = "Pick at least one app. Saving with none selected grants all apps.";
                      hintFilled = "Members of this group can open the apps you check below.";
                    })
                    (lib.neo.ui.mkMode {
                      id = "block";
                      label = "Block list";
                      active = ["block"];
                      listLabel = "Not granted";
                      badge = "warning";
                      hintEmpty = "Pick apps this group does not grant, or stay on All apps.";
                      hintFilled = "This group grants every app except the ones you check below.";
                    })
                  ];
                  save = lib.neo.ui.mkSave {
                    pruneEmptyEntries = true;
                    omitIfEmpty = true;
                  };
                  entryLabel = "Group";
                  emptyHint = "Add Entra groups above to configure per-group app access.";
                  choiceEmptyHint = "No enabled apps with tinyauth yet. Enable a service with edge auth first.";
                };
              };
              sessionExpiry = mkOption {
                type = types.int;
                default = 86400;
                description = "Session expiry time in seconds (default 24h)";
                rank = 20;
              };
              backgroundImage = mkOption {
                type = types.bool;
                default = true;
                rank = 30;
                description = "Random login wallpaper.";
              };
              port = mkOption {
                type = types.port;
                default = 3000;
                internal = true;
                description = "Port on which tinyauth listens";
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "tinyauth";
              auth.available = false;
            }
            // lib.neo.mkContainerDefinitions {
              tinyauth = "ghcr.io/tinyauthapp/tinyauth:v5.2.0";
              # Image override only; the bridge unit is created under mkIf entra
              # and listed in systemdUnits only when entra is on. docker-updater
              # uses systemdUnits ∩ containers, so this image is not pulled when
              # the sidecar is disabled.
              tinyauth-entra = "python:3.13-alpine";
            }
            // {
              # Gate the Entra userinfo bridge unit (and thus docker-updater
              # consumers) the same way oci-containers uses mkIf entra.enabled.
              systemdUnits = mkOption {
                type = types.listOf types.str;
                default =
                  ["docker-tinyauth"]
                  ++ optional config.neo.services.tinyauth.entra.enabled "docker-tinyauth-entra";
                internal = true;
                description = "Systemd unit names for tinyauth and, with Entra sign-in, its userinfo bridge.";
              };
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/tinyauth"
            // lib.neo.mkServiceMeta {
              category = "Security";
              icon = ./icon.svg;
              description = ''
                Tinyauth is the tiniest OpenID Connect (OIDC) authentication and authorization server for your self-hosted applications.
                It functions as lightweight middleware adding secure username/password (bcrypt + optional TOTP), OAuth (GitHub, Google, generic), and LDAP login flows, working seamlessly with reverse proxies such as SWAG/Nginx, Traefik, and Caddy.
                Configuration happens exclusively through environment variables — no dashboards, databases, or complex setup files needed — keeping the statically linked binary tiny and extremely low on resources.
                In the Neo homeserver it acts as the central forward-auth provider, protecting the web UIs of other services while providing its own clean login page.
              '';
              projectUrl = "https://tinyauth.app/";
              githubUrl = "https://github.com/tinyauthapp/tinyauth";
              releaseUrl = "https://github.com/tinyauthapp/tinyauth/releases";
              rank = 10;
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Tinyauth forward authentication service configuration";
      };
    };
}
