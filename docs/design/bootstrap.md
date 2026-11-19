# Bootstrap a new server (`neo bootstrap`)

Goal: from a laptop, one guided command takes a fresh machine (a hoster's Linux image or a NixOS installer) to a running Neo whose web UI answers at `https://neo.<domain>`. Everything after that is done in the web UI.

```bash
nix run github:madebydamo/neo#neo -- bootstrap ./myserver
```

## Flow

1. **Server**: `[user@]IP[:port]` and the SSH key (default `~/.ssh/id_ed25519`). The user is `root` or one with sudo, e.g. `homeserver@` on a machine that already runs Neo (see Non-root login). A probe over SSH shows the OS, whether it is a NixOS installer, its disks (by-id names, without floppies, RAM disks and anything below 1 GB) and its global addresses.
2. **Name and domain**: hostname (`core.hostname`), domain and certificate email (`services.swag`).
3. **Logins**: the key's `.pub` joins `core.ssh.authorizedKeys`; a web login (`services.tinyauth.users`, bcrypt cost 10 via `mkpasswd`); optional Linux password (`core.hashedLinuxPassword`, sha-512).
4. **Disk**: `disko.enabled = true`, `disko.mainDisk` from the probe. Every other disk of the server is offered for `disko.additionalDisks`: not used, Media files (`<root>/DATA/Media`), all Neo data (`volumes.root`) or another mount point; a used one is erased into its own pool. Then automatic snapshots on or off (`disko.autoSnapshot.enabled`).
5. **Reachable from the internet**: suggested from the probe (a public address on an interface). If not, the **streamproxy edge**: an existing Neo server with a public address, reached as `homeserver@…`. `ssh://` and ssh_config aliases work (`ssh://neohq`). The wizard reads its tunnels, asks for the tunnel name (default the hostname; a name the edge uses for another domain is refused), keeps or generates the token and asks for the edge's **public address**: the SSH name is often private (an alias, NetBird, Tailscale), so the default is the edge's first public address on its interfaces. The new server gets `services.rathole` (`remoteAddr`, `name`, `token`). A more specific domain wins over another tunnel's wildcard (`dev.example.com` over `*.example.com`), as streamproxy sorts the more specific name first.

Then:

- **Repo**: the answers are written over the existing settings (other keys stay) and `init_repo` (the `neo init` steps, without the hardware config) creates or updates the repo in `DIR` and commits.
- **Optional web step**: `neo --config-path DIR --profile local web` on this computer; Enter stops it and its saves are committed. Settings are re-read afterwards, so a domain or tunnel changed there is what the next steps use.
- **Edge**: `services.streamproxy.enabled` and `entries.<name> = { url = domain, token, wildcard, includeTopLevel }` are written with `neo settings patch` (JSON on stdin, so the token is never in argv), then `neo activate` runs on the edge. When the edge already has that tunnel for the same domain, it asks before writing and activating again.
- **DNS check**: `<domain>` and `neo.<domain>` must resolve to the server's public addresses or, with a tunnel, to the edge's public addresses (read on the edge, plus what `remoteAddr` resolves to), never to the SSH name. It only warns.
- **Install**: with a non-root login, the installer is started first (Non-root login). Then `neo restore DIR --host root@… --data none --origin "" --reboot` (`restore::remote::run`): kexec into an installer when needed, hardware config on the target, Disko, `nixos-install`. Its own check and "yes" come before the disk is erased.
- **First boot**: waits for `homeserver@` over SSH, then for `https://neo.<domain>/` to answer 200/302/401 (curl pinned to the expected address with `--resolve`), and prints the URL, the login and that `DIR` may be deleted. It is never deleted automatically, so a run that breaks halfway can simply be repeated.

## Idempotency

`DIR` is the state. A rerun loads `DIR/settings.toml` and asks every question again with the earlier answer as the default (Enter keeps it). Answers that are no settings (target, key, edge, web user name) live in `DIR/.git/neo-bootstrap.json`, outside the repo's files. Flags (`--host`, `--domain`, `--disk`, `--public`, `--edge`, …) answer a question without asking; `--defaults` asks nothing and fails when an answer is missing (a new web login password can only be typed).

## Non-root login

nixos-anywhere needs root or sudo without a password, and Neo has neither (`PermitRootLogin = no`; `homeserver` and `admin` are in wheel with `core.hashedLinuxPassword`). For another user bootstrap boots the installer itself over `ssh -t`: it downloads the same kexec tarball nixos-anywhere uses and runs `sudo …/kexec/run`, so sudo asks for the password on your terminal. The installer takes the authorized keys of root and of the sudo user (`$SUDO_USER`, including `/etc/ssh/authorized_keys.d/<user>`) and the host keys, so `root@` logs in with your key afterwards and the restore goes on as usual.

## Edge access

The edge may run an older Neo without `neo settings`. Bootstrap copies only this neo's unwrapped binary to the edge (`nix copy --substitute-on-destination`, a few paths; `homeserver` is a Nix trusted user) and runs it through the edge's own neo wrapper with the exec line swapped, so it gets that install's environment (sops binary, defaults). Activation uses the edge's own `neo activate`. If the copy fails it falls back to the edge's `neo`.

## `neo settings get|patch`

The CLI form of `web::settings::edit::apply_ops`: `get <dotted.path>` prints JSON from `settings.toml`; `patch` reads `[{"set": {"path": [...], "value": …}}, {"unset": {"path": [...], "secret": bool}}]` from stdin. Secret values move into `secrets.yaml` or `secrets.sops.yaml` when the config keeps one (secret paths from the same evaluation as `neo secrets split`), and the secrets are re-encrypted when the SSH keys changed. It takes the shared system lock, like a web UI save.

## Not covered

- A fresh VPS as an edge is a normal bootstrap with a public address; turn on streamproxy there afterwards in the web UI.
- Encrypting secrets, a git remote for the config and backups stay next steps in the web UI.
- The server follows `neo-cli.server.neoInput` (`--server-neo-input`, default `github:madebydamo/neo`).
