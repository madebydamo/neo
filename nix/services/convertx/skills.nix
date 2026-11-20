# Hermes skill for ConvertX.
{...}: {
  flake.modules.nixos.convertx-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.convertx;
  in {
    config.neo.services.convertx.skill.conf = lib.neo.mkServiceSkill {
      service = "convertx";
      inherit config cfg;
      description = "ConvertX file conversion, accounts, file cleanup";
      tags = ["neo" "convertx" "files" "documents"];
      title = "Neo · ConvertX";
      body = ''
        ## When to Use
        Convert documents, images, video, audio or e-books; accounts; leftover files; container health.

        ## Architecture notes
        - One container: `convertx` (ghcr.io/c4illin/convertx, port ${toString cfg.port})
        - Image `containers.convertx` tracks `:latest`. Pin a tag there if an upgrade should wait
        - Appdata is mounted at `/app/data` (SQLite database, uploads, output)
        - `PUID`/`PGID` come from `neo.core.uid`/`gid`, `TZ` from `neo.core.timeZone`. The entrypoint chowns `/app/data` then drops to that user
        - `services.convertx.sessionSecret` is `JWT_SECRET`, passed through the secrets env file, never the unit environment. It must be at least 16 characters. Changing it signs everyone out, including anonymous sessions
        - `allowRegistration` maps to `ACCOUNT_REGISTRATION` (off by default). The first account is still created on `/setup` while the user table is empty
        - `allowUnauthenticated` maps to `ALLOW_UNAUTHENTICATED` (off by default). It is refused unless `services.convertx.auth.enabled` is true
        - `keepHours` maps to `AUTO_DELETE_EVERY_N_HOURS` (default 24). 0 disables deletion
        - `hideHistory` maps to `HIDE_HISTORY`. `language` maps to `LANGUAGE`. `maxConversions` maps to `MAX_CONVERT_PROCESS` (0 = no limit)
        - `HTTP_ALLOWED` is left unset so cookies stay Secure. Neo terminates HTTPS at SWAG
        - `GET /healthcheck` is on publicPaths and is the Docker healthcheck. It returns `{"status":"ok"}`
        - The web UI stays behind tinyauth, then ConvertX's own login unless `allowUnauthenticated`

        ## Credentials
        - Edge: tinyauth (follows Default login)
        - App accounts live in the ConvertX database. Neo does not store the account password
        - `services.convertx.sessionSecret` is the only secret Neo stores for this app

        ## Procedures
        1. systemctl status docker-convertx
        2. curl -fsS http://convertx:${toString cfg.port}/healthcheck (from the SWAG container: `docker exec swag curl …`)
        3. Open the public URL, pass tinyauth, and on a fresh database create the first account on the welcome page
        4. Convert a small file and download the result

        ## Pitfalls
        - An empty or rotating sign-in key signs everyone out on the next start
        - Skip the app's own login without Require login exposes conversion to anyone who can reach the address. The assertion blocks that combination
        - Clearing appdata deletes accounts and every stored file
        - The image is large (LibreOffice, ffmpeg, Calibre, TeX). The first pull and start are slow
        - Conversions of big video files need disk space under appdata and can sit at maxConversions

        ## Verification
        - /healthcheck returns status ok from inside the Docker network
        - The public UI returns 302 to tinyauth when Require login is on
        - A converted file downloads, and old files disappear after keepHours
      '';
    };
  };
}
