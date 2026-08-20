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
        just
        alejandra
        nixos-install-tools
        cargo
        rustc
        rustfmt
        clippy
      ];
      shellHook = ''
        root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
        if [ -n "$root" ] && [ -d "$root/.githooks" ]; then
          git -C "$root" config --local core.hooksPath .githooks
        fi
      '';
    };
  };
}
