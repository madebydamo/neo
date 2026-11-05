# Development philosophy

Why Neo is built the way it is. Read this before changing Neo or writing a plugin: almost every rule in [AGENTS.md](../AGENTS.md) and [Plugin development](PLUGIN-DEVELOPMENT.md) follows from one of these principles. When a rule does not cover your case, decide by the principle.

## Who it is for

**Your data, your machine, your software.** Neo exists so people can leave big-tech clouds without becoming sysadmins. Photos, files, passwords and calendars stay on hardware the owner controls, and the software that runs them is open and theirs.

**The operator is not technical.** The person running Neo knows what a calendar is, not what CalDAV is. The web UI is the product; the CLI, Nix and SSH are for when something is wrong. Every text they see (service cards, option descriptions, guides, errors) is written in plain words and leads with what they get, not with how it works.

**Opinionated defaults.** HTTPS, a login in front of every app, backups and updates are on by default. Security is not homework. A setting exists only when an operator has a real reason to change it; plumbing is internal or in Advanced.

## How it stays trustworthy

**Reproducible, revertible, recoverable.** The whole machine is a NixOS configuration built from `settings.toml`. The same settings give the same system; every activation is a generation that can be rolled back; config plus appdata restores the machine on new hardware. Anything that would break one of these three is wrong, even if it works today.

**One source of truth.** `settings.toml` becomes `config.neo` as it is. Every key is a declared option, every option is a key the web UI can edit. Nothing is configured by hand on the host, and nothing in the web UI bypasses the settings write path (`apply_edit`).

**All mutable state in appdata.** Everything a service writes lives under `${config.neo.core.volumes.appdata}/<name>`, owned by the `homeserver` user. That single rule is what makes Clear appdata, snapshots, backups and restore work for every service without per-service code. Never `/var/lib/<name>`, never `DynamicUser`.

**One front door.** Web traffic enters through SWAG (TLS, geo filters) and tinyauth (login), reaches containers over the `internal` Docker network, and leaves encrypted. No service publishes its web port. Each service chooses where it is reachable (`ingress`: local, Tailscale, web). Paths that skip the login are explicit regexes in `auth.publicPaths`.

**Secrets never touch the Nix store.** Passwords, tokens and keys are declared with `mkSecretOption`, may be encrypted at rest, and reach services only through rendered files at activation. The build refuses a secret in an `environment`, a script or `/etc`. Agents never see secret values.

## How code is shaped

**Every service has the same shape.** `option.nix` (what the operator can set), `default.nix` (what runs), `swag.nix` (how it is reached), `skills.nix` (what the assistant knows). Same helpers (`lib.neo.*`), same names, same rank bands. A reader who knows one service knows all of them; an agent can copy any one as a template.

**Dendritic, auto-imported modules.** Every `.nix` file under `nix/` (or a plugin's `modules/`) is imported by `import-tree`. Files never import siblings; they share values through options. Adding a feature is adding a file.

**Declare presentation next to the option.** Labels, ranks, groups, helpers and widgets are metadata on the option in Nix (`rank`, `helper`, `ui`). The web UI renders them generically and never names a service or option. A new need becomes a new generic widget or choice provider, reusable by every service and plugin.

**Helpers over hand-rolled code.** Sudo rules, proxy blocks, setup units, SSH keys, git access and secrets each have exactly one helper in `lib.neo` or one core option. Writing your own variant creates a second path that drifts; extend the helper instead.

**Status comes from facts.** Health shows what systemd reports about the units a service lists in `systemdUnits`, never guesses from names. Every unit a service runs is listed; post-start configuration is a `mkSetupService` that shows "done" when it worked.

**Composable.** Plugins extend Neo without forking it and can be added more than once. Every name a plugin puts into the system goes through `plugin.name`, so copies never collide.

**Mutations take locks.** Every operation that changes the machine takes a lock scope (`LockSpec`), and its button shows the conflict. No ad-hoc "in progress" flags.

## How work gets done

**The running system is the test.** A change is done when it works on a booted machine: `just launch` builds the VM, then the smoke test checks units, internal reachability, the public health path and the login redirect. Evaluation passing is not enough.

**Checks over conventions.** A rule that can be checked is checked: flake checks catch plugin names that miss a prefix, setup units that are not listed, secrets headed for the store and unknown privacy categories. When you add a rule, add the check.

**Fix root causes.** No workaround in a caller for a bug in a helper, no retry around a race, no `mkForce` to win against a module you could fix.

**AI-native, human in charge.** Every service ships a Hermes skill and every guide is one, so the assistant can operate what is installed. The assistant reads settings and proposes changes; a person clicks Save. Docs and code are written so a coding agent can follow them: short rules with the reason, one example per pattern, and a map of where things are.

**Plain writing everywhere.** Docs, comments and commit messages say what is true now, in short sentences, without marketing. A comment explains why, not what.
