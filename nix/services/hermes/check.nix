# First telegramAllowedUserId is TELEGRAM_HOME_CHANNEL (hermes send --to telegram).
{...}: {
  perSystem = {pkgs, ...}: {
    checks.hermes-telegram-home-channel = pkgs.runCommand "hermes-telegram-home-channel" {} ''
      set -euo pipefail
      option=${./option.nix}
      impl=${./default.nix}
      supervise=${./supervise.nix}

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

      touch "$out"
    '';
  };
}
