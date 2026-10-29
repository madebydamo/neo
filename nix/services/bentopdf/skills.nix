# Hermes skill for BentoPDF.
{...}: {
  flake.modules.nixos.bentopdf-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.bentopdf;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.bentopdf.skill.conf = lib.neo.mkServiceSkill {
      service = "bentopdf";
      inherit cfg domain;
      description = "BentoPDF client-side PDF tools, hidden tools config";
      tags = ["neo" "bentopdf" "pdf" "documents"];
      title = "Neo · BentoPDF";
      body = ''
        ## When to Use
        Browser-based PDF merge/split/convert/edit/sign; hiding tools; page load issues.

        ## Architecture notes
        - Image: ghcr.io/alam00000/bentopdf-simple (static nginx, port 8080)
        - Stateless: no appdata; all PDF processing happens in the user's browser
        - config.json (disabledTools, editorDisabledCategories) is generated from settings and mounted read-only
        - Edge tinyauth enabled by default

        ## Credentials
        - Edge: tinyauth (default); BentoPDF has no app login

        ## Procedures
        1. systemctl status docker-bentopdf
        2. curl http://bentopdf:8080/config.json (from internal network)
        3. Open public URL and authenticate via tinyauth

        ## Pitfalls
        - Tool IDs are the tool page URL without .html (e.g. edit-pdf)
        - Office conversion needs SharedArrayBuffer: HTTPS + COOP/COEP headers (sent by the container)

        ## Verification
        - Root URL returns 200 internally and 302 to tinyauth publicly
        - config.json reflects services.bentopdf.disabledTools
      '';
    };
  };
}
