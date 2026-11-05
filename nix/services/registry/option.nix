# Docker registry (CNCF Distribution) service options.
# Auth is the registry's own htpasswd (bcrypt) basic auth, not tinyauth:
# docker/podman clients answer a WWW-Authenticate challenge and cannot follow
# a tinyauth 302, so edge auth is unavailable for this service.
{...}: {
  flake.modules.nixos.registry-option = {
    config,
    lib,
    ...
  }:
    with lib;
    with {inherit (lib.neo) mkOption mkEnableOption;}; {
      options.neo.services.registry = mkOption {
        type = types.submodule {
          options =
            {
              enabled = mkEnableOption "your own private store for Docker images" {rank = 0;};
              port = mkOption {
                type = types.port;
                internal = true;
                default = 5000;
                description = "Internal port the registry listens on";
              };
              # username:bcrypt_hash lines (htpasswd -nbB), rendered to the registry htpasswd file.
              # When set, every push and pull needs `docker login` with one of these users.
              users = mkOption {
                type = types.listOf types.str;
                default = [];
                rank = 10;
                description = "Accounts allowed to upload and download images. Use the helper to add one. If empty, anyone who can reach the registry can use it.";
                helper = lib.neo.helpers.bcryptUser;
                ui = lib.neo.ui.mkUi {label = "Accounts";};
              };
              # REGISTRY_STORAGE_DELETE_ENABLED: delete manifests/blobs by digest via the API.
              # Deleting only unlinks data; registry garbage-collect frees the space.
              deleteEnabled = mkOption {
                type = types.bool;
                default = false;
                rank = 20;
                description = "Allow deleting images from the registry. Deleted images keep using disk space until the registry's cleanup is run by hand.";
                ui = lib.neo.ui.mkUi {label = "Allow deletes";};
              };
            }
            // lib.neo.mkReverseProxyOptions {
              subdomain = "registry";
              auth.available = false;
            }
            // lib.neo.mkContainerDefinitions {
              registry = "registry:3";
            }
            // lib.neo.mkAppdata "${config.neo.core.volumes.appdata}/registry"
            // lib.neo.mkServiceMeta {
              category = "Files";
              license = {
                spdx = "Apache-2.0";
                url = "https://github.com/distribution/distribution/blob/main/LICENSE";
              };
              icon = ./icon.svg;
              tagline = "Store your own app images";
              description = ''
                A private place for Docker images you build yourself, so your servers download them from home instead of a public registry. Mostly useful for developers.
              '';
              notes = ''
                Based on CNCF Distribution. Push with docker push <subdomain>.<domain>/name:tag and pull from any machine or from neo services.
                Add users to require docker login (htpasswd with bcrypt). With no users the registry is open, so restrict ingress or add a user before exposing it to the internet.
                Image data lives in appdata; enable deletes and run garbage-collect to reclaim space.
              '';
              replaces = ["Docker Hub"];
              privacy = ["code"];
              projectUrl = "https://distribution.github.io/distribution/";
              githubUrl = "https://github.com/distribution/distribution";
              releaseUrl = "https://github.com/distribution/distribution/releases";
            }
            // lib.neo.mkSkillOptions {};
        };
        default = {};
        description = "Docker registry configuration";
      };
    };
}
