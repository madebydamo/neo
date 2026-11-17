# Changes that need a server restart

Status: implemented. Code: `nix/modules/core/restart.nix` (its header comment
lists the reasons and the core trigger history), `restart-required.sh`,
`nix/services/system-updater/`. Operator view: [CLI.md](../CLI.md); plugin
view: [PLUGIN-DEVELOPMENT.md](../PLUGIN-DEVELOPMENT.md#changes-that-need-a-restart).

## What counts

`neo-restart-required` says whether the new system needs a server restart:

- the boot default is ahead of the running system;
- new kernel, initrd or kernel boot options;
- dockerd still runs on an older `daemon.json` (a switch restarts Docker, but
  it can come back from the old unit);
- a raised **restart trigger**.

Kernel and Docker settings are detected by themselves and need no trigger.

## Restart triggers

- A change only a restart applies raises its component's own trigger inside its
  `mkIf cfg.enabled`: `neo.core.restart.triggers.<service> = { level; reason; }`.
  - Core: `triggers.core` in `restart.nix`.
  - Plugins: `plugin.name "<svc>"`.
  - `onEnable` when even switching it on needs one.
- A trigger is due when its level is above the one it started with in this boot
  (`/run/neo/restart-baseline.json`). So disabled or newly enabled components
  never restart the server.

## Who restarts

- `neo activate` records the reasons and never restarts.
- The system updater restarts once per system and reasons (`rebootWhenRequired`
  / `rebootAfterBoot`) and records a check for the next boot (`neo-restart-check`,
  `nix/services/system-updater/restart-check.sh`).
- `neo-auto-update-verify` adds `restart_check` to the run's history, and Hermes
  supervises the run then instead of before the restart.
- The decision logic runs from the updater of the *old* system, so a change to
  it takes effect one run late.

## Checks

Flake checks `restart-required` and `restart-check`
(`nix/output/restart-required-check.nix`, `nix/output/restart-check-check.nix`).
