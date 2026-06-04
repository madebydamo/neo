{...}: {
  perSystem = {
    self',
    config,
    pkgs,
    ...
  }: {
    formatter = pkgs.alejandra;
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
