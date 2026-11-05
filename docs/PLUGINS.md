# Plugins

Plugins add things to Neo without forking it: more apps, your own projects, extra system configuration. A plugin is a Nix flake. Neo imports it into your homeserver, and its services get the same HTTPS, login, updates, status page and backups as the built-in ones.

This page is for people who **use** plugins. Writing one: [Plugin development](PLUGIN-DEVELOPMENT.md), with the full helper reference in [`lib.neo` reference](LIB.md).

## Contents


- [Awesome plugins](#awesome-plugins)
- [Install a plugin](#install-a-plugin)
- [Private plugins](#private-plugins)
- [Local plugins](#local-plugins)
- [After installing](#after-installing)
- [Change the address or branch](#change-the-address-or-branch)
- [Add the same plugin twice](#add-the-same-plugin-twice)
- [Updating plugins](#updating-plugins)
- [Removing a plugin](#removing-a-plugin)


---

## Awesome plugins

| Plugin | What it adds |
|--------|--------------|
| **[highsea.neo](https://github.com/madebydamo/highsea.neo)** | Media stack: Jellyfin, Sonarr, Radarr, Lidarr, Prowlarr, qBittorrent, Bazarr, Tdarr, Seerr, FlareSolverr, Audiobookshelf and more, auto-wired with declarr |
| **[portrait](https://github.com/madebydamo/portrait)** | A personal site built and packaged by its own flake, served behind Neo's HTTPS |

Know a good plugin? Open an issue or PR on Neo to get it listed.

## Install a plugin

1. Open the Neo web UI.
2. Go to **Settings → System**. The **Plugins** list is in the General section.
3. Type the flake URL into the field and click **+ Add plugin**. Leave the prefix field empty:

   ```text
   github:madebydamo/highsea.neo
   ```

   A branch goes at the end for `github:` / `gitlab:` URLs (`github:madebydamo/portrait/dev`) and as `?ref=dev` for `git+…` URLs.

4. Click **Save**, then **Activate** in the top bar. Neo fetches the plugin and rebuilds the system.
5. Open the **Services** tab. The plugin's services are listed next to the built-in ones. Enable the ones you want, Save, Activate.

After the first activation, the plugin card lists the services it adds. Before that it says "No services attributed to this plugin in the current eval".

Plugins are stored in `settings.toml`, one table per plugin, so you can also edit them there. The name after `core.plugins.` is the plugin's id; Neo picks it from the repository name when you add it in the web UI.

```toml
[core.plugins.highsea-neo]
url = "github:madebydamo/highsea.neo"
```

Configs from before Neo had ids (`plugins = [ "github:…", … ]`) still work. `neo update` turns the list into tables once.

## Private plugins

The homeserver fetches plugins with the machine git identity (**Settings → System → Git**, `[core.git]` in `settings.toml`). Use one of these:

- **SSH key.** Copy the **Git SSH public key** from the Git section into your forge, as an account SSH key or a read-only deploy key on the plugin repo. Then use an SSH URL:

  ```text
  git+ssh://git@github.com/you/my-plugin
  ```

  github.com and gitlab.com are trusted out of the box. For any other forge, add its host key to `core.git.knownHosts` first: the value is the output of `ssh-keyscan -t ed25519 git.example.com` without the leading host name. Use `git.example.com:2222` as the key for a non-standard port. **Activate the new host key before adding the plugin**: the build fetches plugins before the new key is installed.

- **Access token.** Add a token for the host and keep the short URL:

  ```toml
  [core.git.tokens]
  "github.com" = "github_pat_…"   # fine-grained, read-only contents on the plugin repos
  ```

  ```text
  github:you/my-plugin
  ```

  The same token works for `git+https://` URLs. GitLab tokens need the `PAT:` prefix for `gitlab:` URLs.

Only the build user (`homeserver`) and Hermes (when enabled) can read these credentials. Root does not need them: the switch step reuses inputs the build already fetched.

## Local plugins

A plugin on the homeserver's disk (for development, or code you do not want to publish):

```text
git+file:/home/you/projects/my-plugin
```

or

```text
path:/home/you/projects/my-plugin
```

With `git+file:` Nix only sees files tracked by git, so `git add` new files. `path:` copies the whole directory. The path has to exist on the machine that builds the config.

You can install several plugins. Their order does not matter.

## After installing

- Plugin services show a badge with the plugin's id on the **Services** grid. The source filter offers **All sources**, **Core**, and one entry per plugin.
- Their options open in the same form as built-in services, including HTTPS, login, "Reachable from", the AI assistant and container images.
- HTTPS and login behave like the rest of Neo when the plugin uses Neo's reverse-proxy helpers (all listed plugins do).

## Change the address or branch

Click **Edit** on the plugin card. Change the **Address**, or type another **Branch** (empty means the repository's default branch), click **Done**, then **Save** and **Activate**.

The plugin keeps its id, so only its `url` line in `settings.toml` and its own entry in `flake.lock` change, and its settings stay. If the new branch no longer has a service you had set up, the build fails on the leftover `[services.<name>]` table: delete that table, or switch back.

## Add the same plugin twice

To run, for example, a `dev` branch next to `main`, add the plugin again with a **prefix**:

1. Type the second URL (`github:madebydamo/portrait/dev`). Neo notices the plugin is already there and fills the prefix field with the branch name.
2. Click **+ Add plugin**, **Save**, **Activate**.

```toml
[core.plugins.portrait]
url = "github:madebydamo/portrait"

[core.plugins.portrait-dev]
url = "github:madebydamo/portrait/dev"
prefix = "dev"
```

Every app of the second copy gets the prefix in front of its name: `dev-portrait-server` in the Services grid and `settings.toml`, its own containers, data folder and the subdomain `dev-portrait`. The prefix uses lowercase letters and digits and starts with a letter. It cannot be changed later, because it is part of every name: remove the copy and add it again instead.

Only plugins written for this can be added twice (see [Prefixes](PLUGIN-DEVELOPMENT.md#prefixes-add-a-plugin-twice)). Others fail to build with "does not support a prefix"; they still work once without a prefix.

## Updating plugins

Plugins are flake inputs of your config (`plugin-<id>`), pinned in its `flake.lock`. **Update** in the top bar (or `neo update` on the command line) refreshes all inputs, Neo and plugins alike. Then **Activate**. The scheduled system updater does the same when it is enabled.

## Removing a plugin

1. Go to **Settings → System → Plugins**.
2. Click **Remove** on the plugin card.
3. Neo shows the `[services.<name>]` tables it will delete from `settings.toml` because only this plugin declared them. For a prefixed copy that is only its own `dev-…` tables. Confirm with **Remove plugin**. Appdata directories are kept.
4. **Save**, then **Activate**.

Do not delete a plugin from `settings.toml` by hand without also removing its service tables. Leftover keys for options that no longer exist make Nix evaluation fail. The web UI does both in one save.

## Write your own

Start with [Plugin development](PLUGIN-DEVELOPMENT.md).
