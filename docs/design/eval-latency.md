# Web UI eval latency: serve stale, evaluate in the background

Status: implemented (steps 0–5 of §9; §8.2 dropped). §11 lists where the
implementation differs from this plan. Scope: `cli/src/commands/web/nix/repl/`, `schema_cache.rs`,
`settings/save.rs`, the WS push channel, the pages that render eval results, and
one template change (`templates/homeserver/modules/settings.nix`).

## 1. The problem today

Every eval-backed page (`/configuration` grid, option panes, guides, privacy map,
`/agent/refs`, `/nav-services` without the build-time file) goes through **one**
`Arc<Mutex<NixEvaluator>>`, which wraps **one** `nix repl` process with `f` (the
config flake) bound in it.

A settings save calls `refresh_after_settings_change` (`settings/save.rs`), which
spawns a task that takes that mutex and runs `NixEvaluator::refresh()`: kill the
repl, spawn a new one, `getFlake` / CLI installable, re-import the extractors. Every
page that needs the evaluator queues behind that mutex until the new flake is
evaluated and the first query (`extract_services` walks all service options) is
done. That is the wait after each value change, even though for a change like
`services.immich.subdomain = "photos"` the grid, the other panes, the navigator and
the theme come out the same.

The same full restart also happens:

- lazily in `auto_refresh_if_stale` when the content fingerprint of the config
  dir's `.nix` / `.toml` / `.lock` files changed (CLI edit, `neo update`, git
  restore, the updater timer);
- in `restart_after_failure` when `repl_needs_restart` decides a failed query
  broke the repl (see §6 for why that fires more often than it should);
- after `nix_repair` and settings restore.

## 2. Goal

- A value change in one service never blocks pages: you can edit Immich, go back
  to the grid and edit Nextcloud right away.
- The page you just saved shows your new values immediately (read-your-writes),
  without waiting for Nix.
- When the new evaluation finishes, anything that really changed (a default that
  depends on another setting, a newly enabled service's units, the grid, the
  navigator) updates in place, pushed over the existing WS.
- Structural changes (plugins, `flake.lock`, `.nix` files, neo update) may still
  wait; that is acceptable.
- Restarting the repl becomes rare and every restart logs why.

## 3. Design overview

Three pieces, each useful on its own, in the order to build them:

1. **Generations + result cache** (§4): every bound flake is a numbered
   *generation*; eval results are cached per (generation, query). Pages are
   served from the newest *ready* generation, never from one still evaluating.
2. **Settings overlay** (§5): when the ready generation is older than
   `settings.toml`, patch the values that changed since that generation directly
   from the TOML, so the edited pane is right immediately.
3. **Background evaluation + push** (§4.3, §7): a second evaluator builds the next
   generation; when it is ready, the cached queries are replayed on it, diffed,
   the generation swaps, and keys whose result differs are pushed to clients that
   show them.

Then, independently, **make each evaluation cheaper** (§8) and **stop the repl
from going cold** (§6).

## 4. Generations and the evaluator pool

### 4.1 Types

```rust
struct Generation {
    id: u64,                       // monotonic
    inputs: InputsSnapshot,        // fingerprint + parsed settings.toml/secrets at bind time
    kind: ChangeKind,              // Values | Structural (vs previous generation)
    cache: DashMap<QueryKey, Arc<CachedResult>>,  // or Mutex<HashMap>
    error: Option<EvalErrorUi>,    // set when binding/first query failed
}

enum QueryKey {
    Services, Theme, Navigator, Guides, Privacy, OptionIndex, SecretPaths, PluginOwners,
    Pane { core: bool, name: String },
}

struct EvalPool {
    serving: ArcSwap<Generation> + the repl that evaluated it (Mutex<NixEvaluator>),
    builder: Option<BuildTask>,    // at most one
    target: watch::Sender<InputsSnapshot>,  // newest inputs on disk
}
```

`QueryKey` replaces the ad-hoc `ev.extract_*()` calls in routes: each route asks
`pool.get(QueryKey::Pane{..})`, which returns `(Arc<CachedResult>, gen_id,
provisional)`. `CachedResult` holds the raw JSON (for diffing) plus the already
enriched context (what `extract_pane` builds today). `SchemaCache` becomes a view
on the serving generation's pane entries and goes away as a separate cache.

