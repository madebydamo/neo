# Hermes skill for dufs.
{...}: {
  flake.modules.nixos.dufs-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.dufs;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.dufs.skill.conf = lib.neo.mkServiceSkill {
      service = "dufs";
      inherit cfg domain;
      description = "Dufs file server and WebDAV";
      tags = ["neo" "dufs" "files" "webdav"];
      title = "Neo · Dufs";
      body = ''
        ## When to Use
        Browse/upload files in the web UI, or mount the share as WebDAV (rclone, davfs2, Windows Explorer, macOS Finder).

        ## Architecture notes
        - WebDAV is the same URL as the UI. When `password` is set, SWAG skips tinyauth for OPTIONS/PROPFIND/PROPPATCH/MKCOL/COPY/MOVE/LOCK/UNLOCK and any request with an Authorization header, so clients see dufs 401/207 instead of a tinyauth 302. GET `/` is not a publicPath.

        ## Credentials
        - `services.dufs.username` / `services.dufs.password`: HTTP Basic for WebDAV (injected for the UI after tinyauth)
        - Unset `password`: native WebDAV clients cannot authenticate (tinyauth 302)
        - Do not use `@`, `:`, or `|` in the password (dufs `-a` rule syntax)

        ## Procedures
        1. Probe `http://dufs:5000/__dufs__/health` from the internal network
        2. Open the public URL, pass tinyauth — UI should list Media and Documents
        3. Point WebDAV clients at the public URL with username/password (HTTP Basic)

        ## Pitfalls
        - Deletes in the UI or via WebDAV remove real host files
        - Clearing appdata removes the writable `/data` share, not media/documents volumes

        ## Verification
        - GET `/` redirects to tinyauth
        - With `password` set, PROPFIND `/` without credentials is 401 (DAV WWW-Authenticate, not tinyauth HTML)
        - With `password` set, PROPFIND `/` with HTTP Basic returns 207 Multi-Status
        - UI lists expected directories; upload works
      '';
    };
  };
}
