{...}: {
  perSystem = {
    self',
    config,
    pkgs,
    ...
  }: {
    devShells.default = pkgs.mkShell {
      import = [];
      nativeBuildInputs = with pkgs; [
        self'.packages.neo
        nix
        git
        nixos-install-tools
      ];
    };
  };
}
