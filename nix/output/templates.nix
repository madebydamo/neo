{...}: {
  config.flake.templates = {
    homeserver = {
      path = ../../templates/homeserver;
      description = "HSaaS self-replicating homeserver template";
    };
  };
}
