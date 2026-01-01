{
  config,
  lib,
  pkgs,
  ...
}:
{
  neo.volumes = {
    appdata = "/home/damo/Documents/projects/homeserver/neo/DATA/AppData";
    data = "/home/damo/Documents/projects/homeserver/neo/DATA/data";
    media = "/home/damo/Documents/projects/homeserver/neo/DATA/media";
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
