# Stirling PDF service implementation (single container).
# Volumes and env match the production Docker Compose guide.
{...}: {
  flake.modules.nixos.stirling-pdf = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.stirling-pdf;
      appdata = "${config.neo.core.volumes.appdata}/stirling-pdf";
    in {
      config = mkIf cfg.enabled {
        systemd.services."docker-stirling-pdf".preStart = lib.concatStringsSep "\n" [
          (lib.neo.mkActivationScriptForDir config {
            dirPath = appdata;
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/tessdata";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/configs";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/logs";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/customFiles";
          })
          (lib.neo.mkActivationScriptForDir config {
            dirPath = "${appdata}/pipeline";
          })
        ];

        virtualisation.oci-containers.containers."stirling-pdf" = {
          image = cfg.containers."stirling-pdf";
          autoStart = true;
          environment =
            {
              SECURITY_ENABLELOGIN = boolToString cfg.enableLogin;
              SYSTEM_DEFAULTLOCALE = cfg.defaultLocale;
              SYSTEM_GOOGLEVISIBILITY = boolToString cfg.googleVisibility;
              SYSTEM_ROOTURIPATH = "/";
              SYSTEMFILEUPLOADLIMIT = cfg.fileUploadLimit;
            }
            // optionalAttrs cfg.enableLogin {
              SECURITY_INITIALLOGIN_USERNAME = cfg.initialLoginUsername;
              SECURITY_INITIALLOGIN_PASSWORD = cfg.initialLoginPassword;
            };
          volumes = [
            "${appdata}/tessdata:/usr/share/tessdata"
            "${appdata}/configs:/configs"
            "${appdata}/logs:/logs"
            "${appdata}/customFiles:/customFiles:rw"
            "${appdata}/pipeline:/pipeline"
          ];
          networks = ["internal"];
        };
      };
    };
}
