{
  config,
  lib,
  pkgs,
  ...
}:
{
  neo.volumes = {
    appdata = "/DATA/appdata";
    data = "/DATA/data";
    media = "/DATA/media";
  };

  neo.services.swag = {
    enabled = true;
    domain = "damianmoser.ch";
    email = "damian.d.moser@gmail.com";
    # extraDomains = [ "fortilegends.ch" ];
  };

  neo.services.filebrowser = {
    enabled = true;
    subdomain = "filebrowser";
    additionalMountPoints = {
      # TBA
    };
  };
}
