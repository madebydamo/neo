# Access page: neo.exposure.report (nix/modules/core/exposure.nix) — apps with
# where they open from and how people log in, open ports, SSH and login page.
{
  neoFlake,
  configName ? null,
}: let
  inherit (import ./extract_lib.nix {inherit neoFlake configName;}) neoConfig tryOr;
in
  tryOr {} (neoConfig.exposure.report or {})
