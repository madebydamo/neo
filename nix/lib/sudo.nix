# Helpers for security.sudo.extraRules used by homeserver service modules.
#
# Services that need passwordless sudo for the homeserver (or other) user should
# declare rules via mkSudoExtraRules instead of hand-rolling path triples.
#
# Path coverage (per binary) mirrors what sudo may resolve after rebuilds:
#   1. Absolute path from the Nix package at eval time
#   2. /run/current-system/sw/bin/<name> (secure_path / current system)
#   3. /nix/store/*-<pname>-*/bin/<name> (other store paths / generations)
{lib, ...}: {
  libExtensions.sudo = {
    neo = rec {
      # Default options for non-interactive service/agent sudo.
      defaultSudoCommandOptions = ["NOPASSWD" "SETENV"];

      # Expand one privileged binary into the three command forms above.
      # `name` is the binary basename (e.g. "systemctl"); `pname` defaults to
      # package.pname for the store-path glob.
      mkSudoCommand = {
        package,
        name,
        options ? defaultSudoCommandOptions,
        pname ? package.pname or (lib.getName package),
      }: [
        {
          command = "${package}/bin/${name}";
          inherit options;
        }
        {
          command = "/run/current-system/sw/bin/${name}";
          inherit options;
        }
        {
          command = "/nix/store/*-${pname}-*/bin/${name}";
          inherit options;
        }
      ];

      # Build a security.sudo.extraRules value (list of one rule block).
      #
      # Usage:
      #   security.sudo.extraRules = lib.neo.mkSudoExtraRules {
      #     users = ["homeserver"];
      #     commands = [
      #       { package = pkgs.systemd; name = "systemctl"; }
      #       { package = pkgs.nix; name = "nix-store"; }
      #     ];
      #   };
      #
      # Unrestricted (e.g. Hermes agent):
      #   security.sudo.extraRules = lib.neo.mkSudoExtraRules {
      #     users = ["hermes"];
      #     all = true;
      #   };
      #
      # Each entry in `commands` is either:
      #   { package, name, options?, pname? }  → expanded via mkSudoCommand
      #   { command, options? }                → raw single sudo command entry
      mkSudoExtraRules = {
        users ? [],
        groups ? [],
        commands ? [],
        all ? false,
        options ? defaultSudoCommandOptions,
      }: let
        expand = cmd:
          if cmd ? package && cmd ? name
          then
            mkSudoCommand (cmd
              // {
                options = cmd.options or options;
              })
          else if cmd ? command
          then [
            {
              command = cmd.command;
              options = cmd.options or options;
            }
          ]
          else throw "lib.neo.mkSudoExtraRules: each command needs { package, name } or { command }";

        commandList =
          if all
          then [
            {
              command = "ALL";
              inherit options;
            }
          ]
          else lib.concatMap expand commands;
      in [
        (
          {commands = commandList;}
          // lib.optionalAttrs (users != []) {inherit users;}
          // lib.optionalAttrs (groups != []) {inherit groups;}
        )
      ];
    };
  };
}
