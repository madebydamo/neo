# Hermes skill for the Docker registry.
{...}: {
  flake.modules.nixos.registry-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.registry.skill.conf = lib.neo.mkServiceSkill {
      service = "registry";
      inherit config;
      description = "Private Docker/OCI image registry";
      tags = ["neo" "registry" "docker" "containers"];
      title = "Neo · Registry";
      body = ''
        ## When to Use
        Push, pull, list or delete private container images.

        ## Architecture notes
        - CNCF Distribution (`registry:3`) on the internal network, port 5000; SWAG proxies the public host without tinyauth
        - Data at appdata `data/` (mounted as /var/lib/registry)

        ## Credentials
        - `services.registry.users`: `username:bcrypt_hash` lines (htpasswd). Empty = open registry (no login)
        - Plain passwords are not stored; ask the operator for one or add a user with the helper

        ## Procedures
        1. `docker login <public host>` (only when users are set)
        2. `docker tag img <public host>/name:tag && docker push <public host>/name:tag`
        3. List repositories: GET `/v2/_catalog`; tags: GET `/v2/<name>/tags/list`

        ## Pitfalls
        - Deleting needs `deleteEnabled = true` and a manifest digest (`Accept: application/vnd.oci.image.manifest.v1+json`); space is freed only by `registry garbage-collect /etc/distribution/config.yml` inside the container
        - Clearing appdata deletes every stored image

        ## Verification
        - Internal GET `http://registry:5000/` returns 200
        - Public GET `/v2/` returns 401 with `WWW-Authenticate: Basic` when users are set, 200 when open
      '';
    };
  };
}
