# Runtime secret files (docs/design/secrets.md, lib in nix/lib/secrets.nix).
#
#   /run/neo-secrets/raw/<path>        raw values from an encrypted backend, root 0400
#   /run/neo-secrets/rendered/<name>   templates with placeholders filled in, per-template owner/mode
#
# Every switch and boot re-renders all templates (activation snippet). A
# template whose placeholder has no raw value fails the activation and names
# the setting, instead of starting a service with a literal placeholder.
#
# Encrypted config (backend sops, secrets.sops.yaml): before rendering, the
# file is decrypted with the SSH host key (identityFile) into raw/. The
# `neo-secrets-root` helper (sudo for neo-web) decrypts the same way and escrows
# the host key for admins (identity.sops.yaml).
#
# Restarts: a template whose rendered content changed queues its restartUnits
# in /run/nixos/activation-restart-list (switch-to-configuration restarts them).
# On an unencrypted config the store source (holding the value) is also a
# restartTrigger; with sops it only holds placeholders.
#
# Leak guard: a placeholder in a container or unit environment, or in an
# /etc text file, is a build error. Those paths never get rendered.
{...}: {
  flake.modules.nixos.core-secrets = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.core.secrets;
    dir = lib.neo.secretsDir;
    inherit (lib.neo.secrets) hasPlaceholder;

    envContent = env:
      lib.concatStrings (lib.mapAttrsToList (k: v: "${k}=${v}\n") env);

    templateModule = {
      name,
      config,
      ...
    }: {
      options = {
        content = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "File content; secret option values (or their placeholders) may appear anywhere in it";
        };
        env = lib.mkOption {
          type = lib.types.nullOr (lib.types.attrsOf lib.types.str);
          default = null;
          description = "Render as an env file (KEY=value per line, for docker --env-file / EnvironmentFile)";
        };
        owner = lib.mkOption {
          type = lib.types.str;
          default = "root";
          description = "Owner of the rendered file";
        };
        group = lib.mkOption {
          type = lib.types.str;
          default = "root";
          description = "Group of the rendered file";
        };
        mode = lib.mkOption {
          type = lib.types.str;
          default = "0400";
          description = "Mode of the rendered file";
        };
        restartUnits = lib.mkOption {
          type = lib.types.listOf lib.types.str;
          default = [];
          description = "Units (name.service) restarted when the rendered content changes";
        };
        copies = lib.mkOption {
          type = lib.types.listOf lib.types.str;
          default = [];
          description = "Extra paths that get the same rendered file (owner/mode too), for services that expect it at a fixed place";
        };
        path = lib.mkOption {
          type = lib.types.str;
          readOnly = true;
          default = "${dir}/rendered/${name}";
          description = "Path of the rendered file";
        };
        text = lib.mkOption {
          type = lib.types.str;
          internal = true;
          readOnly = true;
          default =
            if config.env != null
            then envContent config.env
            else if config.content != null
            then config.content
            else throw "neo.core.secrets.templates.${name}: set content or env";
          description = "Unrendered text (content or env lines)";
        };
      };
    };

    templates = cfg.templates;
    validName = name: builtins.match "[A-Za-z0-9][A-Za-z0-9._-]*" name != null;

    sources = lib.mapAttrs (name: t: pkgs.writeText "neo-secret-template-${name}" t.text) templates;

    sopsEnv = "SOPS_AGE_SSH_PRIVATE_KEY_FILE=${lib.escapeShellArg cfg.identityFile}";

    # Root helper (sudo for neo-web, see nix/services/neo/default.nix):
    #   decrypt <file.sops.yaml>       decrypted document as JSON on stdout
    #   escrow <ssh-ed25519 pub>...    the host key as identity.sops.yaml, encrypted to those keys only
    rootHelper = pkgs.writeShellApplication {
      name = "neo-secrets-root";
      runtimeInputs = [pkgs.sops pkgs.coreutils pkgs.gnused];
      text = ''
        export ${sopsEnv}
        case "''${1:-}" in
          decrypt)
            f=''${2:?usage: neo-secrets-root decrypt <file.sops.yaml>}
            case "$f" in *.sops.yaml) ;; *) echo "not a .sops.yaml file: $f" >&2; exit 2 ;; esac
            exec sops decrypt --input-type yaml --output-type json "$f"
            ;;
          escrow)
            shift
            [ "$#" -gt 0 ] || { echo "usage: neo-secrets-root escrow <ssh-ed25519 public key>..." >&2; exit 2; }
            recipients=$(IFS=,; echo "$*")
            tmp=$(mktemp -d)
            trap 'rm -rf "$tmp"' EXIT
            chmod 700 "$tmp"
            {
              echo "ssh_host_ed25519_key: |"
              sed 's/^/  /' ${lib.escapeShellArg cfg.identityFile}
              printf 'ssh_host_ed25519_key_pub: "%s"\n' "$(cat ${lib.escapeShellArg "${cfg.identityFile}.pub"})"
            } > "$tmp/identity.yaml"
            sops encrypt --age "$recipients" --input-type yaml --output-type yaml "$tmp/identity.yaml"
            ;;
          *)
            echo "usage: neo-secrets-root decrypt <file.sops.yaml> | escrow <ssh-ed25519 public key>..." >&2
            exit 2
            ;;
        esac
      '';
    };

    unitName = u: lib.removeSuffix ".service" u;

    # Every unit that some template restarts → its template sources.
    triggers = lib.foldl' (acc: name:
      lib.foldl' (a: u: a // {${unitName u} = (a.${unitName u} or []) ++ [sources.${name}];}) acc
      templates.${name}.restartUnits) {} (lib.attrNames templates);

    renderLines =
      lib.concatStringsSep "\n" (lib.mapAttrsToList (name: t: "neo_secrets_render ${lib.escapeShellArgs ([sources.${name} t.path t.owner t.group t.mode (lib.concatStringsSep ":" t.copies)] ++ t.restartUnits)}")
        templates);

    keep = lib.concatStringsSep " " (map lib.escapeShellArg (lib.attrNames templates));

    leaks =
      lib.concatLists (lib.mapAttrsToList (c: v:
        lib.mapAttrsToList (k: _: "container ${c}: environment.${k}")
        (lib.filterAttrs (_: val: hasPlaceholder val) (v.environment or {})))
      config.virtualisation.oci-containers.containers)
      ++ lib.concatLists (lib.mapAttrsToList (s: v:
        lib.mapAttrsToList (k: _: "systemd.services.${s}: environment.${k}")
        (lib.filterAttrs (_: val: hasPlaceholder val) (v.environment or {})))
      config.systemd.services)
      ++ lib.mapAttrsToList (n: _: "environment.etc.${n}")
      (lib.filterAttrs (_: e: hasPlaceholder (e.text or null)) config.environment.etc);
  in {
    options.neo.core.secrets = {
      sopsFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        internal = true;
        visible = false;
        description = "secrets.sops.yaml of the config repo (backend sops). Set by the deploy template's settings.nix";
      };

      identityFile = lib.mkOption {
        type = lib.types.str;
        default = "/etc/ssh/ssh_host_ed25519_key";
        internal = true;
        visible = false;
        description = "SSH ed25519 private key that decrypts secrets.sops.yaml (the machine identity)";
      };

      hostKey = lib.mkOption {
        type = lib.types.str;
        default = "";
        internal = true;
        visible = false;
        description = "Public half of identityFile, recorded by `neo secrets enable` so the repo knows its recipients";
      };

      backend = lib.mkOption {
        type = lib.types.enum ["plain" "sops"];
        default = "plain";
        internal = true;
        visible = false;
        description = "Where secret values come from: plain (settings.toml / secrets.yaml) or sops (secrets.sops.yaml). Set by the deploy template's settings.nix";
      };

      templates = lib.mkOption {
        type = lib.types.attrsOf (lib.types.submodule templateModule);
        default = {};
        internal = true;
        visible = false;
        description = "Files rendered at activation with secret values filled in (see nix/lib/secrets.nix)";
      };

      containerEnv = lib.mkOption {
        type = lib.types.attrsOf (lib.types.attrsOf lib.types.str);
        default = {};
        internal = true;
        visible = false;
        description = "Secret environment per OCI container: rendered env file, added to environmentFiles, restarts docker-<container>";
      };
    };

    config = lib.mkMerge [
      {
        neo.core.secrets.templates = lib.mapAttrs' (c: env:
          lib.nameValuePair "oci-${c}.env" {
            inherit env;
            restartUnits = ["docker-${c}.service"];
          })
        cfg.containerEnv;

        virtualisation.oci-containers.containers =
          lib.mapAttrs (c: _: {
            environmentFiles = [templates."oci-${c}.env".path];
          })
          cfg.containerEnv;

        systemd.services = lib.mapAttrs (_: srcs: {restartTriggers = srcs;}) triggers;

        environment.systemPackages = [rootHelper];

        assertions =
          [
            {
              assertion = leaks == [];
              message = "neo secrets: a secret placeholder would be used literally in ${lib.concatStringsSep ", " leaks}. Pass the secret through neo.core.secrets.containerEnv or neo.core.secrets.templates.";
            }
          ]
          ++ [
            {
              assertion = cfg.backend != "sops" || cfg.sopsFile != null;
              message = "neo.core.secrets: backend sops needs sopsFile (secrets.sops.yaml)";
            }
          ]
          ++ lib.mapAttrsToList (name: t: {
            assertion = validName name;
            message = "neo.core.secrets.templates: \"${name}\" is not a valid file name";
          })
          templates
          ++ lib.concatLists (lib.mapAttrsToList (name: t:
            lib.mapAttrsToList (k: v: {
              assertion = !(lib.hasInfix "\n" v);
              message = "neo.core.secrets.templates.${name}: env ${k} contains a newline (env files hold one line per variable)";
            }) (
              if t.env == null
              then {}
              else t.env
            ))
          templates);

        system.activationScripts.neo-secrets = {
          deps = ["users" "groups" "specialfs" "etc"];
          text = ''
            (
              set -u
              umask 077
              raw=${dir}/raw
              rendered=${dir}/rendered
              install -d -m 0751 -o root -g root ${dir} "$rendered"
              install -d -m 0700 -o root -g root "$raw"
              failed=0
              ${lib.optionalString (cfg.backend == "sops") ''
              # Decrypt every value into raw/<path> (keys joined with /).
              rm -rf "$raw"
              install -d -m 0700 -o root -g root "$raw"
              if json=$(${sopsEnv} ${pkgs.sops}/bin/sops decrypt --input-type yaml --output-type json ${cfg.sopsFile}); then
                while IFS=$'\t' read -r key b64; do
                  case "$key" in "" | /* | *..*) continue ;; esac
                  install -d -m 0700 "$raw/$(dirname "$key")"
                  printf '%s' "$b64" | ${pkgs.coreutils}/bin/base64 -d > "$raw/$key"
                  chmod 0400 "$raw/$key"
                done < <(printf '%s' "$json" | ${pkgs.jq}/bin/jq -r 'paths(scalars) as $p | [($p | map(tostring) | join("/")), (getpath($p) | tostring | @base64)] | @tsv')
              else
                echo "neo-secrets: cannot decrypt ${cfg.sopsFile} with ${cfg.identityFile}" >&2
                failed=1
              fi
              unset json
            ''}
              # Placeholders are matched on a copy that drops each one once handled,
              # so a value that itself looks like a placeholder cannot loop.
              # Fill placeholders in the global $text. Plain values as-is, the JSON
              # variant as an escaped string literal, derived ones (SHA256/BASE64)
              # computed from their base64-encoded input after filling it.
              neo_secrets_value() {
                if [ ! -f "$raw/$1" ]; then
                  echo "neo-secrets: no value for setting $1 (needed by $2)" >&2
                  failed=1
                  return 1
                fi
              }
              neo_secrets_fill_plain() {
                local re='@NEO_SECRET:([A-Za-z0-9._/:-]+)@' rest=$text ph key val
                while [[ $rest =~ $re ]]; do
                  ph=''${BASH_REMATCH[0]}
                  key=''${BASH_REMATCH[1]}
                  neo_secrets_value "$key" "$1" || return 1
                  val=$(cat "$raw/$key"; printf x)
                  val=''${val%x}
                  text=''${text//"$ph"/"$val"}
                  rest=''${rest//"$ph"/}
                done
              }
              neo_secrets_fill() {
                local red='@NEO_SECRET_(SHA256|BASE64):([A-Za-z0-9+/=]+)@'
                local rej='@NEO_SECRET_JSON:([A-Za-z0-9._/:-]+)@'
                local rest ph fn key val outer
                rest=$text
                while [[ $rest =~ $red ]]; do
                  ph=''${BASH_REMATCH[0]}
                  fn=''${BASH_REMATCH[1]}
                  outer=$text
                  text=$(printf '%s' "''${BASH_REMATCH[2]}" | ${pkgs.coreutils}/bin/base64 -d; printf x)
                  text=''${text%x}
                  neo_secrets_fill_plain "$1" || return 1
                  case $fn in
                    SHA256)
                      val=$(printf '%s' "$text" | ${pkgs.coreutils}/bin/sha256sum)
                      val=''${val%% *}
                      ;;
                    BASE64) val=$(printf '%s' "$text" | ${pkgs.coreutils}/bin/base64 -w0) ;;
                  esac
                  text=''${outer//"$ph"/"$val"}
                  rest=''${rest//"$ph"/}
                done
                rest=$text
                while [[ $rest =~ $rej ]]; do
                  ph=''${BASH_REMATCH[0]}
                  key=''${BASH_REMATCH[1]}
                  neo_secrets_value "$key" "$1" || return 1
                  val=$(${pkgs.jq}/bin/jq -Rs . < "$raw/$key")
                  val=''${val//<\//<\\/}
                  text=''${text//"$ph"/"$val"}
                  rest=''${rest//"$ph"/}
                done
                neo_secrets_fill_plain "$1"
              }

              neo_secrets_render() {
                local src=$1 dst=$2 owner=$3 group=$4 mode=$5 copies=$6
                shift 6
                local text tmp
                text=$(cat "$src"; printf x)
                text=''${text%x}
                neo_secrets_fill "$dst" || return 0
                tmp=$(mktemp "$dst.XXXXXX")
                printf '%s' "$text" > "$tmp"
                chown "$owner:$group" "$tmp"
                chmod "$mode" "$tmp"
                # Changed (not new: boot renders everything) → restart its units.
                if [ -e "$dst" ] && ! ${pkgs.diffutils}/bin/cmp -s "$tmp" "$dst"; then
                  for u in "$@"; do echo "$u" >> /run/nixos/activation-restart-list; done
                fi
                mv -f "$tmp" "$dst"
                local IFS=:
                for c in $copies; do
                  mkdir -p "$(dirname "$c")"
                  cp -p "$dst" "$c.neo-tmp" && mv -f "$c.neo-tmp" "$c"
                done
              }

              ${renderLines}

              # Drop files of templates that no longer exist.
              keep=(${keep})
              for f in "$rendered"/*; do
                [ -e "$f" ] || continue
                n=''${f##*/}
                case " ''${keep[*]-} " in
                  *" $n "*) ;;
                  *) rm -f "$f" ;;
                esac
              done

              [ "$failed" -eq 0 ]
            )
          '';
        };
      }
    ];
  };
}
