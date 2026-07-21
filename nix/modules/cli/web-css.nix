# packages.neo-web-css — Tailwind + daisyUI → $out/neo-ui.css (used by packages.neo STATIC_DIR).
{
  lib,
  self,
  ...
}: {
  perSystem = {pkgs, ...}: let
    npm = pkgs.buildNpmPackage {
      pname = "neo-web-css-npm";
      version = "0.1.0";
      src = lib.cleanSourceWith {
        src = self + "/cli/web-css";
        filter = path: type: let
          name = baseNameOf path;
        in
          !(name == "node_modules")
          && !(lib.hasInfix "/node_modules/" (toString path));
      };
      npmDepsHash = "sha256-/RHHcncYWVnxM29vWNbsEBZ+pjaRTN96G6BhyATbQ7M=";
      dontNpmBuild = true;
      installPhase = ''
        runHook preInstall
        mkdir -p $out
        cp -a node_modules $out/
        cp package.json input.css $out/
        runHook postInstall
      '';
    };

    neoWebCss = pkgs.stdenvNoCC.mkDerivation {
      pname = "neo-web-css";
      version = "0.1.0";
      src = lib.cleanSourceWith {
        src = self + "/cli";
        filter = path: type: let
          p = toString path;
          name = baseNameOf path;
        in
          !(lib.hasInfix "/node_modules/" p)
          && !(lib.hasInfix "/target/" p)
          && !(lib.hasInfix "/result" p)
          && name != "neo-ui.css";
      };
      nativeBuildInputs = [pkgs.nodejs];
      buildPhase = ''
        runHook preBuild
        rm -rf web-css/node_modules
        ln -s ${npm}/node_modules web-css/node_modules
        cp -f ${npm}/input.css web-css/input.css
        ./web-css/node_modules/.bin/tailwindcss \
          -i ./web-css/input.css \
          -o ./neo-ui.css \
          --minify
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        mkdir -p $out
        cp neo-ui.css $out/neo-ui.css
        runHook postInstall
      '';
    };
  in {
    packages.neo-web-css = neoWebCss;
  };
}
