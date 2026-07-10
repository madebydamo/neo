# Option helper constructors and shared presets for the neo web config editor.
# Helpers are UI-only metadata on options (like rank); they are never executed during
# pure NixOS evaluation. Scripts live under nix/helpers/ and resolve to flake store paths.
{lib, ...}: let
  scripts = ../helpers;

  mkHelper = {
    id,
    kind,
    script,
    label ? "Generate",
    description ? "",
    apply ? "set",
    inputs ? [],
  }:
    assert lib.assertMsg (kind == "button" || kind == "form")
    "neo.mkHelper ${id}: kind must be button|form";
    assert lib.assertMsg (apply == "set" || apply == "append")
    "neo.mkHelper ${id}: apply must be set|append";
    assert lib.assertMsg (kind != "button" || inputs == [])
    "neo.mkHelper ${id}: button kind must have empty inputs";
    assert lib.assertMsg (kind != "form" || inputs != [])
    "neo.mkHelper ${id}: form kind requires non-empty inputs";
    assert lib.assertMsg (script != null)
    "neo.mkHelper ${id}: script is required"; {
      inherit
        id
        kind
        script
        label
        description
        apply
        inputs
        ;
    };
in {
  libExtensions.helpers = {
    neo = {
      inherit mkHelper;
      helpers = {
        randomToken = mkHelper {
          id = "random-token";
          kind = "button";
          label = "Generate";
          description = "Fill with a cryptographically random secret.";
          apply = "set";
          script = scripts + "/random-token.sh";
          inputs = [];
        };

        bcryptUser = mkHelper {
          id = "bcrypt-user";
          kind = "form";
          label = "Set user";
          description = "Fill this list entry with a username:bcrypt_hash line for tinyauth (TINYAUTH_AUTH_USERS). Use + Add for a new row first.";
          # set (not append): UI applies to a specific list index via target.index
          apply = "set";
          script = scripts + "/bcrypt-user.sh";
          inputs = [
            {
              name = "username";
              type = "str";
              label = "Username";
              required = true;
              placeholder = "alice";
            }
            {
              name = "password";
              type = "password";
              label = "Password";
              required = true;
            }
          ];
        };

        mkpasswdSha512 = mkHelper {
          id = "mkpasswd-sha512";
          kind = "form";
          label = "Hash password";
          description = "Generate a SHA-512 crypt hash (mkpasswd -m sha-512).";
          apply = "set";
          script = scripts + "/mkpasswd-sha512.sh";
          inputs = [
            {
              name = "password";
              type = "password";
              label = "Password";
              required = true;
            }
          ];
        };
      };
    };
  };
}
