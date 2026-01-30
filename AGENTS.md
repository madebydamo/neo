# AGENTS.md - Development Guidelines for Homeserver Nix Configuration

## Overview
This repository contains a Nix-based homeserver configuration that builds Docker containers for running various services (filebrowser, rathole, swag, etc.). The codebase follows NixOS module patterns and uses flakes for reproducible builds.

## Build Commands

### Full Build and Deploy
```bash
# Build the Nix flake and create Docker image
./build-and-load.sh

# Alternative: Build only (creates result/tarball/nixos-system-x86_64-linux.tar.xz)
nix build

# Load built image into Docker and start services
docker compose up -d --remove-orphans
```

### Development Build
```bash
# Quick rebuild during development (note: may timeout, use with --max-jobs 1 if needed)
nix build --rebuild

# Build for VM testing (non-Docker) - fails due to missing boot/grub config
nix build .#nixosConfigurations.homeserver.config.system.build.vm
```

### Development Build
```bash
# Quick rebuild during development
nix build --rebuild

# Build for VM testing (non-Docker)
nix build .#nixosConfigurations.homeserver.config.system.build.vm
```

## Lint and Check Commands

### Nix Code Quality
```bash
# Format all Nix files
nix fmt

# Check formatting without applying changes
alejandra --check .

# Apply formatting
alejandra .

# Check for syntax errors and dead code
nix flake check

# Note: statix and deadnix are not installed in this environment
# Install with: nix shell nixpkgs#statix nixpkgs#deadnix
# Then run: statix check
# Or: deadnix
```

### Docker and Compose Validation
```bash
# Validate docker-compose.yaml syntax
docker compose config

# Check Docker image build without building
docker build --dry-run .

# Note: dockerfilelint is not installed in this environment
```

## Test Commands

### Integration Testing
```bash
# Test full deployment
./build-and-load.sh && docker compose ps

# Test individual services
docker compose up -d <service-name>
docker compose logs <service-name>

# Health check services
curl -f http://localhost:<port>/health || echo "Service unhealthy"
```

### Nix Flake Testing
```bash
# Test flake evaluation
nix flake metadata

# Check all outputs build successfully (Docker package only)
nix build .#all

# Quick evaluation check for Docker package (faster than full build)
nix build .#packages.x86_64-linux.homeserver --dry-run

# Test VM configuration (fails due to missing boot/grub config)
nix build .#nixosConfigurations.homeserver.config.system.build.vm

# Note: nix flake check fails on VM configuration due to missing fileSystems
# and boot.loader configuration. Docker builds work fine.
```

## Code Style Guidelines

### Nix Language Conventions

#### File Structure
- Use `.nix` extension for all Nix files
- Separate concerns: `default.nix` for imports, `option.nix` for options, service-specific files for implementation
- Keep files under 200 lines when possible
- Use consistent directory structure: `services/<name>/default.nix`

#### Imports and Modules
```nix
# Good: Clear, organized imports
{ config, lib, ... }:

with lib;

let
  cfg = config.neo.services.example;
in {
  # Module definition
}

# Avoid: Unnecessary with statements
{ config, lib, ... }:
with lib;
with builtins;  # Don't do this
```

#### Option Definitions
```nix
# Good: Descriptive options with proper types
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

#### Naming Conventions

##### Variables and Functions
- Use `camelCase` for variable names: `additionalMountPoints`, `proxyConf`
- Use `snake_case` for attribute names in option paths: `neo.services.filebrowser`
- Function names: `mkActivationScriptForDir`, `mkActivationScriptForFile`
- Configuration attributes: `system.stateVersion`, `virtualisation.oci-containers`

##### Module Structure
```nix
# Good: Consistent module naming
neo.services.filebrowser
neo.services.rathole
neo.volumes.appdata

# Avoid: Inconsistent naming
neo.fileBrowser  # inconsistent casing
services.fb      # unclear abbreviation
```

#### Types and Type Safety
```nix
# Good: Explicit types with defaults
port = mkOption {
  type = types.port;
  default = 8080;
};

# Good: Nullable types when appropriate
subdomain = mkOption {
  type = types.nullOr types.str;
  default = null;
};

# Avoid: Generic types without constraints
value = mkOption {
  type = types.anything;  # Too permissive
};
```

#### String Handling
```nix
# Good: Use lib functions for string operations
userId = toString config.users.users.${cfg.user}.uid;

# Good: Proper escaping in scripts
script = ''
  mkdir -p ${escapeShellArg dirPath}
  chown ${user}:${group} ${escapeShellArg dirPath}
'';

# Avoid: Manual string concatenation
path = config.neo.volumes.appdata + "/service";  # Error-prone
```

#### Error Handling
```nix
# Good: Use assertions for validation
assert assertMsg (cfg.port > 1024) "Port must be > 1024 for non-root";

