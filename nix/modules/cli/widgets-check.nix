# Node tests for colocated option-form UI widgets (cli/templates/options/widgets).
{...}: {
  perSystem = {pkgs, ...}: {
    checks.option-form-widgets =
      pkgs.runCommand "option-form-widgets" {
        nativeBuildInputs = [pkgs.nodejs];
      } ''
        set -euo pipefail
        mkdir -p cli/templates/options cli/src/utils
        cp ${../../../cli/src/utils/plugin_id_vectors.json} cli/src/utils/plugin_id_vectors.json
        # Mirror `just test-widgets`: all of cli/static (widget hosts + standalone modules and their tests).
        cp -r ${../../../cli/static} cli/static
        chmod -R u+w cli/static
        cp ${../../../cli/templates/configuration.html.hbs} cli/templates/configuration.html.hbs
        cp -r ${../../../cli/templates/options/widgets} cli/templates/options/widgets
        node --test cli/templates/options/widgets/*.test.js cli/templates/options/widgets/test/*.test.js cli/static/*.test.js
        touch "$out"
      '';
  };
}
