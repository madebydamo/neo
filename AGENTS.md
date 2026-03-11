# AGENTS.md - Development Guidelines for Neo Homeserver Nix Configuration (Dendritic Pattern)

## Overview
Nix-based homeserver configuration building QEMU VMs for services using flakes. Follows the [Dendritic pattern](https://github.com/mightyiam/dendritic): flake-parts top-level Nixpkgs module system with `import-tree` auto-importing all non-entrypoint Nix files from `./nix/` as modules. Each file implements a single feature across configs (e.g., `nix/services/&lt;name&gt;/*.nix`).

Example pattern from dendritic/example:
- `flake.nix`: flake-parts + import-tree `./modules`
- Modules dir: auto-imported Nix files as top-level modules
- Benefits: Known type per file, automatic import, path independence

## Build Commands (via justfile)
```bash
just build     # nix build .#nixosConfigurations.homeserver.config.system.build.vm
just launch    # Build + QEMU launch (4CPUs/8G RAM)
just ssh       # SSH to VM
just exec CMD  # Run CMD in VM
just logs SVC  # journalctl -u SVC
just status    # VM/QEMU/SSH status
just shutdown  # Graceful VM shutdown
```

## Lint and Check
```bash
just format  # alejandra .
just check   # nix flake check (syntax/types)
```

## Test Commands
No unit tests. Use:
```bash
nix flake check  # Full typecheck/lint
just exec 'systemctl status &lt;svc&gt;'  # Single service status (e.g., filebrowser)
just exec 'journalctl -u &lt;svc&gt;'     # Single service logs
just exec 'systemctl list-units --type=service'  # All services
just logs &lt;svc&gt;  # Tail service logs
```

## Code Style Guidelines (Nix)

### File Structure (Dendritic)
- `.nix` for all files
- `nix/services/&lt;name&gt;/`: `default.nix` (impl), `option.nix` (opts), `swag.nix` (proxy)
- Keep &lt;200 lines/file
- Auto-imported via import-tree

### Naming
- Vars: camelCase (`additionalMountPoints`)
- Attrs: snake_case (`neo.services.filebrowser`)
- Fns: camelCase (`mkActivationScriptForDir`)
- Opts: `mkEnableOption (mdDoc &quot;...&quot;)`

### Option Definitions
```nix
options.neo.services.example = mkOption {
  type = types.submodule ({...}: { options = { enabled = mkEnableOption (mdDoc &quot;...&quot;); }; });
  default = { };
  description = mdDoc &quot;...&quot;;
};
```

### Types
```nix
port = mkOption { type = types.port; default = 8080; };
subdomain = mkOption { type = types.nullOr types.str; default = null; };
```

### Strings/Scripts
```nix
userId = toString config.users.users.${cfg.user}.uid;
script = ''
  mkdir -p ${escapeShellArg dirPath}
  chown ${user}:${group} ${escapeShellArg dirPath}
'';
```

### Errors
```nix
assert lib.assertMsg (cfg.port &gt; 1024) &quot;Port &gt;1024&quot;;
mkIf cfg.enabled { ... }
```

### Lib Fns (lib.nix)
```nix
mkActivationScriptForDir = { dirPath, mode ? &quot;0755&quot;, ... } @ args:
  assert lib.assertMsg (dirOf dirPath == &quot;/&quot;) &quot;Absolute path&quot;;
  # impl
```

### Service Modules
```nix
{ config, lib }: let cfg = config.neo.services.foo; in {
  imports = [ ./option.nix ./swag.nix ];
  system.activationScripts.foo-dirs = mkActivationScriptForDir { ... };
} // mkIf cfg.enabled {
  virtualisation.oci-containers.containers.foo = { ... };
}
```

### Volumes
```nix
neo.volumes.appdata = &quot;/path/appdata&quot;;
volumes = [ &quot;${config.neo.volumes.appdata}/foo:/config&quot; ];
```

## Workflow: Add Service
1. `nix/services/&lt;name&gt;/option.nix`
2. `default.nix` (above pattern)
3. `swag.nix` if proxy
4. `just format && just check`
5. `just build && just launch`
6. Test: `just exec 'systemctl status &lt;name&gt;'`

## Checklist
- [ ] `just check`
- [ ] `just format`
- [ ] Typed/descr opts
- [ ] Lib fns for scripts
- [ ] Est. patterns (vols, mounts)

## Debug
```bash
just logs &lt;svc&gt;
just exec 'journalctl -u &lt;svc&gt; --no-pager'
just exec 'systemctl status &lt;svc&gt;'
```

## Security/Perf
- No secrets committed (sops-nix)
- Least priv users/ports &gt;1024
- Minimal VM, restart=always, limits
- Abs paths, validated inputs