# Good: Use mkIf for conditional configuration
(mkIf cfg.enabled {
  # Service configuration
})

# Good: Proper error messages
proxyConf = mkOption {
  type = types.nullOr types.str;
  default = null;
  description = mdDoc ''
    Nginx proxy configuration. If null, default configuration is used.
    Example: "proxy_pass http://localhost:8080;"
  '';
};
```

#### Library Functions (`lib.nix`)

##### Function Signatures
```nix
# Good: Documented functions with defaults
mkActivationScriptForDir = {
  dirPath,
  mode ? "0755",
  user ? "root",
  group ? "root",
} @ args:
assert lib.assertMsg (dirOf dirPath == "/") "dirPath must be absolute";
# Function body
```

##### Attribute Sets
```nix
# Good: Consistent attribute naming
{
  mkActivationScriptForFile = { ... };
  mkActivationScriptForDir = { ... };
}

# Avoid: Mixed naming conventions
{
  mkFileScript = { ... };  # Inconsistent
  dirScript = { ... };     # Inconsistent
}
```

#### Configuration Patterns

##### Service Modules
```nix
# Good: Consistent service structure
{ config, lib, ... }:

with lib;

let
  cfg = config.neo.services.serviceName;
in {
  imports = [
    ./option.nix
    ./swag.nix
  ];

  # Activation scripts
  system.activationScripts.create-dirs = mkActivationScriptForDir {
    dirPath = "${config.neo.volumes.appdata}/service";
    user = "1000";
    group = "1000";
  };
}
// (mkIf cfg.enabled {
  # Service configuration
  virtualisation.oci-containers.containers.serviceName = {
    # Container config
  };
})
```

##### Volume Management
```nix
# Good: Consistent volume definitions
neo.volumes = {
  appdata = "/path/to/appdata";
  media = "/path/to/media";
  documents = "/path/to/documents";
};

# Good: Volume mounting pattern
volumes = [
  "${config.neo.volumes.appdata}/service:/config"
  "${config.neo.volumes.media}:/srv/Media"
];
```

#### Docker Compose Integration

##### Service Definition
```yaml
# Good: Consistent service naming and structure
version: "3.8"

services:
  homeserver:
    image: homeserver:latest
    privileged: true
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ./DATA:/data
    restart: unless-stopped
```

##### Environment Variables
```nix
# Good: Environment variable naming
environment = {
  TZ = "Europe/Zurich";
  PUID = "1000";
  PGID = "1000";
};
```

## Development Workflow

### Adding a New Service
1. Create `services/<name>/` directory
2. Add `option.nix` for configuration options
3. Add `default.nix` for service implementation
4. Add `swag.nix` for reverse proxy configuration (if needed)
5. Import in `services/default.nix`
6. Test with `nix flake check`
7. Build and deploy with `./build-and-load.sh`

### Code Review Checklist
- [ ] `nix flake check` passes
- [ ] `nix fmt` applied to all files
- [ ] Options have proper types and descriptions
- [ ] Activation scripts use library functions
- [ ] Volume mounts follow established patterns
- [ ] Service follows naming conventions
- [ ] Documentation updated if needed

### Debugging
```bash
# Check service logs
docker compose logs <service-name>

# Inspect container
docker compose exec <service-name> /bin/sh

# Test Nix evaluation
nix eval .#packages.x86_64-linux.homeserver

# Debug VM
nix build .#nixosConfigurations.homeserver.config.system.build.vm
result/bin/run-nixos-vm
```

## Current Verifier Status

### Working Verifiers
- **alejandra**: Available and working. Use `alejandra --check .` to check formatting, `alejandra .` to apply formatting
- **nix flake check**: Partially working. Checks Docker configuration successfully, but fails on VM configuration due to missing boot/grub settings
- **docker compose config**: Working. Validates docker-compose.yaml syntax (shows warning about obsolete version field)
- **nix flake metadata**: Working. Shows flake information and inputs

### Unavailable Verifiers
- **statix**: Not installed. Install with `nix shell nixpkgs#statix` then run `statix check`
- **deadnix**: Not installed. Install with `nix shell nixpkgs#deadnix` then run `deadnix`
- **dockerfilelint**: Not installed in this environment

### Current Issues
- 13 Nix files require formatting with alejandra (flake.nix now formatted)
- VM configuration fails flake check due to missing boot/grub configuration
- nix fmt now works with alejandra formatter

## Security Considerations

- Never commit secrets or API keys
- Use environment variables for sensitive configuration
- Follow principle of least privilege for user IDs
- Validate all inputs in activation scripts
- Use absolute paths for volume mounts

## Performance Guidelines

- Keep Docker images minimal
- Use appropriate restart policies
- Configure resource limits where needed
- Optimize volume mounts for I/O patterns
- Use network isolation appropriately