# AGENTS.md - Development Guidelines for Neo Homeserver (Nix + Rust CLI)

## Overview
This is a Nix-based homeserver configuration that builds QEMU VMs for various self-hosted services using flakes. It follows the [Dendritic pattern](https://github.com/mightyiam/dendritic): flake-parts with `import-tree` that auto-imports all non-entrypoint `.nix` files from `./nix/` as modules. Each file implements a focused feature (e.g. `nix/services/<name>/{default,option,swag}.nix`).

The project also includes a Rust CLI (`cli/`) for bootstrap, init, build, and management operations, built via crane in Nix.

**flake.nix** uses `inputs.import-tree ./nix` for automatic module loading. See `nix/modules/flakeparts/` and `nix/lib/` for extensions.

No traditional unit tests; validation via `nix flake check`, VM runtime tests, and Rust integration tests (minimal).

## Build, Lint, and Test Commands (justfile + Nix)

### Core Commands
```bash
just build      # Runs neo nuke/init/build -> creates VM (see justfile for details)
just launch     # shutdown + build + launch QEMU VM (4CPU/8G, port 2222)
just ssh        # SSH into running VM (uses tools/id_ed25519)
just exec CMD   # SSH and run CMD in VM (e.g. 'systemctl status filebrowser')
just logs SVC   # journalctl -b -u SVC
just status     # Check QEMU, monitor, SSH, disk status
just shutdown   # Graceful QEMU shutdown via monitor or pkill
```

### Lint and Format
```bash
just format  # alejandra . (Nix formatter)
just check   # git add changed files; nix flake check (in root + build/)
```

**Rust specific:**
- `cargo fmt --all -- --check` (or via nix develop)
- `cargo clippy -- -D warnings`
- Build CLI: `nix build .#neo` or `nix run .#neo -- build`

### Test Commands
No dedicated unit tests currently (previous test package removed). Validation focuses on:

```bash
nix flake check -L                  # Full syntax, typecheck, build checks
nix build .#neo                     # Test Rust CLI build
just build && just launch           # Full VM integration test
just exec 'systemctl status <svc>'  # Single service test (e.g. filebrowser, swag)
just exec 'journalctl -u <svc> --no-pager'  # Service logs
just exec 'systemctl list-units --type=service'  # List all services
just exec 'neo --help'              # Test CLI in VM
```

**Running a single test:**
- For Nix checks: `nix build .#checks.x86_64-linux.<check-name>` (see flake outputs)
- For Rust (if adding tests): `cargo test <test_function> -- --test-threads=1`
- Service-specific: `just exec 'systemctl --no-pager status <svc> && echo "OK"'`
- Full lint+check: `just format && just check && nix flake check`

Use `nix develop` for Rust/Cargo environment with neo CLI prebuilt.

## Code Style Guidelines

### Nix - File Structure (Dendritic Pattern)
- All files end in `.nix`
- `nix/services/<name>/`: `option.nix` (options), `default.nix` (impl + mkIf), `swag.nix` (reverse proxy config)
- `nix/lib/`: reusable fns (activation, auth, types)
- `nix/modules/*/*.nix`: core, bootstrap, disko, flakeparts
- Keep files <200 lines. One concern per file.
- Start every file with `# Descriptive comment.`
- Auto-imported; no manual imports in flake.nix except top-level.
- See `nix/lib/default.nix:19` for lib extension pattern.

### Nix - Imports and Structure
```nix
# Service description comment.
{ config, lib, ... }: 
let
  cfg = config.neo.services.example;
in {
  imports = [ ./option.nix ./swag.nix ];  # Always include
  # activationScripts here if needed
} // lib.mkIf cfg.enabled {
  # main config
}
```
- Always `with lib;` or qualify `lib.mkIf`, `lib.mkOption`.
- Use `let cfg = config.neo.services.foo; in` pattern (see `nix/services/filebrowser/default.nix:9`).
- Reference lib extensions as `lib.neo.mkActivationScriptForDir config { ... }` (defined in `nix/lib/activation/dir.nix:4`).

### Naming Conventions (Nix)
- Variables: camelCase (`additionalMountPoints`)
- Nix attrs/options: snake_case (`neo.services.filebrowser`, `neo.core.volumes.appdata`, `neo.neo-service`)
- Functions: camelCase (`mkActivationScriptForDir`, `mkReverseProxyOptions`, `mkContainerDefinitions`, `mkSystemdUnits`)
- Options: `enabled = mkEnableOption "description";`
- Use plain strings for all descriptions (not `lib.mdDoc`).

### Option Definitions (in option.nix)
```nix
options.neo.services.example = mkOption {
  type = types.submodule {
    options = {
      enabled = mkEnableOption "example service";
      domain = mkOption { type = types.nullOr types.str; default = null; ... };
      # Use neo.mkReverseProxyOptions { subdomain = "example"; ... }
    };
  };
  default = { };
  description = "Example service configuration";
};
```
See `nix/services/filebrowser/option.nix:9` and `nix/services/hermes/option.nix:142` for complex examples with reverse proxy.

### Types and Options
- `types.port`, `types.str`, `types.nullOr`, `types.listOf`, `types.attrsOf types.str`
- For reverse proxies: `// neo.mkReverseProxyOptions { subdomain = "..."; auth.publicPaths = [...]; }`
- For docker containers (to enable image switching, auto-updater, UI status): `// lib.neo.mkContainerDefinitions { "cname" = "repo:tag"; ... }` (or with extraUnits for additional systemd units); then in default.nix use `cfg.containers."cname"` for the image value instead of hardcoding. Use `// lib.neo.mkSystemdUnits [ "unitname" ]` for pure systemd services.
- Volumes: define in core, reference via `config.neo.core.volumes.*`

### Strings, Scripts, and Activation
Use lib helpers (preferred):
```nix
systemd.services.docker-foo.preStart = lib.concatStringsSep "\n" [
  (lib.neo.mkActivationScriptForDir config { dirPath = "${config.neo.core.volumes.appdata}/foo"; })
  (lib.neo.mkActivationScriptForFile config {
    filePath = "...";
    content = builtins.toJSON { ... };
    mode = "0644";
  })
];
```
See `nix/lib/activation/dir.nix:4` and `file.nix`. Escape with `${escapeShellArg ...}`. Use absolute paths.

### Error Handling and Assertions (Nix)
```nix
assert lib.assertMsg (cfg.port > 1024) "Ports must be > 1024 for non-root";
lib.mkIf cfg.enabled { ... }  # Conditional config only
# Prefer mkIf over if/then in most module contexts
```
Use `lib.neo` helpers for common patterns to avoid duplication.

### Lib Functions (nix/lib/)
```nix
# In nix/lib/activation/dir.nix or similar
neo.mkActivationScriptForDir = config: { dirPath, mode ? "0755", user ? ..., ... }: ''
  if [ ! -e ${dirPath} ]; then
    mkdir -p ${dirPath}
    chown ${user}:${group} ${dirPath}
    chmod ${mode} ${dirPath}
  fi
'';
```
See `nix/lib/authorization.nix` for `authBlock`, `authLocations`, `mkReverseProxyOptions`.
See `nix/lib/containers.nix` for `mkContainerDefinitions`, `mkSystemdUnits`, `getAllContainers` (declare images + units in option.nix for docker services and custom systemd units; enables image config + updater + UI status/logs).

### Volumes and OCI Containers
```nix
neo.core.volumes.appdata = "/var/neo/AppData";
volumes = [
  "${config.neo.core.volumes.appdata}/foo:/config"
  "${config.neo.core.volumes.media}:/srv/Media"
] ++ (lib.mapAttrsToList (h: c: "${config.neo.core.volumes.${h}}:${c}") cfg.additionalMountPoints);
```
Always use `extraOptions = ["--network=internal"]` for internal services. `restartPolicy = "always"`, resource limits where possible.

### Service Modules Example
See `nix/services/filebrowser/default.nix:19`, `nix/services/swag/default.nix:20`, `nix/modules/core/default.nix:43`.

## Rust CLI Code Style (cli/src/)
- **Formatting/Linting**: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`
- **Build/Test**: Integrated in Nix (`nix build .#neo`); `cargo test <test_name>` for single test (add #[test] as needed)
- **Imports**: Grouped - std, extern crates, then `crate::` or `super::`. See `cli/src/main.rs:1-13`
- **Naming**: snake_case for functions/vars, UpperCamelCase for structs/enums (e.g. `Cli`, `Commands`)
- **Error Handling**: `anyhow::{Context, Result}`, extensive use of `?` and `.context("msg")`. No `unwrap()` in lib code. Main uses `if let Err(e) = run()`
- **CLI**: Use clap derive (`#[derive(Parser)]`, `#[command(...)]`). See `cli/src/main.rs:15`, `commands/*.rs`
- **TOML**: Use `toml_edit::DocumentMut` for precise edits without reformatting.
- Keep functions small, prefer `execute_command` helper.

Example from `cli/src/main.rs:118`:
```rust
fn run(cli: Cli) -> Result<()> { ... }
```

## Agentic Coding Guidelines
- **Before editing**: Use glob/grep/read tools extensively to understand patterns. Follow existing conventions strictly (see "Following conventions" in system prompt).
- **When modifying**: 
  1. Read relevant files (options, defaults, lib, similar services).
  2. Mimic style, naming, imports exactly.
  3. Use `lib.neo.*` helpers.
  4. NEVER add comments unless explicitly asked (per code style).
  5. After changes: ALWAYS run `just format && just check` via bash tool.
- **For new services**: Follow "Workflow: Add Service" below. Add to templates if needed.
- **Testing**: Verify with `just check`, VM launch, `just exec 'systemctl status <new-svc>'`. Use nix-developer-agent via Task tool when available for Nix changes.
- **Security**: No committed secrets (use sops-nix). Ports >1024 for non-root. Least privilege. Validate all inputs. Abs paths only.
- **Performance**: Minimal containers, proper volumes, restart=always. Use internal network.
- **NEVER commit** unless user explicitly asks. Run lints first.
- **Cursor/Copilot rules**: None found in .cursor/ or .github/.

## Workflow: Add a New Service
1. Create `nix/services/<name>/option.nix` (with submodule + mkReverseProxyOptions if applicable + mkContainerDefinitions for any oci containers (images) and/or mkSystemdUnits for custom units)
2. Create `nix/services/<name>/default.nix` (activationScripts using lib.neo, oci-container config, mkIf enabled)
3. Create `nix/services/<name>/swag.nix` for nginx proxy/auth if public
4. Update any core modules if new volume needed (`nix/modules/core/*`)
5. `just format && just check`
6. `just build && just launch`
7. Test: `just exec 'systemctl status <name>'`, `just logs <name>`, verify in browser via swag
8. Check VM with `just status`

## Expanded Checklist
- [ ] `just format`
- [ ] `just check` (passes type/syntax/build checks)
- [ ] All options use plain string descriptions
- [ ] Uses `lib.neo` helpers for dirs/files/activation/proxy
- [ ] Follows dendritic: separate option/impl/swag
- [ ] Volumes/mounts use `neo.core.volumes.*` pattern
- [ ] Rust changes (if any): cargo fmt/clippy, Result handling
- [ ] Security: no secrets, proper users, network=internal
- [ ] Tested in VM with `just launch` + `just exec`
- [ ] File <200 lines, follows exact surrounding code style

## Debug Commands
```bash
just status
just logs <svc>
just exec 'journalctl -u <svc> --no-pager -f'
just exec 'systemctl status <svc>'
just exec 'ls -la /var/neo/AppData/<svc>'
nix flake check --print-build-logs
```

## Additional Notes
- VM uses disko for disk, sops-nix for secrets (not committed).
- Templates in `templates/homeserver/` for new deployments.
- CLI commands in `cli/src/commands/*.rs` mirror Nix bootstrap logic.
- When in doubt, grep similar services (`nix/services/*/`) first.
- For agents: prefer Task tool with "nix-developer-agent" for modifications. Verify with lint/check/build before concluding.

This document (~165 lines) equips agentic tools with precise patterns for consistent contributions.
