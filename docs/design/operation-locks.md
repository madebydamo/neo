# Operation locks

Status: implemented. Every operation that changes the machine (web route,
background job, CLI subcommand, updater timer) takes lock **scopes**, so two
operations never step on each other and buttons show the conflict. Code:
`cli/src/utils/locks/` (`LockSpec`, the module comment has the mechanics),
`cli/src/commands/web/locks.rs`, `cli/static/locks.js`,
`nix/modules/core/operation-locks.nix`. What the CLI takes and `--lock-wait`:
[CLI.md](../CLI.md).

## Scopes

Each scope is taken shared or exclusive (readers–writer); each is an `flock` on
`/run/neo/locks/<scope>.lock`, released when the process exits, even after a
crash.

| Operation | Scopes (`LockSpec`) |
|-----------|---------------------|
| Activate, update, generation switch, store repair, data restore | `system` exclusive (`system_change`) |
| Snapshot restore, clear appdata | `system` shared + `service/<name>` + every `unit/<unit>` exclusive (`service_data`) |
| Unit start/stop/restart, image pull | `unit/<unit>` (`unit`) |
| Snapshot | `service/<name>` shared |
| Settings writes | `system` shared (`system_shared`) |

## Web routes

- Take the lock with `web::locks::try_lock`. Long jobs move the guard into
  their task.
- Answer a conflict with `Blocked` (409, toasted by `static/locks.js`).
- Mark buttons with `data-neo-lock="<scope>[:sh] …"` (`lock_attr`); `locks.js`
  disables them while a live holder conflicts.

## Start guards

Snapshot restore and clear appdata take the service down with `units::quiesce`
(`cli/src/commands/web/units/quiesce.rs`), which also sets unit **start
guards**: `/run/neo/guard/<unit>`, enforced by a global `AssertPathExists=!`
drop-in for every service and target in `nix/modules/core/operation-locks.nix`.
systemd then refuses to start the unit, whoever asks (timers, dependencies, a
manual `systemctl start`). Guards go away with the lock.

## New mutating operation

Pick a `LockSpec`, take it, annotate its button. No ad-hoc in-progress checks.