### 4.2 Serving

`get(key)`:

1. Hit in `serving.cache` → return it (no Nix). This alone makes grid ↔ pane
   navigation instant on an unchanged config; today every grid visit re-runs
   `extract_services`.
2. Miss → evaluate on the serving repl (the mutex is only held for this one
   query), store, return.
3. If the key is *structural* (`Services`, `Navigator`, `PluginOwners`,
   the `plugins` core pane) and a `Structural` build is pending, wait for the
   builder instead (the case the user accepts waiting for).

`provisional = serving.inputs != target` (the disk moved on since this
generation was bound). Provisional responses get the overlay (§5) and a response
header `X-Neo-Eval-Gen: <id>; provisional`.

### 4.3 Building the next generation

Triggered by `refresh_after_settings_change` (instead of killing the repl) and by
the fingerprint check that `auto_refresh_if_stale` does today (moved out of the
query path into a cheap watcher, e.g. the 750 ms action-bar loop already stat()s
the repo).

1. Classify the diff between `serving.inputs` and the disk:
   - `Values`: only `settings.toml` / `secrets.*` changed, and no structural key
     (`core.plugins`, anything else that changes flake inputs or which modules
     load) changed.
   - `Structural`: `flake.lock`, any `.nix`, `core.plugins`, `NEO_FLAKE_PATH`.
2. Start (or restart) the builder on the new inputs. Debounce ~300 ms so a burst of
   saves is one build. If inputs move again while a build runs: for `Values`,
   interrupt it (SIGINT makes `nix repl` abandon the current evaluation and return
   to its prompt; verify) and rebind; for `Structural`, let it finish and chain.
   Cap restarts so constant editing still converges (e.g. never interrupt a build
   older than 20 s).
3. When the builder's `f` is bound: replay the keys in `serving.cache`, most
   recently used first (all of them; there are tens, not thousands), into the new
   generation's cache.
4. Swap `serving` to the new generation, publish `eval_generation` (§7) with the
   keys whose JSON differs. The old repl becomes the next builder (§4.4) or is
   killed.
5. If binding or a replayed query fails with an evaluation error: **do not swap**.
   Keep serving the old generation (+overlay) and push an eval-error banner
   ("Your last change does not evaluate: …", existing `eval_error_actions`
   partial). The UI stays usable with a broken config instead of every page showing
   the error. The next save starts a new build.

### 4.4 Two repls, ping-pong

The repl is single-threaded, so serving the old generation while building the new
one needs two processes. Keep two long-lived ones, **A** serving and **B**
building; after a swap their roles flip. With §8.1 a values change is a rebind
inside the builder's existing process (warm parse cache), not a new process. A
structural change still spawns a fresh builder process.

Memory: two evaluated NixOS configs at once (roughly 1–2 GB each). Make it
configurable (`neo.services.neo` option, e.g. "Fast settings editing", default on
when RAM ≥ 4 GB): off → one repl, it rebuilds in place and serves the result
cache + overlay meanwhile; cache misses wait.

## 5. Settings overlay (read-your-writes)

Every generation keeps the parsed `settings.toml` (+ plain secrets) it was bound
from. For a provisional response:

- `changed = diff(serving.inputs.settings, disk settings)` as a set of TOML paths.
- `Pane{name}`: for each option whose path is in `changed`, set `current` to the
  new TOML value, or to the option's `default` if the key was removed; recompute
  `currentDisplay` and `options_json`. Everything else stays as evaluated.
- `Services`: patch each service's `enabled` from `services.<name>.enabled`.
- Others: served as-is; corrected by the push after the swap.

This covers what the user sees in the common case. Things it cannot know (a
default that depends on another setting, choice providers like `authApps`, units
of a newly enabled service, the "at home" state on the privacy map) are fixed by
the push a few seconds later. `visibleWhen` and `keysFrom` are client-side and
already follow the form values.

The overlay must use the same path mapping the save path uses
(`settings::edit`), including attrsOf entries and nested fields; secrets go
through the existing redaction / `reveal_secrets` path, never into the pushed JSON.

