# AGENTS.md - Development Guidelines for Homeserver Nix Configuration

## Overview
Nix-based homeserver configuration that builds VM configurations for various services using flakes for reproducible builds.

## Build Commands

### Development Build
```bash
just build                        # Build VM configuration
just launch                       # Build and launch VM with QEMU
just ssh                          # SSH into running VM
just exec CMD                     # Execute command in VM
just logs SVC                     # View service logs in VM
```

## Lint and Check Commands

### Nix Code Quality
```bash
nix fmt                          # Format all Nix files
alejandra --check .              # Check formatting without applying changes
alejandra .                      # Apply formatting
just format                      # Apply formatting with alejandra
nix flake check                  # Check for syntax errors
```

## Test Commands

### VM Testing with Just
```bash
just exec "systemctl status <service-name>"   # Test service status
just exec "journalctl -u <service-name>"      # Check service logs
just exec "systemctl list-units --type=service" # List all services
just exec "journalctl --no-pager -n 50"       # View recent system logs
```

## Code Style Guidelines

### Nix Language Conventions

#### File Structure
- Use `.nix` extension for all Nix files
- Separate concerns: `default.nix` for imports, `option.nix` for options
- Keep files under 200 lines when possible
- Directory structure: `services/<name>/default.nix`

#### Naming Conventions
- `camelCase` for variable names: `additionalMountPoints`, `proxyConf`
- `snake_case` for attribute names: `neo.services.filebrowser`
- Function names: `mkActivationScriptForDir`, `mkActivationScriptForFile`

#### Option Definitions
```nix
options.neo.services.example = mkOption {
  type = types.submodule {
    options = {
      enabled = mkEnableOption (mdDoc "Enable the example service");
      port = mkOption {
        type = types.port;
        default = 8080;
        description = mdDoc "Port for the service";
      };
    };
  };
  default = {};
  description = mdDoc "Example service configuration";
};
```

#### Types and Type Safety
```nix
port = mkOption {
  type = types.port;
  default = 8080;
};

subdomain = mkOption {
  type = types.nullOr types.str;
  default = null;
};
```

#### String Handling
```nix
userId = toString config.users.users.${cfg.user}.uid;

script = ''
  mkdir -p ${escapeShellArg dirPath}
  chown ${user}:${group} ${escapeShellArg dirPath}
'';
```

#### Error Handling
```nix
assert assertMsg (cfg.port > 1024) "Port must be > 1024 for non-root";

(mkIf cfg.enabled {
  # Service configuration
})
```

#### Library Functions (`lib.nix`)
```nix
mkActivationScriptForDir = {
  dirPath,
  mode ? "0755",
  user ? "root",
  group ? "root",
} @ args:
assert lib.assertMsg (dirOf dirPath == "/") "dirPath must be absolute";
# Function body
```

#### Configuration Patterns

##### Service Modules
```nix
{ config, lib, ... }:

with lib;

let
  cfg = config.neo.services.serviceName;
in {
  imports = [
    ./option.nix
    ./swag.nix
  ];

  system.activationScripts.create-dirs = mkActivationScriptForDir {
    dirPath = "${config.neo.volumes.appdata}/service";
    user = "1000";
    group = "1000";
  };
}
// (mkIf cfg.enabled {
  virtualisation.oci-containers.containers.serviceName = {
    # Container config
  };
})
```

##### Volume Management
```nix
neo.volumes = {
  appdata = "/path/to/appdata";
  media = "/path/to/media";
};

volumes = [
  "${config.neo.volumes.appdata}/service:/config"
  "${config.neo.volumes.media}:/srv/Media"
];
```

## Development Workflow

### Adding a New Service
1. Create `services/<name>/` directory
2. Add `option.nix` for configuration options
3. Add `default.nix` for service implementation
4. Add `swag.nix` for reverse proxy configuration (if needed)
5. Import in `services/default.nix`
6. Test with `nix flake check`
7. Build and deploy with `just build && just launch`

### Code Review Checklist
- [ ] `nix flake check` passes
- [ ] `nix fmt` applied to all files
- [ ] Options have proper types and descriptions
- [ ] Activation scripts use library functions
- [ ] Volume mounts follow established patterns
- [ ] Service follows naming conventions

### Debugging
```bash
just logs <service-name>          # View service logs in VM
just exec "journalctl -u <service-name> --no-pager"  # Detailed service logs
just exec "systemctl status <service-name>"         # Check service status
just exec "journalctl --since '1 hour ago'"         # Recent system logs
```

## Security Considerations

- Never commit secrets or API keys
- Use environment variables for sensitive configuration
- Follow principle of least privilege for user IDs
- Validate all inputs in activation scripts
- Use absolute paths for volume mounts

## Performance Guidelines

- Keep VM configurations minimal
- Use appropriate service restart policies
- Configure resource limits where needed
- Optimize volume mounts for I/O patterns
- Use network isolation appropriately