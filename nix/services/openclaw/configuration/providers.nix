# OpenClaw model provider definitions.
# Conditionally includes xAI, Anthropic, and OpenAI providers based on
# which API keys are configured, along with model specs and costs.
{...}: {
  flake.modules.nixos.openclaw-config-providers = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.openclaw;

      hasAnyApiKey = cfg.xaiApiKey != null || cfg.anthropicApiKey != null || cfg.openaiApiKey != null;

      xaiProvider = optionalAttrs (cfg.xaiApiKey != null) {
        xai = {
          baseUrl = "https://api.x.ai/v1";
          apiKey = cfg.xaiApiKey;
          api = "openai-completions";
          models = [
            {
              id = "grok-4-1-fast-reasoning";
              name = "Grok 4.1 Fast Reasoning";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 2000000;
              maxTokens = 4096;
              cost = {
                input = 0.2;
                output = 0.5;
                cacheRead = 0.05;
                cacheWrite = 0.05;
              };
            }
            {
              id = "grok-4.20-0309-reasoning";
              name = "Grok 4.20 Multi Agent";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 2000000;
              maxTokens = 4096;
              cost = {
                input = 2;
                output = 6;
                cacheRead = 0.2;
                cacheWrite = 0.2;
              };
            }
          ];
        };
      };

      anthropicProvider = optionalAttrs (cfg.anthropicApiKey != null) {
        anthropic = {
          baseUrl = "https://api.anthropic.com";
          apiKey = cfg.anthropicApiKey;
          api = "anthropic-messages";
          models = [
            {
              id = "claude-sonnet-4-20250514";
              name = "Claude Sonnet 4";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 200000;
              maxTokens = 16384;
              cost = {
                input = 3.0;
                output = 15.0;
                cacheRead = 0.3;
                cacheWrite = 3.75;
              };
            }
            {
              id = "claude-opus-4-20250514";
              name = "Claude Opus 4";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 200000;
              maxTokens = 32000;
              cost = {
                input = 15.0;
                output = 75.0;
                cacheRead = 1.5;
                cacheWrite = 18.75;
              };
            }
          ];
        };
      };

      openaiProvider = optionalAttrs (cfg.openaiApiKey != null) {
        openai = {
          baseUrl = "https://api.openai.com/v1";
          apiKey = cfg.openaiApiKey;
          api = "openai-completions";
          models = [
            {
              id = "gpt-4o";
              name = "GPT-4o";
              reasoning = false;
              input = [
                "text"
                "image"
              ];
              contextWindow = 128000;
              maxTokens = 16384;
              cost = {
                input = 2.5;
                output = 10.0;
                cacheRead = 1.25;
                cacheWrite = 0;
              };
            }
            {
              id = "o3";
              name = "o3";
              reasoning = true;
              input = [
                "text"
                "image"
              ];
              contextWindow = 200000;
              maxTokens = 100000;
              cost = {
                input = 10.0;
                output = 40.0;
                cacheRead = 2.5;
                cacheWrite = 0;
              };
            }
          ];
        };
      };
    in
      mkIf (cfg.enabled && hasAnyApiKey) {
        home-manager.users.openclaw.programs.openclaw.config.models = {
          mode = "merge";
          providers = xaiProvider // anthropicProvider // openaiProvider;
        };
      };
}