## 6. Keeping the repl warm

What makes it cold today, from the code:

1. **Every save restarts the process** (§1). Fixed by §4/§8.
2. **GC removes what the repl evaluated.** The repl's flake copy and inputs are
   not GC roots. After `nix.gc` (or the updater's collect), a lazily forced thunk
   reads `/nix/store/…-source/flake.nix`, gets `path … does not exist`
   (`MissingStorePath`, which is in `repl_needs_restart`'s restart set; the test
   `detects_missing_store_path_trace` is this exact trace), and the next request
   pays a full cold start. Likely the "went stale overnight" case. Fix: after
   binding `f`, add indirect GC roots for `f.outPath` and every
   `f.inputs.*.outPath` (recursively) under `$STATE_DIRECTORY/gcroots/gen-<id>`;
   drop them when a generation is discarded. Do the same for the icon store paths
   `IconSet` publishes.
3. **Restarts on errors that do not break the repl.** `repl_needs_restart`
   restarts for every message not starting with `nix error:`. That includes our
   own `serde_json::from_value` / "parse marked json" / "missing marker line"
   errors (extractor output shape, not a broken repl) and lock contention
   ("SQLite database is busy" while `nixos-rebuild` or the updater runs). Fix:
   restart only on timeout, closed stdout, `MissingStorePath`; resync with a
   marker for everything else (what eval errors already do).
4. **Lazy staleness on first request.** `auto_refresh_if_stale` runs inside
   `query_json`, so the first page after any external change (CLI edit, `neo
   update`, version restore, updater bumping `flake.lock`) pays the full rebuild
   synchronously. With §4.3 the watcher notices it and builds in the background.
5. **Warm-up warms the wrong things.** `warm_up` forces navigator + theme; the
   first `/configuration` still forces `extract_services` (all options), and the
   chat drawer's first `/agent/refs` forces `extract_option_index` (every option of
   every service). Warm-up should fill the generation cache with `Services`,
   `Theme`, `Navigator`, `Guides`, `Privacy`, then `OptionIndex` last, at low
   priority (a user request jumps the queue: release the mutex between keys).
6. **neo-web restarts** (activation changing the unit, crashes with
   `Restart=always`) start from zero. Lower priority; §8.2 (a persistent result
   cache keyed by input hash) covers it.

