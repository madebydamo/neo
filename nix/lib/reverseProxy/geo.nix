# DB-IP / geo allow-deny helpers for SWAG (linuxserver/mods:swag-dbip).
{lib, ...}: {
  libExtensions.geo = {
    neo = {
      geoAccessInclude = "  include /config/nginx/geo-access.conf;\n";

      mkDbipConf = geo: let
        upper = c: lib.toUpper c;
        whitelistBody =
          if geo.countryWhitelist == []
          then "    default yes;\n"
          else
            "    default no;\n"
            + lib.concatMapStrings (c: "    ${upper c} yes;\n") geo.countryWhitelist;
        blacklistBody =
          "    default yes;\n"
          + lib.concatMapStrings (c: "    ${upper c} no;\n") geo.countryBlacklist;
        continentBody =
          "    default yes;\n"
          + lib.concatMapStrings (c: "    ${upper c} no;\n") geo.continentBlacklist;
      in ''
        geoip2 /config/geoip2db/dbip-country-lite.mmdb {
            auto_reload 1w;
            $geoip2_data_continent_code   continent code;
            $geoip2_data_country_iso_code country iso_code;
        }

        map $geoip2_data_country_iso_code $geo-whitelist {
        ${whitelistBody}}

        map $geoip2_data_country_iso_code $geo-blacklist {
        ${blacklistBody}}

        map $geoip2_data_continent_code $continent-blacklist {
        ${continentBody}}

        geo $lan-ip {
            default no;
            10.0.0.0/8 yes;
            172.16.0.0/12 yes;
            192.168.0.0/16 yes;
            127.0.0.1 yes;
        }
      '';

      geoAccessConf = ''
        if ($lan-ip = yes) {
            set $geo-whitelist yes;
            set $geo-blacklist yes;
            set $continent-blacklist yes;
        }
        if ($geo-whitelist = no) { return 404; }
        if ($geo-blacklist = no) { return 404; }
        if ($continent-blacklist = no) { return 404; }
      '';
    };
  };
}
