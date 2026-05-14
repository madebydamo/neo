{...}: {
  config.flake.templates = {
    homeserver = {
      path = ../../templates/homeserver;
      description = "HSaaS self-replicating homeserver template";
    };
    plugin = {
      path = ../../templates/plugin;
      description = "Example plugin or homeserver";
    };
  };
}
