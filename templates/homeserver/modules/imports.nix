{inputs, ...}: {
  imports = [
    inputs.flake-file.flakeModules.dendritic
  ];
  flake-file.outputs = "inputs: inputs.flake-parts.lib.mkFlake { inherit inputs; } (inputs.import-tree ./modules)";
}
