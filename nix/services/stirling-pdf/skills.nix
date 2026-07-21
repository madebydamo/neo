# Hermes skill for Stirling PDF.
{...}: {
  flake.modules.nixos.stirling-pdf-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.stirling-pdf;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.stirling-pdf.skill.conf = lib.neo.mkServiceSkill {
      service = "stirling-pdf";
      inherit cfg domain;
      description = "Stirling PDF tools, OCR, login, appdata";
      tags = ["neo" "stirling-pdf" "pdf" "documents"];
      title = "Neo · Stirling PDF";
      body = ''
        ## When to Use
        PDF merge/split/OCR/compress/convert, appdata configs, login troubleshooting.

        ## Architecture notes
        - Image: stirlingtools/stirling-pdf (port 8080)
        - Appdata: tessdata, configs (H2 DB + settings), logs, customFiles, pipeline
        - Edge tinyauth **enabled by default**; Stirling built-in login **disabled by default**
        - Health: GET /api/v1/info/status is on publicPaths (tinyauth bypass) → {"status":"UP",...}

        ## Credentials
        - Edge: tinyauth (default)
        - Optional app login: set `services.stirling-pdf.enableLogin = true` plus initialLoginUsername/Password (first boot only)

        ## Procedures
        1. systemctl status docker-stirling-pdf
        2. curl http://stirling-pdf:8080/api/v1/info/status (from internal network)
        3. Open public URL and authenticate via tinyauth

        ## Pitfalls
        - initialLogin* only apply when enableLogin=true and only before the DB is created
        - Clearing appdata destroys users/settings/DB
        - Large uploads need reverse-proxy body size + timeouts (already set in proxyConf)

        ## Verification
        - /api/v1/info/status returns UP
        - UI loads after tinyauth; can run a simple merge/split
      '';
    };
  };
}
