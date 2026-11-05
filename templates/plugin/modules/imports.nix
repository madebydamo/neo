# flakeModules.plugin turns neoPlugin.modules into what Neo imports (once per
# plugin instance, each with its own prefix) and exports nixosModules.default.
{inputs, ...}: {
  imports = [
    inputs.flake-file.flakeModules.dendritic
    inputs.neo.flakeModules.plugin
  ];
}
