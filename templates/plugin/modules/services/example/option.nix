# Example service options: a static web page (linuxserver/nginx) behind SWAG and tinyauth.
# Every file under modules/ is auto-imported (import-tree). Do not `imports` siblings.
# Rename `example` (directory, module names, the plugin.name argument) for your service.
{...}: {
  neoPlugin.modules.example-option = {
    config,
    lib,
    plugin,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; let
      name = plugin.name "example";
    in {
      options.neo.services.${name} = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "Example static web page${plugin.label}" {rank = 0;};
              # Internal options are not shown in the web UI or written to settings.toml.
              port = mkOption {
                type = types.port;
                internal = true;
                default = 80;
                description = "Port the container listens on inside the internal network";
              };
              # Service-specific options use ranks 10–89 (see nix/lib/option.nix in neo).
              greeting = mkOption {
                type = types.str;
                default = "Hello from a Neo plugin";
                rank = 10;
                description = "Text shown on the example page";
              };
              # Passwords, tokens and API keys: lib.neo.mkSecretOption, passed to the
              # container through neo.core.secrets.containerEnv (docs/LIB.md → Secrets).
            }
            # subdomain / ingress / auth / customDomains (ranks 100–130).
            // lib.neo.mkReverseProxyOptions {
              subdomain = name;
              # Regexes that bypass tinyauth, e.g. a health check for monitoring.
              auth.publicPaths = [
                "^/health$"
              ];
            }
            # Optional: route outbound traffic through the vpn service (gluetun).
            # Delete this block if the service does not need it.
            // lib.neo.mkVpnOptions {
              containers = [name];
              networks = ["internal"];
              ports = [80];
            }
            # containers.<name> = image. Overridable in settings; drives updates and the units list.
            # Extra non-docker units (setup services, timers) go in extraUnits / timerUnits.
            // lib.neo.mkContainerDefinitions {
              ${name} = "lscr.io/linuxserver/nginx:latest";
            }
            # Mutable state lives here, so Clear appdata and snapshots cover it.
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/${name}"
            // lib.neo.mkServiceMeta {
              category = "Utilities";
              # A file next to this module (svg, png, …) or a URL.
              icon = ./icon.svg;
              # One line for the services grid card: what the user gets.
              tagline = "An example web page${plugin.label}";
              # Plain sentences for non-technical users: what they can do with it.
              description = ''
                Example plugin service: a static page served by linuxserver/nginx.
                Replace this with what your service does for the people using it.
              '';
              # Operator facts, shown collapsed under "Good to know".
              notes = ''
                Serves the files under appdata/${name}/www. Replace this with how Neo runs it.
              '';
              projectUrl = "https://docs.linuxserver.io/images/docker-nginx/";
              githubUrl = "https://github.com/linuxserver/docker-nginx";
              # Upstream license, linked in the pane. If the image contains closed code or
              # the app needs a closed server: openSource = false; note = "what is closed";
              license = {
                spdx = "GPL-3.0";
                url = "https://github.com/linuxserver/docker-nginx/blob/master/LICENSE";
              };
            }
            # Hermes skill toggle. The content is filled in skills.nix.
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Example service configuration";
      };
    };
}
