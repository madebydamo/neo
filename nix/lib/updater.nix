# Shared paths for system/docker updater change markers consumed by Hermes supervision.
{...}: {
  libExtensions.updater = {
    neo = rec {
      updaterStateDir = "/var/lib/neo/updater";
      dockerUpdaterManifest = "${updaterStateDir}/docker-last.json";
      systemUpdaterManifest = "${updaterStateDir}/system-last.json";
    };
  };
}
