# First telegramAllowedUserId is TELEGRAM_HOME_CHANNEL (hermes send --to telegram).
# LLM is a single llm.{provider,apiKey,model,baseUrl} group, not per-vendor keys.
{...}: {
  perSystem = {pkgs, ...}: {
    checks.hermes-telegram-home-channel = pkgs.runCommand "hermes-telegram-home-channel" {} ''
      set -euo pipefail
      option=${./option.nix}
      impl=${./default.nix}
      supervise=${./supervise.nix}
      catalog=${../../lib/hermes-providers.nix}

      if ! grep -q 'widget = "primaryItemList"' "$option"; then
        echo "FAIL hermes telegramAllowedUserId must use primaryItemList widget" >&2
        exit 1
      fi
      if ! grep -q 'entryLabel = "Home channel"' "$option"; then
        echo "FAIL hermes primaryItemList must label the first ID as Home channel" >&2
        exit 1
      fi
      if ! grep -q 'TELEGRAM_HOME_CHANNEL = telegramHomeChannel' "$impl"; then
        echo "FAIL hermes default.nix must set TELEGRAM_HOME_CHANNEL from first allowed ID" >&2
        exit 1
      fi
      if ! grep -q 'TELEGRAM_HOME_CHANNEL = telegramHomeChannel' "$supervise"; then
        echo "FAIL hermes supervise.nix must set TELEGRAM_HOME_CHANNEL from first allowed ID" >&2
        exit 1
      fi
      if ! grep -q 'builtins.head cfg.telegramAllowedUserId' "$impl"; then
        echo "FAIL hermes default.nix must take head of telegramAllowedUserId" >&2
        exit 1
      fi
      if ! grep -q 'builtins.head cfg.telegramAllowedUserId' "$supervise"; then
        echo "FAIL hermes supervise.nix must take head of telegramAllowedUserId" >&2
        exit 1
      fi

      if ! grep -q 'llm = mkOption' "$option"; then
        echo "FAIL hermes option.nix must expose llm submodule" >&2
        exit 1
      fi
      if ! grep -q 'hermesProviderIds' "$option"; then
        echo "FAIL hermes llm.provider must use lib.neo.hermesProviderIds" >&2
        exit 1
      fi
      if grep -qE 'xaiApiKey|anthropicApiKey|openaiApiKey|openrouterApiKey|modelProvider|defaultModel' "$option" "$impl" "$supervise"; then
        echo "FAIL hermes must not keep split per-vendor LLM keys" >&2
        exit 1
      fi
      if ! grep -q 'mkHermesLlmEnv' "$impl"; then
        echo "FAIL hermes default.nix must map llm.apiKey via mkHermesLlmEnv" >&2
        exit 1
      fi
      if ! grep -q 'mkHermesLlmEnv' "$supervise"; then
        echo "FAIL hermes supervise.nix must map llm.apiKey via mkHermesLlmEnv" >&2
        exit 1
      fi
      if ! grep -q 'plugins/model-providers' "$catalog"; then
        echo "FAIL hermes provider catalog must be parsed from hermes-agent plugins/model-providers" >&2
        exit 1
      fi
      if ! grep -q 'inputs.hermes-agent' "$catalog"; then
        echo "FAIL hermes provider catalog must read the hermes-agent flake input" >&2
        exit 1
      fi
      if ! grep -q '_OAUTH_CAPABLE_PROVIDERS' "$catalog"; then
        echo "FAIL hermes provider catalog must parse OAuth-capable ids from hermes-agent" >&2
        exit 1
      fi
      if ! grep -q 'HERMES_OVERLAYS' "$catalog"; then
        echo "FAIL hermes provider catalog must parse Hermes overlays (xai-oauth SuperGrok)" >&2
        exit 1
      fi
      if ! grep -q 'openai-codex' "$catalog"; then
        echo "FAIL hermes provider catalog must treat openai-codex as OAuth" >&2
        exit 1
      fi
      if ! grep -q 'needsBaseUrl' "$catalog"; then
        echo "FAIL hermes provider catalog must mark custom as needsBaseUrl / hasApiKey" >&2
        exit 1
      fi

      touch "$out"
    '';
  };
}
