# Hermes service options.
# Order: enabled → required secrets → Telegram → llm (provider/key/model) → soul/supervise → proxy/skill.
{...}: {
  flake.modules.nixos.hermes-option = {
    config,
    lib,
    pkgs,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      neoHermesAuth = lib.neo.mkNeoHermesAuth pkgs ./oauth.py;
      telegramGroup = lib.neo.ui.mkGroup {
        id = "telegram";
        label = "Telegram";
        description = "Chat with Hermes in Telegram and get its messages there";
        icon = lib.neo.ui.icons.mail;
        rank = 50;
      };
    in {
      options.neo.services.hermes = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Hermes, an AI assistant that lives on your server, remembers what it learns and helps run Neo" {rank = 0;};

              gatewayPort = mkOption {
                type = types.port;
                internal = true;
                default = 18789;
                description = "Port for the Hermes gateway/API";
              };

              dashboardPort = mkOption {
                type = types.port;
                internal = true;
                default = 9119;
                description = "Port for the Hermes web dashboard UI (uses tinyauth via SWAG)";
              };

              # Required for a non-loopback dashboard bind; SWAG logs in with it
              # so tinyauth is the only login people see.
              dashboardPassword = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Password for the Hermes dashboard. Press Generate once; Neo signs you in with it, so you don't type it at login.";
                rank = 10;
                helper = lib.neo.helpers.randomToken // {label = "Generate dashboard password";};
                ui = lib.neo.ui.mkUi {label = "Dashboard password";};
              };

              # Gateway/API authentication token.
              gatewayToken = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Secret that protects Hermes' internal connection. Press Generate once and keep it.";
                rank = 20;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Gateway token";};
              };

              # Bearer token for neo web's /internal/mcp settings tools.
              neoToolsToken = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = "Lets the assistant read your settings and suggest changes, which you then save or apply in the neo web UI. Press Generate to turn it on; leave empty to keep it off.";
                rank = 25;
                helper = lib.neo.helpers.randomToken;
                ui = lib.neo.ui.mkUi {label = "Settings tools token";};
              };

              telegramBotToken = lib.neo.mkSecretOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  Token of the Telegram bot Hermes talks through.
                  Create a bot with @BotFather in Telegram and paste the token it gives you.
                '';
                rank = 30;
                ui = lib.neo.ui.mkUi {
                  group = telegramGroup;
                  label = "Bot token";
                };
              };

              # The first ID becomes TELEGRAM_HOME_CHANNEL (`hermes send --to
              # telegram`, cron delivery, update reports).
              telegramAllowedUserId = mkOption {
                type = types.listOf types.int;
                default = [];
                description = ''
                  Telegram users or chats that may talk to Hermes. The first one is where Hermes sends its own messages.
                  Get your ID from @userinfobot in Telegram; group chat IDs start with a minus.
                '';
                rank = 40;
                ui = lib.neo.ui.mkUi {
                  group = telegramGroup;
                  label = "Allowed Telegram users";
                  widget = "primaryItemList";
                  entryLabel = "Home channel";
                  emptyHint = "Add a Telegram user or chat ID. The first entry is the home channel.";
                };
              };

              telegramGroups = mkOption {
                type = types.attrsOf (
                  types.submodule {
                    options = {
                      requireMention = mkOption {
                        type = types.bool;
                        default = true;
                        rank = 0;
                        description = "Hermes only answers in this group when someone @mentions it.";
                        ui = lib.neo.ui.mkUi {
                          label = "Only answer when mentioned";
                        };
                      };
                    };
                  }
                );
                rank = 50;
                default = {};
                description = ''
                  Settings per Telegram group chat.
                  Add a group by its chat ID, or "*" for the default of all groups.
                '';
                ui = lib.neo.ui.mkUi {
                  group = telegramGroup;
                  label = "Telegram groups";
                };
              };

              # One provider + key or OAuth + model. Catalog: hermes-agent plugins/model-providers.
              llm = mkOption {
                type = types.submodule {
                  options = {
                    # config.yaml model.provider. List = hermes-agent model-provider
                    # plugins plus overlays without a plugin (SuperGrok is xai-oauth).
                    # OAuth login writes auth.json as user hermes; Anthropic is
                    # API-key only here. Ollama / vLLM: pick custom + baseUrl.
                    provider = mkOption {
                      type = types.nullOr (types.enum lib.neo.hermesProviderIds);
                      default = null;
                      apply = v:
                        if v == ""
                        then null
                        else v;
                      rank = 0;
                      description = "Which AI service Hermes uses. Paste an API key or log in below.";
                      ui = lib.neo.ui.mkUi {
                        label = "AI service";
                      };
                    };

                    # Named vendors: mapped to the plugin's env var (XAI_API_KEY,
                    # ANTHROPIC_API_KEY, HF_TOKEN, …). custom: config.yaml model.api_key.
                    apiKey = lib.neo.mkSecretOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 10;
                      example = "xai-...";
                      description = ''
                        The key from your AI service's account page.
                        Leave empty when you log in instead, or when your own AI server needs no key.
                      '';
                      ui = lib.neo.ui.mkUi {
                        label = "API key";
                      };
                    };

                    # config.yaml model.default. Empty = not pinned by Nix; in Hermes'
                    # NixOS managed mode the dashboard cannot save model changes.
                    model = mkOption {
                      type = types.nullOr types.str;
                      default = "grok-build-latest";
                      rank = 20;
                      example = "grok-4.6";
                      description = "The AI model Hermes uses. Pick one from the list or type a model name.";
                    };

                    # config.yaml model.base_url; OpenAI-compatible /v1 endpoint
                    # (Ollama, vLLM, llama.cpp, proxies).
                    baseUrl = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      rank = 30;
                      example = "http://127.0.0.1:11434/v1";
                      description = ''
                        Address of your own AI server, for example Ollama at http://127.0.0.1:11434/v1.
                        Leave empty for the built-in AI services.
                      '';
                      ui = lib.neo.ui.mkUi {
                        label = "AI server address";
                      };
                    };
                  };
                };
                default = {};
                rank = 60;
                description = "The AI service and model Hermes thinks with, and how it signs in.";
                ui = lib.neo.ui.mkUi {
                  widget = "providerAuth";
                  catalog = lib.neo.hermesProviderCatalog;
                  oauth = lib.neo.ui.mkOauth {
                    script = "${neoHermesAuth}/bin/neo-hermes-auth";
                    runAs = "hermes";
                    env = {
                      HERMES_HOME = "${config.neo.services.hermes.stateDir}/.hermes";
                    };
                  };
                };
              };

              # true: overwrite $HERMES_HOME/SOUL.md with Neo's co-pilot identity on
              # every activation. false: seed SOUL.md only if missing.
              forceSoul = mkOption {
                type = types.bool;
                default = false;
                rank = 88;
                description = "Reset Hermes' personality to the Neo default every time settings are applied (this discards your edits).";
                ui = lib.neo.ui.mkUi {
                  label = "Reset personality";
                };
              };

              # After a system-updater or docker-updater run that changed something
              # (or failed), Hermes reads logs and systemd state (run history in the
              # updaters' appdata, updater/docker and updater/system). Broken: notify
              # (`hermes send --to all`, then telegram) and, for Docker, retag the
              # previous image and restart. Warnings: notify only. Clean: no message.
              # No-op runs skip Hermes. The first telegramAllowedUserId is the
              # TELEGRAM_HOME_CHANNEL.
              superviseUpdates = mkOption {
                type = types.bool;
                default = false;
                rank = 89;
                description = ''
                  After each update, let Hermes check whether anything broke, message you, and roll back broken app updates.
                  System updates are never rolled back; Hermes only reports what failed.
                '';
                ui = lib.neo.ui.mkUi {
                  label = "Watch over updates";
                };
              };

              stateDir = mkOption {
                type = types.str;
                internal = true;
                default = "${config.neo.core.volumes.appdata}/hermes";
                description = "State directory for Hermes data (HERMES_HOME inside)";
              };
            }
            // lib.neo.mkReverseProxyOptions {subdomain = "hermes";}
            // lib.neo.mkSystemdUnits [
              "hermes-dashboard"
              "hermes-agent"
            ]
            // lib.neo.mkAppdata config.neo.services.hermes.stateDir
            // lib.neo.mkServiceMeta {
              category = "AI";
              icon = ./icon.png;
              tagline = "A personal AI assistant that lives on your server";
              description = ''
                Chat with an AI assistant on Telegram or in the browser that knows your homeserver: it answers questions about your apps, finds out why something does not work and runs tasks on a schedule.
                It remembers what it learns about you and your setup.
              '';
              notes = ''
                Hermes Agent by Nous Research. It needs an AI provider: an API key or a login under llm.
                Talks over Telegram, Discord, Slack, WhatsApp, Signal, email, the CLI and its web dashboard. Has persistent memory, writes and improves its own skills, runs scheduled automations and subagents in a sandbox.
                Every enabled service publishes a skill, so Hermes knows how this server is set up.
              '';
              replaces = ["ChatGPT" "Google Assistant"];
              privacy = ["assistant"];
              projectUrl = "https://hermes-agent.nousresearch.com/";
              githubUrl = "https://github.com/NousResearch/hermes-agent";
              releaseUrl = "https://github.com/NousResearch/hermes-agent/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Hermes Agent service configuration";
      };
    };
}
