{...}: {
  perSystem = {
    self',
    config,
    pkgs,
    ...
  }: {
    formatter = pkgs.alejandra;
    devShells.default = pkgs.mkShell {
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
        nodejs
      ];
      shellHook = ''
        root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
        if [ -n "$root" ] && [ -d "$root/.githooks" ]; then
          git -C "$root" config --local core.hooksPath .githooks
        fi
        # bash and fish find neo's completion through XDG_DATA_DIRS; zsh only
        # reads fpath, so a zsh started inside the dev shell gets it via FPATH.
        # An exported FPATH replaces zsh's default fpath, so keep that default.
        if [ -z "''${FPATH:-}" ] && command -v zsh >/dev/null 2>&1; then
          FPATH="$(zsh -fc 'print -r -- $FPATH' 2>/dev/null || true)"
        fi
        export FPATH="${self'.packages.neo}/share/zsh/site-functions''${FPATH:+:$FPATH}"
      '';
    };
  };
}
