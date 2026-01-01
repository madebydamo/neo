{
  config,
  lib,
  pkgs,
  ...
}:
{
  neo.volumes = {
    appdata = "/home/damo/Documents/projects/homeserver/neo/DATA/AppData";
    data = "/home/damo/Documents/projects/homeserver/neo/DATA";
    media = "/home/damo/Documents/projects/homeserver/neo/DATA/Media";
    documents = "/home/damo/Documents/projects/homeserver/neo/DATA/Documents";
  };

  neo.services.swag = {
    enabled = true;
    domain = "damianmoser.ch";
    email = "damian.d.moser@gmail.com";
    # extraDomains = [ "fortilegends.ch" ];
  };

  neo.services.rathole = {
    enabled = true;
    token = "1985c3fddaa7928c7c07a30637267806";
    remoteAddr = "151.241.217.226";
    port = 2223;
    name = "dev";
  };

  neo.services.filebrowser = {
    enabled = true;
    subdomain = "filebrowser";
    additionalMountPoints = {
      # TBA
    };
  };
}
