# Enables the flake-parts modules system for storing lower-level modules.
{inputs, ...}: {
  imports = [
    inputs.flake-parts.flakeModules.modules
  ];
}