Before any of this: **make restarts explainable** (step 0 in §9). Log one line per
restart with the reason (save, fingerprint with the changed file paths, error
kind + first error line, nix_repair, startup) and the time from spawn to
`F_BOUND` / first query. Then check what actually causes the "opening index
triggers a fresh eval" case instead of guessing: on a host `/nav-services` reads
the build-time file and `/` itself evaluates nothing, so the cold eval there comes
from one of 2–5 (or from the agent drawer's `/agent/refs`), and the log will say
which.

Smaller fingerprint fixes while there: `secrets.yaml` / `secrets.sops.yaml` are not
in the fingerprint (only `.nix` / `.toml` / `.lock`), so an external secrets edit
is never noticed; include them.

## 7. Pushing updates to the browser

Uses the existing WS (`routes/ws.rs`, `AppConfig::broadcast`). New message:

```json
{ "type": "eval-generation", "gen": 18, "changed": ["services", "pane:service:immich"], "error": null }
```

Client (`static/eval_sync.js`, loaded after `neo_util.js`):

- Every eval-backed fragment root carries `data-neo-eval-key="pane:service:immich"`
  and `data-neo-eval-gen="17"` (set by the templates from the `get()` result).
- On a message, for each element on the page whose key is in `changed` and whose
  gen is older: re-fetch it with htmx (`htmx.ajax('GET', url, {target, swap})`),
  silently (`data-neo-progress="false"`, no navbar spinner).
- Option pane with unsaved edits (the leave guard knows the form is dirty): do not
  swap; show a small "Updated from the new configuration – reload" note in the
  pane header. Never clobber user input.
- Keys not in `changed`: just bump `data-neo-eval-gen` and drop the provisional
  marker. This is the common case after a value change: nothing visibly moves.
- `error` set: show the eval-error banner from §4.3 step 5.
- Provisional fragments get a quiet marker (e.g. a small dot by the pane title,
  "Checking the new configuration…"), replacing today's navbar spinner, which
  should only spin for blocking (structural) builds. Split `eval_busy` into
  `eval_pending` (background build) and `eval_blocking` in the action bar
  signature.

The diff is on the raw extractor JSON (before enrichment), so `currentDisplay`
reformatting cannot cause spurious updates. Sort keys before comparing, or compare
`serde_json::Value`s.

## 8. Making each evaluation cheaper

### 8.1 Rebind settings instead of reloading the flake

`modules/settings.nix` reads `../settings.toml` from the flake source, so any value
change means a new flake copy in the store and a new `getFlake` (or, on a host, a
new process because the installable is fixed on the command line).

Change the template so the settings can be injected from outside:

```nix
# settings.nix
options.neo._settingsOverride = lib.mkOption { type = lib.types.nullOr lib.types.attrs; default = null; internal = true; };
config.neo = if config.neo._settingsOverride != null then config.neo._settingsOverride else <today's merge>;
```

(Exact shape to work out: `neo` is a submodule tree, so the override probably
needs to live outside `neo.*` to avoid a definition cycle, e.g. a top-level
`_neoSettingsOverride` option.) Then the builder does, inside its existing
process:

```nix
gen18 = f.nixosConfigurations.<host>.extendModules { modules = [ {
  _neoSettingsOverride = mergeSecrets (builtins.fromTOML (builtins.readFile /abs/settings.toml)) …;
} ]; }
```

No store copy, no `getFlake`, no new process; nixpkgs/neo files are already
parsed in that process (Nix's file eval cache), so only the module fixpoint is
redone. Extractors take the configuration instead of `neoFlake = f`.

Needs a template migration (the existing migration mechanism) and a fallback to
today's full reload when an old `settings.nix` lacks the option. Measure first
(step 0): if `getFlake` + parse is a small part of the total, this is less
urgent than §4–§7.

### 8.2 Persistent result cache (optional, later)

Write each generation's cache to `$STATE_DIRECTORY/eval-cache/<inputs-hash>.json`.
After a neo-web restart, serve from it while the repl warms up (provisional until
the first generation is bound). Key on the content hash of the config files plus
`NEO_FLAKE_PATH` plus the neo binary version, so a mismatch never serves old data
as current.

## 9. Plan

0. **Measure and log** (small, do first): restart reasons, spawn → `F_BOUND` →
   first `extract_services` timings, per-query timings. Run on the VM and on a
   host for a day. Decides the weight of §6.2 / §6.3 / §8.1.
1. **Keep warm, cheap fixes** (§6.2 GC roots, §6.3 narrower `repl_needs_restart`,
   §6.5 warm-up set, secrets in fingerprint). Independent, low risk.
2. **Generation + result cache** (§4.1, §4.2) with the single repl: routes go
   through `pool.get(key)`, `SchemaCache` folds into it. Grid/pane navigation on
   an unchanged config becomes instant.
3. **Overlay** (§5) + **non-blocking refresh**: on a save, mark the cache
   provisional and serve it with the overlay while the single repl rebuilds; only
   cache misses wait. Most of the felt win, still one process.
4. **Second repl, replay + diff + push** (§4.3, §4.4, §7): no waiting at all for
   value changes; broken configs no longer break the UI.
5. **Settings rebind** (§8.1) with template migration, if step 0 shows the reload
   cost matters.
6. **Persistent cache** (§8.2), if neo-web restarts are still a visible cost.

Tests: unit tests for change classification, overlay path mapping (scalar,
attrsOf entry field, removed key → default), JSON diff keys, and
`repl_needs_restart`; widget-style JS test for `eval_sync.js` (dirty form not
swapped, unchanged keys only bump the gen). Live check on the VM: save in one
pane, open the grid and a second pane at once (no wait), confirm the push arrives
and an invalid value shows the banner while the UI keeps working.

## 10. Risks and open questions

- **Saves that depend on eval**: `route_secrets` needs `extract_secret_paths`.
  Secret paths are schema, so a stale generation is fine for `Values` changes; a
  `Structural` change (new plugin with secrets) must wait for the builder before
  saving secrets into its options.
- **Apply uses the disk, not the UI's generation**: Save & apply runs
  `nixos-rebuild` on the files, so a provisional UI never applies stale state.
  The diff modal and action bar already read git, not the evaluator.
- **Assistant tools** (`/internal/mcp` reads settings) should read the overlaid
  view too, or explicitly report "evaluating".
- **Memory** of two repls on small hosts (§4.4 switch).
- **SIGINT interrupt** of a running `nix repl` evaluation: verify it returns to
  the prompt reliably across Nix versions; otherwise only chain builds.

## 11. Implementation notes

Code: `cli/src/commands/web/nix/{pool,snapshot,overlay}.rs`, `nix/repl/mod.rs`,
`static/eval_sync.js`, template `modules/settings.nix` (`neoSettingsDir`).

- **No interrupting.** A build is never cancelled; saves within 300 ms become
  one build and later ones chain (a burst of three saves = one generation).
- **Repls are recycled.** `nix repl` never frees an old binding (a re-bound
  name keeps its value reachable), so a repl takes at most 6 generations or
  1.5 GiB RSS, then the next build starts a fresh one (in the idle slot when
  two repls run, so nobody waits).
- **Structural changes swap early.** Only the services list, theme and the
  other structural keys are computed before the swap (what pages wait for);
  the rest of the previous generation's results are replayed afterwards and
  announced in a second push. A structure seen before reuses its repl.
- **GC rooting runs after an evaluation**, not during it (it competed for CPU
  and stretched the first evaluation).
- **Cold start.** The first generation computes the services list and theme
  (the configuration page) before it is served, then the navigator, guides,
  privacy map, secret paths and option index in the background, then starts
  the spare repl so the first settings change is a rebind, not a reload.
- **Two change maps.** The push carries, per query, the generation where its
  result last changed, both raw and compared with the overlaid older result.
  A page rendered provisionally uses the second, so the pane you just saved is
  not fetched again when the evaluation agrees with the overlay.
- **Unparseable settings.toml** turns the overlay off (it would otherwise
  reset every value to its default); the build fails and the banner shows why.
- **GC roots** come from `nix flake archive --dry-run --no-write-lock-file`
  (inputs never fetched are skipped) as `nix-store --add-root` links in
  `$STATE_DIRECTORY/gcroots`, removed with their repl and at startup. Icon
  store paths are not rooted: after a GC one is missing until the next
  evaluation copies it again.
- **Restart rules.** A repl is replaced only after a timeout, a closed pipe or
  a missing store path; config errors, our own JSON parsing, a busy Nix
  database or a failed download leave it in place.
- **Operator switch.** `neo.services.neo.backgroundEval` (default on) →
  `NEO_WEB_EVAL_REPLS=2`; off runs one repl: pages are still served from the
  cache with the overlay, cache misses wait for the build.

Measured in the dev VM (`just launch`, 4 vCPU, guest with the old template
unless noted): first evaluation 8.6 s; all 57 service panes open in a median
of 61 ms (max 154 ms) and go back in at most 106 ms; tabs, core sections and
guides 33–130 ms; after a save every click stays at 50–100 ms while the next
generation is evaluated (≈5 s by reload, 1.8–5 s by rebind with the new
template); five services edited in a row show every saved value on reopening;
a structural change makes the services list wait ≈3.5 s (pane clicks
meanwhile ≈70–230 ms).

The VM run also found slow clicks outside evaluation: every `sudo -n` from
neo-web took ≈1.85 s, because the `/nix/store/*-<pname>-*/bin/…` rules of
`lib.neo.mkSudoCommand` made sudo glob the whole store. `Defaults fast_glob`
(`nix/modules/core/base.nix`) brings it to ≈30 ms; the Versioning tab's
snapshot card went from 3.8 s to 0.29 s, and every unit action, log and
snapshot button that uses sudo gains the same.

Measured on the development machine (fast CPU; `build/` config, ~60 services):
first evaluation 1.2 s, `/configuration` from cache 8 ms, a pane right after a
save 13 ms (provisional, new value shown), next generation by rebind 0.6–1.3 s
including the replay of 7–9 cached queries. A host with a slower CPU scales
the evaluation times, not the page times.
