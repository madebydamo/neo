# Default access for every app (docs/design/access-and-defaults.md): where apps
# can be opened from, whether they ask for the login page, and whether they
# sign in with the shared login (OIDC through tinyauth). Each app follows these
# until its own setting is changed.
#
# Options declared with lib.neo.mkInheritedOption { fromModule = true; } inside a
# service (ingress and auth.enabled from mkReverseProxyOptions, sso from
# mkSsoOption) cannot read `config` where they are declared, so this module
# gives each of them its source's value with mkDefault. settings.toml values
# (normal priority) still win.
{...}: {
  flake.modules.nixos.access = {
    config,
    lib,
    options,
    ...
  }: let
    inherit (lib) mkOption types;
    ui = lib.neo.ui;
    cfg = config.neo.access;
    tinyauth = config.neo.services.tinyauth or {};

    isOption = o: builtins.isAttrs o && (o._type or null) == "option";
    # Paths (below a service) of options that take their value from a module,
    # into plain submodules (auth.enabled), not into attrsOf / listOf entries.
    inheritedPaths = prefix: opts:
      lib.concatMap (
        k: let
          o = opts.${k};
        in
          if isOption o
          then
            if (o.inherits.fromModule or false)
            then [
              {
                path = prefix ++ [k];
                from = o.inherits.path;
              }
            ]
            else if (o.type.name or "") == "submodule"
            then inheritedPaths (prefix ++ [k]) (o.type.getSubOptions [])
            else []
          else []
      ) (lib.filter (k: !(lib.hasPrefix "_" k)) (lib.attrNames opts));

    serviceDefaults = name: let
      opt = options.neo.services.${name};
      subs =
        if isOption opt && (opt.type.name or "") == "submodule"
        then opt.type.getSubOptions []
        else {};
    in
      lib.foldl' lib.recursiveUpdate {} (map (
        i:
          lib.setAttrByPath i.path (
            lib.mkDefault (lib.getAttrFromPath (lib.splitString "." i.from) config.neo)
          )
      ) (inheritedPaths [] subs));
  in {
    options.neo.access = {
      defaultIngress =
        mkOption {
          type = types.listOf (types.enum lib.neo.ingressVariants);
          default = lib.neo.ingressVariants;
          # Each app's Reachable from (mkReverseProxyOptions) follows this.
          description = "Where apps can be opened from, unless an app sets its own. Home network means devices at home, Tailscale and NetBird your own devices anywhere, Internet everyone.";
        }
        // {
          rank = 10;
          ui = ui.mkUi {
            label = "Apps are reachable from";
            summary = true;
            choices = lib.neo.ingressVariants;
            choiceLabels = {
              local = "Home network";
              tailscale = "Tailscale";
              netbird = "NetBird";
              web = "Internet";
            };
          };
        };

      defaultLogin =
        mkOption {
          type = types.bool;
          default = true;
          # Each app's auth.enabled follows this, unless the app fixes it.
          description = "Apps ask everyone to log in on the shared login page before they open, unless an app sets its own. Apps that must always ask, or that have their own logins for phone apps, keep their setting.";
        }
        // {
          rank = 20;
          ui = ui.mkUi {
            label = "Apps ask for login";
            summary = true;
          };
        };

      defaultSso =
        mkOption {
          type = types.bool;
          default = false;
          # Each app's sso option (lib.neo.mkSsoOption) follows this: OIDC
          # through tinyauth's OIDC server (tinyauth.oidcClients).
          description = "Apps that can use it sign you in with your login page account, so you manage users in one place instead of in every app. Needs the login page (tinyauth) and a domain.";
        }
        // {
          rank = 30;
          ui = ui.mkUi {
            label = "Sign in to apps with your login page account";
            summary = true;
          };
        };
    };

    config = {
      neo.services = lib.genAttrs (lib.attrNames options.neo.services) serviceDefaults;

      assertions = [
        {
          assertion = !cfg.defaultSso || (tinyauth.enabled or false);
          message = "neo.access.defaultSso: signing in to apps with the login page account needs the login page (services.tinyauth.enabled).";
        }
      ];
    };
  };
}
