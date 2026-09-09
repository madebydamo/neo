# Hermes follow-ups

Ideas from comparing Neo’s Hermes module with upstream docs
([configuration](https://hermes-agent.nousresearch.com/docs/user-guide/configuration),
[Nix setup](https://hermes-agent.nousresearch.com/docs/getting-started/nix-setup),
[providers](https://hermes-agent.nousresearch.com/docs/integrations/providers)).
Not scheduled. Pick one and turn it into a normal change.

Done already: one `services.hermes.llm` group (`provider` / `apiKey` / `model` / `baseUrl`)
with a `providerAuth` widget (API key and/or OAuth login + model). Provider ids,
env vars, `auth_type`, and OAuth-capable ids are parsed from the `hermes-agent`
flake input. SuperGrok OAuth (`xai-oauth`) is a Hermes overlay in
`hermes_cli/providers.py` (not a plugin); ChatGPT/Codex is the `openai-codex`
plugin. Neo parses both. Custom endpoints take an optional `llm.apiKey` written
to `model.api_key`.
OAuth status / device-code / PKCE / refresh runs as user `hermes` via
`neo-hermes-auth`.

---

## High

### Nous Portal OAuth polish

Device-code login for `nous` is in the `providerAuth` widget. Remaining: surface Portal Tool Gateway extras (search / image / TTS / browser) after a successful login, and optional `NOUS_API_KEY` as a fallback when OAuth is not used.

### `database.journal_mode = delete`

Hermes docs: WAL is unsafe on some virtiofs / network mounts; existing WAL DBs are not live-downgraded. Directly relevant to the QEMU smoke VM. Pin in `services.hermes-agent.settings.database.journal_mode`.

### Discord / Slack / other messaging

Only Telegram is a Neo option. Upstream messaging extras are already in `extraDependencyGroups` (`messaging`, plus matrix / dingtalk / feishu). Tokens belong in env (same pattern as `TELEGRAM_BOT_TOKEN`).

---

## Medium

### `web.backend`

Separate from the LLM provider. Firecrawl, Brave, xAI search, DuckDuckGo, Exa, Parallel, SearXNG, Tavily, … Env vars like `FIRECRAWL_API_KEY` / `BRAVE_SEARCH_API_KEY`. Default is auto-detect from whichever key is present.

### Fallback providers + auxiliary models

Vision, compression, title-gen, web extract should not always burn the main model. Upstream: `fallback_providers` / `fallback_model`, and `auxiliary.<task>.{provider,model}`. Dashboard cannot save these under Nix managed mode, so Neo would have to pin them.

### MCP servers

First-class on the upstream NixOS module (`mcpServers.*`, stdio or HTTP, optional OAuth). Neo does not expose it. Secrets must stay in env files (`${VAR}` in server env), not in `settings`.

### Write-approval gates

`skills.write_approval` and `memory.write_approval` — useful on a homeserver where the agent has sudo. Default upstream is off (write freely).

---

## Lower

### Container mode (`container.enable`)

Agent can `apt` / `pip` / `npm` inside a persistent Ubuntu container. Neo currently runs native with a large `extraPackages` set instead. Tradeoff: mutable layer vs reproducibility. Recreating the container (image / volume / options change) drops the writable layer.

### Official `backend.mode = "dashboard"`

The hermes-agent NixOS module can run `hermes dashboard` next to the gateway. Neo has a hand-rolled `hermes-dashboard` unit (non-loopback + internal basic auth + SWAG auto-login). Could collapse onto `backend.*` if the bind/auth behaviour matches.

### `toolsets` / compression / `max_turns`

`toolsets` is hardcoded to `["all"]`. Compression (`enabled`, `threshold`, `summary_model`) and `max_turns` / `agent.max_turns` are commonly customized in the NixOS module examples.

### TTS / image generation / browser cloud

Tool Gateway via Nous (`web.backend`, `image_gen.provider`, `tts.provider`, `browser.cloud_provider` = `nous`) or direct keys (xAI, fal, ElevenLabs, …). Only worth exposing after Portal OAuth or a specific operator request.

---

## Notes for later

- Hermes NixOS **managed mode** blocks `hermes setup`, `hermes config set`, and dashboard config saves. Anything we want durable has to be a Neo option (or a CLI OAuth flow that writes `auth.json`).
- Do not put secrets in `settings` / Nix `environment` if we ever grow a real secret path; today Neo already puts tokens in `settings.toml` → unit env (world-readable store). Same as other services; not a Hermes-only problem.
- `packages.configKeys` on the hermes-agent flake is every `DEFAULT_CONFIG` leaf (config.yaml), not the provider catalog. Providers live in `plugins/model-providers/`.
- Overlay-only ids come from `HERMES_OVERLAYS` + dashboard `_OAUTH_PROVIDER_CATALOG` (flow/name). If Hermes adds another overlay-only OAuth provider, it should appear without a Neo extra.
