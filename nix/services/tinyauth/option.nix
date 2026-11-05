# Tinyauth forward authentication service options.
{...}: {
  flake.modules.nixos.tinyauth-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      oauthGroup = lib.neo.ui.mkGroup {
        id = "oauth";
        label = "Single sign-on";
        description = "Sign in with OAuth providers (Entra ID, Authentik, …); app access per group";
        icon = lib.neo.ui.icons.key;
        rank = 50;
      };
    in {
      options.neo.services.tinyauth = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "the shared login page that protects your apps" {rank = 0;};
              # Entries are username:bcrypt_hash. Generate one by hand with:
              # docker run -i -t --rm ghcr.io/tinyauthapp/tinyauth:v5.2.0 user create --interactive
              users = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "People who can sign in to your apps. Click + Add, then Helper to enter a username and password.";
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
                      description = "Apps this user may open. When set, the user can open only these.";
                      rank = 0;
                      ui.choices = "authApps";
                    };
                    block = mkOption {
                      type = types.listOf types.str;
                      default = [];
                      description = "Apps this user may not open. Only used when no allowed apps are set.";
                      rank = 10;
                      ui.choices = "authApps";
                    };
                  };
                });
                default = {};
                # Keys are usernames (before ":" in users). Orphan keys for deleted users are
                # ignored at evaluation and dropped on save.
                description = "Which apps each user may open. A user without a rule can open every app.";
                rank = 15;
                ui = lib.neo.ui.mkUi {
                  label = "App access per user";
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
              # OAuth sign-in providers, keyed by provider ID. The ID is used in the
              # callback URL and in TINYAUTH_OAUTH_PROVIDERS_<ID>_*, so lowercase a-z0-9 only.
              oauth = mkOption {
                type = types.attrsOf (types.submodule {
                  options = {
                    enabled = mkOption {
                      type = types.bool;
                      default = true;
                      description = "Show this provider on the tinyauth login page.";
                      rank = 0;
                    };
                    type = mkOption {
                      type = types.enum ["generic" "entra"];
                      default = "generic";
                      # entra: URLs come from the tenant, groups from Microsoft Graph.
                      description = "Generic works with any OAuth / OIDC login server (Authentik, Keycloak, Google, …). Microsoft Entra ID only needs your tenant ID.";
                      rank = 5;
                      ui = lib.neo.ui.mkUi {
                        label = "Provider type";
                        choiceLabels = {
                          generic = "Generic OAuth / OIDC";
                          entra = "Microsoft Entra ID";
                        };
                      };
                    };
                    buttonName = mkOption {
                      type = types.str;
                      default = "";
                      description = "Text on the sign-in button. Empty shows \"Microsoft\" for Entra, otherwise the provider name.";
                      rank = 10;
                      ui = lib.neo.ui.mkUi {label = "Button text";};
                    };
                    clientId = mkOption {
                      type = types.str;
                      default = "";
                      description = "Client ID from the app you registered at the provider.";
                      rank = 20;
                      ui = lib.neo.ui.mkUi {label = "Client ID";};
                    };
                    clientSecret = lib.neo.mkSecretOption {
                      type = types.str;
                      default = "";
                      description = "Client secret from the app you registered at the provider. For Entra, use the secret value, not the secret ID.";
                      rank = 30;
                      ui = lib.neo.ui.mkUi {label = "Client secret";};
                    };
                    tenantId = mkOption {
                      type = types.str;
                      default = "";
                      description = "Directory (tenant) ID from the Entra app registration overview.";
                      rank = 40;
                      ui = lib.neo.ui.mkUi {
                        label = "Tenant ID";
                        visibleWhen = "type=entra";
                      };
                    };
                    groupNames = mkOption {
                      type = types.bool;
                      default = true;
                      description = "Use group names instead of IDs. Needs the GroupMember.Read.All permission with admin consent in Entra.";
                      rank = 50;
                      ui = lib.neo.ui.mkUi {
                        label = "Use group names";
                        visibleWhen = "type=entra";
                      };
                    };
                    authUrl = mkOption {
                      type = types.str;
                      default = "";
                      description = "Sign-in address from your provider, e.g. https://authentik.example.com/application/o/authorize/";
                      rank = 40;
                      ui = lib.neo.ui.mkUi {
                        label = "Authorization URL";
                        visibleWhen = "type=generic";
                      };
                    };
                    tokenUrl = mkOption {
                      type = types.str;
                      default = "";
                      description = "Token address from your provider, e.g. https://authentik.example.com/application/o/token/";
                      rank = 41;
                      ui = lib.neo.ui.mkUi {
                        label = "Token URL";
                        visibleWhen = "type=generic";
                      };
                    };
                    userinfoUrl = mkOption {
                      type = types.str;
                      default = "";
                      description = "User info address from your provider, e.g. https://authentik.example.com/application/o/userinfo/. It must return the email, and groups for app access.";
                      rank = 42;
                      ui = lib.neo.ui.mkUi {
                        label = "User info URL";
                        visibleWhen = "type=generic";
                      };
                    };
                    scopes = mkOption {
                      type = types.listOf types.str;
                      default = ["openid" "email" "profile"];
                      description = "What the login asks the provider for. The default usually works; Authentik includes groups in profile.";
                      rank = 45;
                      ui = lib.neo.ui.mkUi {
                        label = "Scopes";
                        visibleWhen = "type=generic";
                      };
                    };
                  };
                });
                default = {};
                description = "Let people sign in with an outside account (Microsoft, Authentik, Google, …). Name each provider in lowercase letters and digits, and register https://<login subdomain>.<your domain>/api/oauth/callback/<name> at the provider.";
                rank = 50;
                ui = lib.neo.ui.mkUi {
                  label = "Sign-in providers";
                  group = oauthGroup;
                };
              };
              oauthAutoRedirect = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Name of a sign-in provider to send people to straight away, skipping the login page. Empty shows the login page.";
                rank = 52;
                ui = lib.neo.ui.mkUi {
                  label = "Skip login page";
                  group = oauthGroup;
                };
              };
              # Groups admitted to apps. Keys of groupAccess follow this list.
              oauthGroups = mkOption {
                type = types.listOf types.str;
                default = [];
                # Matched against each provider's groups claim.
                description = "Groups at your provider whose members may use apps (for Entra: name or ID). Provider users in none of them are turned away.";
                rank = 55;
                ui = lib.neo.ui.mkUi {
                  label = "Allowed groups";
                  group = oauthGroup;
                };
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
                      description = "Apps this group does not grant. Only used when no allowed apps are set.";
                      rank = 10;
                      ui.choices = "authApps";
                    };
                  };
                });
                default = {};
                description = "Which apps the members of each allowed group may open. A group without a rule grants every app.";
                rank = 60;
                ui = lib.neo.ui.mkUi {
                  label = "App access per group";
                  group = oauthGroup;
                  widget = "exclusiveListPair";
                  keysFrom = lib.neo.ui.mkKeysFrom {option = "oauthGroups";};
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
                  emptyHint = "Add OAuth groups above to configure per-group app access.";
                  choiceEmptyHint = "No enabled apps with tinyauth yet. Enable a service with edge auth first.";
                };
              };
              sessionExpiry = mkOption {
                type = types.int;
                default = 86400;
                description = "How long people stay signed in, in seconds. The default 86400 is one day.";
                rank = 20;
                ui = lib.neo.ui.mkUi {label = "Stay signed in for (seconds)";};
              };
              backgroundImage = mkOption {
                type = types.bool;
                default = true;
                rank = 30;
                description = "Show a random photo behind the login page.";
                ui = lib.neo.ui.mkUi {label = "Login wallpaper";};
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
              # Image option always declared (UI override). The bridge unit is only
              # created and listed in systemdUnits while an Entra provider is enabled.
              # docker-updater uses systemdUnits ∩ containers, so this image is not
              # pulled or restarted when the sidecar is disabled.
              tinyauth-entra = "python:3.13-alpine";
            }
            // {
              # Gate the Entra userinfo bridge unit (and thus docker-updater
              # consumers via systemdUnits ∩ containers) the same way
              # oci-containers does: only while an Entra provider is enabled.
              systemdUnits = mkOption {
                type = types.listOf types.str;
                default =
                  ["docker-tinyauth"]
                  ++ optional (any (p: p.enabled && p.type == "entra") (attrValues config.neo.services.tinyauth.oauth)) "docker-tinyauth-entra";
                internal = true;
                description = "Systemd unit names for tinyauth and, with Entra sign-in, its userinfo bridge.";
              };
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/tinyauth"
            // lib.neo.mkServiceMeta {
              category = "Security";
              license = {
                spdx = "AGPL-3.0";
                url = "https://github.com/tinyauthapp/tinyauth/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "The login page in front of your apps";
              description = ''
                Makes sure only you and the people you add can open your apps.
                Sign in once to reach all of them, optionally with Google, GitHub or Microsoft, and add a second factor for extra safety.
              '';
              notes = ''
                Forward-auth for SWAG: username and password (bcrypt, optional TOTP), OAuth (GitHub, Google, generic, Entra) and LDAP. Configured through environment variables, no database.
                Services list the paths that skip the login (health checks, phone app APIs) in auth.publicPaths.
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
