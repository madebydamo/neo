# Assistant chat and settings tools

Status: implemented. How the AI parts of the neo web UI work today. The original
design (why proposals, the MCP tools, the operations and the card) is
[descriptions-and-guides.md §3a/§3b](descriptions-and-guides.md#3a-agent-chat-in-the-neo-web-ui);
this doc describes the chat drawer as built and points there for the settings tools.

## Two tokens

With Hermes on, two visible secrets turn on the AI parts of the web UI
(`nix/services/hermes/default.nix`):

- **Gateway token** (≥16 chars): Hermes' API server (`API_SERVER_KEY`,
  127.0.0.1:`gatewayPort`) and the **chat drawer**.
- **Settings tools token**: the MCP server `neo` for Hermes at `/internal/mcp`,
  which reads settings and proposes changes.

## Chat drawer

Code:

| Part | Where |
|------|-------|
| Drawer JS | `cli/static/agent_chat.js` (+ `agent_chat_*.js`, `agent_chat.test.js`) |
| Markup | `cli/templates/partials/agent_drawer.html.hbs`, `partials/agent_launch.html.hbs` |
| Routes | `cli/src/commands/web/chat/` (`chat::routes()`), Hermes client `cli/src/commands/web/agent/` |
| Hosts | `chat/host.rs` (`ChatHost`, `chat::Host`), `chat/neo_host.rs` (`NeoHost`), `cli/src/commands/security/chat.rs` |
| `@` references | `chat/refs.rs` |

### One set of routes, one host per page

The routes are shared. Each server manages a `chat::Host` (`ChatHost` in
`chat/host.rs`) and mounts `chat::routes()`. The host decides:

- its contexts,
- which past chats it lists,
- the first-turn instructions,
- the `@` candidates and the reference block,
- the Markdown render.

neo web's host is `chat/neo_host.rs` (`NeoHost`). The security report's is
`security/chat.rs`: contexts `security:general|<finding>`, and `@CVE-…` tags
findings. A new page with the drawer adds a host, never a second set of routes.

### Proxy and opening

- neo-web proxies `/agent/open|chat|approve|stop|render` to the Hermes session
  chat SSE API. The key stays server-side.
- Open the drawer from any element with
  `data-neo-agent="general|service:<name>|guide:<id>"` (plus
  `data-neo-agent-title`, `-starter`, `-hint`).
- Render such buttons only when the context has `assistant`
  (`agent::available()`).

### Chats

- Every open starts a **new chat**, unless a turn is running or an unread reply
  waits. `/agent/open` only hands out a `neo-<context>-<ts>` id; the Hermes
  session, titled from the first message, is created by the first `/agent/chat`.
- **Chats** in the drawer lists past chats: `GET /agent/sessions?context=`
  (`neo-` ids with messages, this page's first). Reopen = `/agent/open
  {session_id}`, newest 500 messages. Delete: `DELETE /agent/sessions/<id>`.

### Turns survive reloads

- neo-web reads each turn's Hermes stream in its own task (Hermes interrupts a
  turn whose reader leaves), so a reload or a closed drawer never stops it.
- The drawer remembers `{session_id, run_id}` in sessionStorage and polls
  `GET /agent/run/<id>` (status + pending approvals).
- The header button (`data-neo-agent-launch`) shows working /
  waiting-for-approval / unread.
- A status line under the log shows phase, elapsed time and the current tool.
- Approval cards count down `timeout_s`. They end on expiry, when the run no
  longer lists them, or with a plain message on 409 `approval_not_pending` /
  `stale_generation`.
- Tool calls render as collapsible rows: input from `tool.started.args`, output
  from `run.completed.messages`, duration from the event `ts`.

### `@` references

- `GET /agent/refs` (`chat/refs.rs`) lists services, settings
  (`extract_option_index.nix`), guides and privacy categories. Cached until
  `settings.toml` / `flake.lock` change.
- The drawer fuzzy-matches them client-side.
- Tokens `@svc`, `@svc.opt`, `@core.opt`, `@guide:<id>`, `@privacy:<id>` in a
  sent message get a `<neo-references>` block appended (secrets redacted like
  the settings tools). `agent/history.rs` strips it again when history is shown.

## Settings tools

MCP server `neo` at `/internal/mcp` (`cli/src/commands/web/routes/agent_tools.rs`;
SWAG 404s `/internal/`). Hermes reads settings (secrets redacted) and
**proposes** changes. Tools, operations, validation and the card:
[descriptions-and-guides.md §3b](descriptions-and-guides.md#3b-settings-tool).

Rules:

- A proposal (`cli/src/commands/web/proposals/`, card
  `cli/templates/proposal_card.html.hbs`, page `/proposals/<id>`) writes nothing
  until the user clicks Save (`settings::edit::apply_edit`, the form's write
  path) or Save & apply (then `apply_or_activate`).
- New settings writes go through `apply_edit` too, never a second path.
- A change is `{op, path, key?, field?, value?, group?, from?}` with ops
  `set`/`unset`/`add`/`remove`/`generate`/`ask`/`copy`/`move`. `key` + `field`
  address one entry field (`accounts` → `tmr` → `password`).
- Secret values never pass through the agent; a value at a secret position is
  refused at any depth:
  - `generate` runs the field's Generate script;
  - `ask` puts a password input on the card, one per `group`, sent only with
    the Save click;
  - `copy`/`move` name a source in `from {path, key?, field?}` whose saved
    value neo reads on Save, secrets included. A rename is a `move` inside one
    option, and a secret may only land on a secret position
    (`redact::secret_leak`).
- Every value shown to the agent or on the card goes through `web/redact.rs`
  (nested secrets → `<set>`/`<unset>`).
- Proposals hold no secret values and persist in
  `$STATE_DIRECTORY/proposals.json` (24 h), so cards survive the neo-web restart
  of Save & apply.
