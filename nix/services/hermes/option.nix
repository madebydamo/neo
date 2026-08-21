# Hermes service options.
# Order: enabled → required secrets → Telegram → optional LLM keys → model/provider/soul → proxy/skill.
{...}: {
  flake.modules.nixos.hermes-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.hermes = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Hermes Agent service" {rank = 0;};

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

              dashboardPassword = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  Internal Hermes dashboard password (required for non-loopback bind).
                  SWAG auto-logs in with this so only tinyauth is user-facing.
                '';
                rank = 10;
                helper = lib.neo.helpers.randomToken // {label = "Generate dashboard password";};
              };

              gatewayToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Gateway authentication token";
                rank = 20;
                helper = lib.neo.helpers.randomToken;
              };

              telegramBotToken = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = ''
                  Telegram bot token string.
                  Create a bot via @BotFather on Telegram.
                '';
                rank = 30;
              };

              telegramAllowedUserId = mkOption {
                type = types.listOf types.int;
                default = [];
                description = ''
                  List of Telegram user/chat IDs allowed to interact with the bot.
                  Get your ID from @userinfobot on Telegram.
                '';
                rank = 40;
              };

              telegramGroups = mkOption {
                type = types.attrsOf (
                  types.submodule {
                    options = {
                      requireMention = mkOption {
                        type = types.bool;
                        default = true;
                        rank = 0;
                        description = "Whether the bot requires an @mention in this group";
                      };
                    };
                  }
                );
                rank = 50;
                default = {};
                description = ''
                  Per-group Telegram settings. Keys are chat IDs (as strings) or "*" for default.
                '';
              };

              # Optional LLM credentials. Prefer OAuth (e.g. xAI) without keys; API keys pin provider=xai|…
              xaiApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 60;
                description = ''
                  Optional xAI (Grok) API key. When set, Neo pins model.provider = "xai".
                  Leave empty for xAI OAuth (set modelProvider = "xai-oauth") or other providers.
                '';
              };

              anthropicApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 70;
                description = ''
                  Optional Anthropic (Claude) API key. When set, Neo pins model.provider = "anthropic"
                  unless modelProvider is set explicitly.
                '';
              };

              openaiApiKey = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 80;
                description = ''
                  Optional OpenAI API key. When set, Neo pins model.provider = "openai"
                  unless modelProvider is set explicitly.
                '';
              };

              defaultModel = mkOption {
                type = types.nullOr types.str;
                default = "grok-build-latest";
                rank = 85;
                description = ''
                  Optional default LLM model id written to config.yaml as model.default
                  (e.g. "grok-4", "grok-build-latest", "claude-sonnet-4").
                  Leave empty so Nix does not pin the model — Hermes/OAuth or prior
                  config.yaml values are preserved across rebuilds.
                  Note: under the Hermes NixOS module, the dashboard cannot save model
                  changes (managed mode); pin here or use CLI OAuth flows that write auth.json.
                '';
              };

              modelProvider = mkOption {
                type = types.nullOr types.str;
                default = null;
                rank = 86;
                description = ''
                  Optional model.provider for config.yaml (e.g. "xai-oauth", "xai", "anthropic",
                  "openai", "openrouter"). Explicit value always wins.
                  If unset, Neo derives from API keys (xaiApiKey → xai, anthropicApiKey → anthropic,
                  openaiApiKey → openai); if no keys either, provider is not written so OAuth or
                  an existing config.yaml provider is left alone.
                  For xAI SuperGrok OAuth without an API key: modelProvider = "xai-oauth".
                '';
              };

              forceSoul = mkOption {
                type = types.bool;
                default = false;
                rank = 88;
                description = ''
                  When true, overwrite $HERMES_HOME/SOUL.md with Neo's default co-pilot identity
                  on every activation. When false (default), seed SOUL.md only if missing so
                  operator edits are preserved.
                '';
              };

              superviseUpdates = mkOption {
                type = types.bool;
                default = false;
                rank = 89;
                description = ''
                  After a system-updater or docker-updater run that actually changed something
                  (or failed), launch Hermes to read logs and systemd state.
                  Updater run history (JSON + logs, append-only) lives in those
                  services' appdata (`updater/docker`, `updater/system`); last.json
                  is retargeted at the start of each run.
                  Broken: notify the Hermes home channel; for Docker, retag the previous image
                  and restart the containers. Warnings or migration hints: notify only, keep
                  the new image. Clean: no message. No-op updater runs skip Hermes entirely.
                  System updates are never rolled back automatically.
                  Notifications use `hermes send --to all` (every configured home channel),
                  then `hermes send --to telegram` if needed. Run `/sethome` in Telegram/Discord/…
                  so a home channel exists.
                '';
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
              icon = "https://raw.githubusercontent.com/NousResearch/hermes-agent/main/website/static/img/logo.png";
              description = ''
                Hermes Agent is the self-improving AI agent built by Nous Research. The only agent with a built-in learning loop — it creates skills from experience, improves them during use, nudges itself to persist knowledge, and builds a deepening model of who you are across sessions.
                It is not a coding copilot tethered to an IDE or a chatbot wrapper around a single API. An autonomous agent that lives on your server, remembers what it learns, and gets more capable the longer it runs. Deploy it on a $5 VPS, a GPU cluster, or serverless infrastructure that costs nearly nothing when idle.
                Interact with it from Telegram, Discord, Slack, WhatsApp, Signal, Email, CLI, and its web dashboard. Features include persistent memory, autonomous skill creation and refinement, scheduled automations, parallel subagents, real sandboxing with multiple backends, full web and browser control, vision, and support for virtually any LLM provider.
              '';
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
