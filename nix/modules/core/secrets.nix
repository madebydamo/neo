# Runtime secret files (docs/design/secrets.md, lib in nix/lib/secrets.nix).
#
#   /run/neo-secrets/raw/<path>        raw values from an encrypted backend, root 0400
#   /run/neo-secrets/rendered/<name>   templates with placeholders filled in, per-template owner/mode
#
# Every switch and boot re-renders all templates (activation snippet). A
# template whose placeholder has no raw value fails the activation and names
# the setting, instead of starting a service with a literal placeholder.
#
# Restarts: each template's store source is a restartTrigger of its units. The
# source holds the value (unencrypted config) or a placeholder with the
# ciphertext stamp (encrypted config), so a changed secret restarts exactly the
# units that use it.
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

    unitName = u: lib.removeSuffix ".service" u;

    # Every unit that some template restarts → its template sources.
    triggers = lib.foldl' (acc: name:
      lib.foldl' (a: u: a // {${unitName u} = (a.${unitName u} or []) ++ [sources.${name}];}) acc
      templates.${name}.restartUnits) {} (lib.attrNames templates);

    renderLines =
      lib.concatStringsSep "\n" (lib.mapAttrsToList (name: t: "neo_secrets_render ${lib.escapeShellArgs [sources.${name} t.path t.owner t.group t.mode]}")
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

        assertions =
          [
            {
              assertion = leaks == [];
              message = "neo secrets: a secret placeholder would be used literally in ${lib.concatStringsSep ", " leaks}. Pass the secret through neo.core.secrets.containerEnv or neo.core.secrets.templates.";
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
          deps = ["users" "groups" "specialfs"];
          text = ''
            (
              set -u
              umask 077
              raw=${dir}/raw
              rendered=${dir}/rendered
              install -d -m 0751 -o root -g root ${dir} "$rendered"
              install -d -m 0700 -o root -g root "$raw"
              failed=0

              # Placeholders are matched on a copy that drops each one once handled,
              # so a value that itself looks like a placeholder cannot loop.
              neo_secrets_render() {
                local src=$1 dst=$2 owner=$3 group=$4 mode=$5
                local content rest ph key val tmp
                local re='@NEO_SECRET:([A-Za-z0-9._/-]+):([A-Za-z0-9]*)@'
                content=$(cat "$src"; printf x)
                content=''${content%x}
                rest=$content
                while [[ $rest =~ $re ]]; do
                  ph=''${BASH_REMATCH[0]}
                  key=''${BASH_REMATCH[1]}
                  if [ ! -f "$raw/$key" ]; then
                    echo "neo-secrets: no value for setting $key (needed by $dst)" >&2
                    failed=1
                    return 0
                  fi
                  val=$(cat "$raw/$key"; printf x)
                  val=''${val%x}
                  content=''${content//"$ph"/"$val"}
                  rest=''${rest//"$ph"/}
                done
                tmp=$(mktemp "$dst.XXXXXX")
                printf '%s' "$content" > "$tmp"
                chown "$owner:$group" "$tmp"
                chmod "$mode" "$tmp"
                mv -f "$tmp" "$dst"
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
