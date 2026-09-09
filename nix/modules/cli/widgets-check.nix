# Node tests for colocated option-form UI widgets (cli/templates/options/widgets).
{...}: {
  perSystem = {pkgs, ...}: {
    checks.option-form-widgets =
      pkgs.runCommand "option-form-widgets" {
        nativeBuildInputs = [pkgs.nodejs];
      } ''
        set -euo pipefail
        mkdir -p cli/static cli/templates/options
        cp ${../../../cli/static/option_form.js} cli/static/option_form.js
        cp ${../../../cli/templates/configuration.html.hbs} cli/templates/configuration.html.hbs
        cp -r ${../../../cli/templates/options/widgets} cli/templates/options/widgets
        cd cli/templates/options/widgets
        node --test *.test.js test/*.test.js
        touch "$out"
      '';
  };
}
