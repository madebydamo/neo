"""MCP server for Docmost (community edition), signed in through tinyauth.

Docmost's own MCP server is an enterprise feature. This one speaks MCP
(streamable HTTP, JSON responses) on /mcp and is its own OAuth 2.1
authorization server, so MCP clients (Claude, Cursor, MCP Inspector, ...)
connect with nothing but the URL:

  /.well-known/oauth-protected-resource[/mcp]   RFC 9728 resource metadata
  /.well-known/oauth-authorization-server       RFC 8414 server metadata
  /mcp/oauth/register                           RFC 7591 dynamic registration
  /mcp/oauth/authorize                          behind tinyauth (SWAG)
  /mcp/oauth/token                              PKCE code + refresh grants

The browser step goes through tinyauth like any app: SWAG passes the user in
Remote-* headers to /mcp/oauth/authorize, the user approves the client, and
the bridge issues a code. Clients, codes and tokens are HMAC-signed blobs
(MCP_KEY), so nothing is stored; a new appSecret revokes everything.

Tools act as the signed-in user: the bridge logs into Docmost with the same
HMAC(SSO_KEY, email) password as docmost-sso (sso.py, imported here), so
Docmost's own space and page permissions apply.
Stdlib only; runs in a plain python image.
"""

import base64
import binascii
import hashlib
import hmac
import html
import http.client
import ipaddress
import json
import mimetypes
import os
import re
import secrets
import socket
import ssl
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock

import sso
from sso import DocmostError, auth_token, call, log

PORT = int(os.environ.get("PORT", "8081"))
PUBLIC_URL = os.environ["PUBLIC_URL"].rstrip("/")
MCP_KEY = os.environ["MCP_KEY"].encode()
RESOURCE = f"{PUBLIC_URL}/mcp"
RESOURCE_METADATA = f"{PUBLIC_URL}/.well-known/oauth-protected-resource/mcp"

ACCESS_TTL = 3600
CODE_TTL = 300
CONSENT_TTL = 600
# Absolute: refreshing never extends past this, so users pass tinyauth again
# (and its access rules) at least this often.
SESSION_TTL = 30 * 24 * 3600

PROTOCOL_VERSIONS = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"]
# Docmost's default upload limit (FILE_UPLOAD_SIZE_LIMIT); base64 in a
# tools/call adds a third, plus JSON-RPC overhead.
MAX_FILE = 50 * 1024 * 1024
MAX_BODY = MAX_FILE * 4 // 3 + 1024 * 1024


# --- signed blobs ---------------------------------------------------------


def b64(data):
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def unb64(text):
    return base64.urlsafe_b64decode(text + "=" * (-len(text) % 4))


def sign(kind, payload, ttl=None):
    body = {**payload, "k": kind}
    if ttl:
        body["exp"] = int(time.time()) + ttl
    raw = b64(json.dumps(body, separators=(",", ":")).encode())
    mac = b64(hmac.new(MCP_KEY, f"{kind}.{raw}".encode(), hashlib.sha256).digest())
    return f"{raw}.{mac}"


def verify(kind, blob):
    """Payload of a blob signed as `kind`, or None (bad, wrong kind, expired)."""
    try:
        raw, mac = (blob or "").split(".")
        want = b64(hmac.new(MCP_KEY, f"{kind}.{raw}".encode(), hashlib.sha256).digest())
        if not hmac.compare_digest(mac, want):
            return None
        body = json.loads(unb64(raw))
    except ValueError:
        return None
    if body.get("k") != kind or body.get("exp", float("inf")) < time.time():
        return None
    return body


# One-time use for codes and refresh tokens, while they could still verify.
spent = {}
spent_lock = Lock()


def spend(blob, exp):
    now = time.time()
    with spent_lock:
        for key in [k for k, e in spent.items() if e < now]:
            del spent[key]
        if blob in spent:
            return False
        spent[blob] = exp
        return True


# --- OAuth ------------------------------------------------------------------


def redirect_ok(uri):
    """https anywhere, plain http only to loopback (native / CLI clients)."""
    try:
        parts = urllib.parse.urlsplit(uri)
    except ValueError:
        return False
    if parts.fragment or not parts.hostname:
        return False
    if parts.scheme == "https":
        return True
    return parts.scheme == "http" and parts.hostname in ("localhost", "127.0.0.1", "::1")


def redirect_matches(registered, uri):
    # RFC 8252: loopback redirects may use any port.
    if uri in registered:
        return True
    parts = urllib.parse.urlsplit(uri)
    if parts.scheme != "http":
        return False
    for reg in registered:
        r = urllib.parse.urlsplit(reg)
        if r.scheme == "http" and r.hostname == parts.hostname and r.path == parts.path and r.query == parts.query:
            return True
    return False


def with_query(uri, params):
    parts = urllib.parse.urlsplit(uri)
    query = "&".join(filter(None, [parts.query, urllib.parse.urlencode(params)]))
    return urllib.parse.urlunsplit(parts._replace(query=query))


def server_metadata():
    return {
        "issuer": PUBLIC_URL,
        "authorization_endpoint": f"{PUBLIC_URL}/mcp/oauth/authorize",
        "token_endpoint": f"{PUBLIC_URL}/mcp/oauth/token",
        "registration_endpoint": f"{PUBLIC_URL}/mcp/oauth/register",
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": ["docmost"],
    }


def resource_metadata():
    return {
        "resource": RESOURCE,
        "authorization_servers": [PUBLIC_URL],
        "bearer_methods_supported": ["header"],
        "scopes_supported": ["docmost"],
        "resource_name": "Docmost",
    }


def register(body):
    uris = body.get("redirect_uris")
    if not isinstance(uris, list) or not uris or not all(isinstance(u, str) and redirect_ok(u) for u in uris):
        return 400, {
            "error": "invalid_redirect_uri",
            "error_description": "redirect_uris: https, or http on localhost",
        }
    name = str(body.get("client_name") or "MCP client")[:80]
    client_id = sign("client", {"r": uris, "n": name})
    return 201, {
        "client_id": client_id,
        "client_id_issued_at": int(time.time()),
        "client_name": name,
        "redirect_uris": uris,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    }


def issue_tokens(email, client_id, session_exp):
    now = int(time.time())
    access = sign("access", {"sub": email, "cid": client_id}, ACCESS_TTL)
    refresh = sign("refresh", {"sub": email, "cid": client_id, "exp": session_exp, "j": secrets.token_hex(8)})
    return {
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": min(ACCESS_TTL, max(session_exp - now, 0)),
        "refresh_token": refresh,
        "scope": "docmost",
    }


def token(form):
    def err(code, desc):
        return 400, {"error": code, "error_description": desc}

    grant = form.get("grant_type")
    client_id = form.get("client_id", "")
    if not verify("client", client_id):
        return 401, {"error": "invalid_client", "error_description": "unknown client_id; register again"}

    if grant == "authorization_code":
        code = form.get("code", "")
        data = verify("code", code)
        if not data or data["cid"] != client_id or data["ru"] != form.get("redirect_uri"):
            return err("invalid_grant", "code is invalid, expired or for another client")
        verifier = form.get("code_verifier", "")
        challenge = b64(hashlib.sha256(verifier.encode()).digest())
        if not verifier or not hmac.compare_digest(challenge, data["cc"]):
            return err("invalid_grant", "PKCE verification failed")
        if not spend(code, data["exp"]):
            return err("invalid_grant", "code already used")
        return 200, issue_tokens(data["sub"], client_id, int(time.time()) + SESSION_TTL)

    if grant == "refresh_token":
        refresh = form.get("refresh_token", "")
        data = verify("refresh", refresh)
        if not data or data["cid"] != client_id:
            return err("invalid_grant", "refresh token is invalid or expired; sign in again")
        if not spend(refresh, data["exp"]):
            return err("invalid_grant", "refresh token already used")
        return 200, issue_tokens(data["sub"], client_id, data["exp"])

    return err("unsupported_grant_type", "use authorization_code or refresh_token")


def consent_page(client, params, email, consent):
    host = urllib.parse.urlsplit(params["redirect_uri"]).netloc
    fields = "".join(
        f"<input type=hidden name='{html.escape(k)}' value='{html.escape(v)}'>"
        for k, v in {**params, "consent": consent}.items()
    )
    return (
        "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>"
        "<title>Connect to Docmost</title>"
        "<body style='font-family:system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem'>"
        "<h1>Connect to Docmost</h1>"
        f"<p><b>{html.escape(client['n'])}</b> wants to read and edit Docmost pages as "
        f"<b>{html.escape(email)}</b>.</p>"
        f"<p>It will be sent back to <code>{html.escape(host)}</code>. Only allow clients you set up yourself.</p>"
        f"<form method=post>{fields}"
        "<button name=decision value=allow style='font-size:1rem;padding:.5rem 1.2rem'>Allow</button> "
        "<button name=decision value=deny style='font-size:1rem;padding:.5rem 1.2rem'>Deny</button>"
        "</form>"
    )


# --- Docmost as the user ---------------------------------------------------


class Sessions:
    """Docmost auth tokens per user, renewed on 401."""

    def __init__(self):
        self.lock = Lock()
        self.tokens = {}

    def call(self, email, path, body):
        return self.as_user(email, lambda token: call(path, body, token)[0])

    def upload(self, email, page_id, name, mime, data):
        return self.as_user(email, lambda token: upload(token, page_id, name, mime, data))

    def as_user(self, email, fn):
        with self.lock:
            token = self.tokens.get(email)
        if token is None:
            token = self.login(email)
        try:
            return fn(token)
        except DocmostError as e:
            if e.status != 401:
                raise
            return fn(self.login(email))

    def login(self, email):
        token = auth_token(sso.login(email))
        with self.lock:
            self.tokens[email] = token
        return token


def upload(token, page_id, name, mime, data):
    """POST one file to Docmost's /files/upload (multipart); the attachment row."""
    boundary = secrets.token_hex(16)
    quoted = name.replace("\\", "\\\\").replace('"', '\\"').replace("\r", "").replace("\n", "")
    # Docmost reads fields only when they come before the file.
    body = (
        f'--{boundary}\r\nContent-Disposition: form-data; name="pageId"\r\n\r\n{page_id}\r\n'
        f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{quoted}"\r\n'
        f"Content-Type: {mime}\r\n\r\n"
    ).encode() + data + f"\r\n--{boundary}--\r\n".encode()
    url = urllib.parse.urlsplit(sso.DOCMOST)
    conn = http.client.HTTPConnection(url.hostname, url.port or 80, timeout=120)
    try:
        conn.request(
            "POST",
            "/api/files/upload",
            body,
            {
                "Content-Type": f"multipart/form-data; boundary={boundary}",
                "Accept": "application/json",
                "Authorization": f"Bearer {token}",
            },
        )
        res = conn.getresponse()
        raw = res.read().decode("utf-8", "replace")
    finally:
        conn.close()
    if res.status >= 400:
        raise DocmostError(res.status, raw)
    data = json.loads(raw)
    # Sent with res.send(), so not always inside Docmost's {data: …} envelope.
    return data["data"] if "id" not in data and isinstance(data.get("data"), dict) else data


sessions = Sessions()


def page_url(page):
    space = page.get("space") or {}
    if space.get("slug") and page.get("slugId"):
        return f"{PUBLIC_URL}/s/{space['slug']}/p/{page['slugId']}"
    return None


def page_summary(page):
    out = {k: page.get(k) for k in ("id", "slugId", "title", "parentPageId", "updatedAt") if page.get(k) is not None}
    space = page.get("space") or {}
    if space:
        out["space"] = {k: space.get(k) for k in ("id", "name", "slug") if space.get(k)}
    if "hasChildren" in page:
        out["hasChildren"] = page["hasChildren"]
    if page_url(page):
        out["url"] = page_url(page)
    return out


# --- Markdown fidelity ------------------------------------------------------
#
# Docmost converts Markdown itself (marked on import, turndown on export).
# Two of its quirks would make every read-modify-write damage a page:
#  - tables: the export keeps <colgroup>, so turndown's GFM plugin misses the
#    header row and adds an empty one; the real header becomes a body row
#    (fixed upstream by stripping colgroups; harmless once that ships).
#  - math: the import runs math through Markdown (so "\," loses its
#    backslash) and the export Markdown-escapes it ("\frac" -> "\\frac").
# get_page undoes the export side, writes escape math for the import side,
# so what get_page returns can be written back unchanged.

FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
CODE_SPAN = re.compile(r"(`+)(?!`)[\s\S]+?(?<!`)\1(?!`)")
PUNCT = r"[!-/:-@\[-`{-~]"


def map_prose(md, fn):
    """Apply fn to the Markdown outside fenced code blocks and code spans."""
    out, prose, fence = [], [], None

    def flush():
        if prose:
            text = "".join(prose)
            pos = 0
            for m in CODE_SPAN.finditer(text):
                out.append(fn(text[pos : m.start()]))
                out.append(m.group(0))
                pos = m.end()
            out.append(fn(text[pos:]))
            prose.clear()

    for line in md.splitlines(keepends=True):
        m = FENCE.match(line)
        if fence:
            out.append(line)
            if m and m.group(1)[0] == fence[0] and len(m.group(1)) >= len(fence) and not line[m.end() :].strip():
                fence = None
        elif m and not (m.group(1)[0] == "`" and "`" in line[m.end() :]):
            flush()
            fence = m.group(1)
            out.append(line)
        else:
            prose.append(line)
    flush()
    return "".join(out)


# Export: "$$\n…\n$$" blocks and "$…$" inline, content Markdown-escaped.
READ_MATH_BLOCK = re.compile(r"(?m)^\$\$\n([\s\S]*?)\n\$\$$")
READ_MATH_INLINE = re.compile(r"(?<![\\$])\$(?!\$)((?:\\.|[^\\$\n])+)\$")
# Import (Docmost's marked extensions): same patterns as its tokenizers.
WRITE_MATH_BLOCK = re.compile(r"\$\$(?!\$)([\s\S]+?)\$\$")
WRITE_MATH_INLINE = re.compile(r"(?<!\S)\$(?!\s)(.+?)(?<!\s)\$(?!\d)")


def unescape_md(text):
    return re.sub(r"\\(" + PUNCT + ")", r"\1", text)


def escape_md(text):
    # CommonMark turns every backslash-escaped ASCII punctuation back into itself.
    return re.sub("(" + PUNCT + ")", r"\\\1", text)


READ_MATH = re.compile(READ_MATH_BLOCK.pattern + "|" + READ_MATH_INLINE.pattern.replace("(?m)", ""), re.M)


def math_from_docmost(md, doc):
    """Show each formula as stored: the export escapes it and flattens newlines."""
    stored = [(n.get("attrs") or {}).get("text") or "" for n in walk(doc or {}) if n.get("type") in ("mathBlock", "mathInline")]
    found = []
    map_prose(md, lambda text: found.extend(READ_MATH.finditer(text)) or text)
    exact = iter(stored if len(found) == len(stored) else [])

    def replace(m):
        block = m.group(1) is not None
        text = next(exact, None)
        if text is None:
            text = unescape_md(m.group(1) if block else m.group(2))
        return f"$$\n{text}\n$$" if block else f"${text}$"

    return map_prose(md, lambda text: READ_MATH.sub(replace, text))


def math_to_docmost(md):
    """(Markdown for Docmost's import, the LaTeX of each math node in order)."""
    latex = []

    def escaped(m):
        latex.append(m.group(1).strip())
        # Newlines would become <br> inside the formula; fix_math restores them.
        return escape_md(m.group(1).replace("\n", " "))

    def prose(text):
        parts, pos = [], 0
        for m in WRITE_MATH_BLOCK.finditer(text):
            parts.append(WRITE_MATH_INLINE.sub(lambda i: f"${escaped(i)}$", text[pos : m.start()]))
            parts.append(f"$${escaped(m)}$$")
            pos = m.end()
        parts.append(WRITE_MATH_INLINE.sub(lambda i: f"${escaped(i)}$", text[pos:]))
        return "".join(parts)

    return map_prose(md, prose), latex


def fix_math(email, page_id, latex, operation):
    """Set the math just written to its exact LaTeX.

    Docmost's import stores math as rendered HTML (entities, <br>), so after a
    Markdown write the formulas are patched in the page JSON and saved as JSON.
    """
    if not latex:
        return
    doc = sessions.call(email, "/pages/info", {"pageId": page_id}).get("content") or {}
    nodes = [n for n in walk(doc) if n.get("type") in ("mathBlock", "mathInline")]
    if operation == "append":
        nodes = nodes[len(nodes) - len(latex) :] if len(nodes) >= len(latex) else []
    elif operation == "prepend":
        nodes = nodes[: len(latex)]
    if len(nodes) != len(latex):
        log(f"math fix skipped on {page_id}: {len(nodes)} math nodes for {len(latex)} formulas")
        return
    changed = False
    for node, text in zip(nodes, latex):
        attrs = node.setdefault("attrs", {})
        if attrs.get("text") != text:
            attrs["text"] = text
            changed = True
    if changed:
        sessions.call(email, "/pages/update", {"pageId": page_id, "content": doc, "format": "json", "operation": "replace"})


def write_markdown(email, page_id, md, operation="replace", extra=None):
    """Write Markdown to a page (Docmost /pages/update), math made exact."""
    body, latex = math_to_docmost(md)
    page = sessions.call(
        email,
        "/pages/update",
        {**(extra or {}), "pageId": page_id, "content": body, "format": "markdown", "operation": operation},
    )
    fix_math(email, page["id"], latex, operation)
    return page


EMPTY_ROW = re.compile(r"^\|(?:\s+\|)+$")
DIVIDER_ROW = re.compile(r"^\|(?:\s*:?-{3,}:?\s*\|)+$")


def node_text(node):
    return node.get("text", "") + "".join(node_text(c) for c in node.get("content") or [])


def walk(node):
    yield node
    for child in node.get("content") or []:
        yield from walk(child)


def signature(cells):
    # Compare header rows by their words only: Markdown and JSON spell links,
    # escapes and formatting differently.
    return "|".join(re.sub(r"[\W_]+", "", re.sub(r"\]\([^)]*\)", "", c)).lower() for c in cells)


def md_cells(row):
    return re.split(r"(?<!\\)\|", row.strip().strip("|"))


def fix_tables(md, doc):
    """Put header rows that Docmost's export pushed down back on top."""
    headers = set()
    for node in walk(doc or {}):
        if node.get("type") == "table" and node.get("content"):
            first = node["content"][0].get("content") or []
            if first and all(c.get("type") == "tableHeader" for c in first):
                headers.add(signature([node_text(c) for c in first]))
    if not headers:
        return md

    lines = md.split("\n")
    i = 0
    while i + 2 < len(lines):
        if (
            EMPTY_ROW.match(lines[i])
            and DIVIDER_ROW.match(lines[i + 1])
            and lines[i + 2].startswith("|")
            and (i == 0 or not lines[i - 1].startswith("|"))
            and signature(md_cells(lines[i + 2])) in headers
        ):
            lines[i : i + 3] = [lines[i + 2], lines[i + 1]]
        i += 1
    return "\n".join(lines)


# What Markdown carries through Docmost's import/export; anything else is
# dropped by a whole-page rewrite (see lossy()).
MD_NODES = {
    "doc", "paragraph", "text", "heading", "bulletList", "orderedList", "listItem",
    "taskList", "taskItem", "blockquote", "codeBlock", "horizontalRule", "hardBreak",
    "image", "table", "tableRow", "tableHeader", "tableCell", "callout",
    "mathInline", "mathBlock", "details", "detailsSummary", "detailsContent",
    "footnotes", "footnote", "footnoteReference",
}
MD_MARKS = {"bold", "italic", "strike", "code", "link", "highlight", "textStyle"}
NODE_LABELS = {
    "mention": "mention",
    "attachment": "file attachment",
    "drawio": "draw.io diagram",
    "excalidraw": "Excalidraw drawing",
    "embed": "embed",
    "video": "video",
    "audio": "audio",
    "pdf": "PDF",
    "subpages": "subpages list",
    "columns": "column layout",
    "status": "status badge",
    "tabs": "tabs",
    "pageBreak": "page break",
    "transclusionReference": "synced block",
    "transclusionSource": "synced block",
}
MARK_LABELS = {
    "comment": "comment anchor",
    "underline": "underline",
    "subscript": "subscript",
    "superscript": "superscript",
}


def lossy(doc):
    """Human list of what a Markdown rewrite of this page would drop."""
    found = {}

    def add(label):
        found[label] = found.get(label, 0) + 1

    for node in walk(doc or {}):
        kind, attrs = node.get("type"), node.get("attrs") or {}
        if kind not in MD_NODES:
            add(NODE_LABELS.get(kind, kind))
            continue
        if attrs.get("textAlign") not in (None, "left", "start"):
            add("text alignment")
        if attrs.get("indent"):
            add("indentation")
        if kind in ("tableCell", "tableHeader"):
            if (attrs.get("colspan") or 1) > 1 or (attrs.get("rowspan") or 1) > 1:
                add("merged table cell")
            if attrs.get("backgroundColor"):
                add("table cell colour")
        for mark in node.get("marks") or []:
            name, mattrs = mark.get("type"), mark.get("attrs") or {}
            if name not in MD_MARKS:
                add(MARK_LABELS.get(name, name))
            elif name in ("textStyle", "highlight") and mattrs.get("color"):
                add("text colour" if name == "textStyle" else "highlight colour")
    return [f"{n}× {label}" if n > 1 else label for label, n in sorted(found.items())]


UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$", re.I)


def page_uuid(email, page_id):
    """UUID for a page id or slugId: some Docmost endpoints (delete) take only UUIDs."""
    if UUID.match(page_id or ""):
        return page_id
    return sessions.call(email, "/pages/info", {"pageId": page_id})["id"]


def read_page(email, page_id):
    """(page, markdown, json doc): Markdown as get_page shows it, fixed up."""
    page = sessions.call(email, "/pages/info", {"pageId": page_id})
    doc = page.get("content")
    md = sessions.call(email, "/pages/info", {"pageId": page["id"], "format": "markdown"}).get("content") or ""
    return page, math_from_docmost(fix_tables(md, doc), doc), doc


def ensure_markdown_safe(page_id, doc, args):
    lost = lossy(doc)
    if lost and not args.get("dropUnsupported"):
        raise ValueError(
            f"Page {page_id} has content Markdown cannot carry ({', '.join(lost)}); rewriting it would delete that. "
            "Use update_page with operation append/prepend, edit it in the Docmost UI, "
            "or pass dropUnsupported: true to rewrite anyway."
        )


# --- page positions (fractional indexing, as Docmost's fractional-indexing-jittered)

DIGITS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"


def integer_length(head):
    if "a" <= head <= "z":
        return ord(head) - ord("a") + 2
    if "A" <= head <= "Z":
        return ord("Z") - ord(head) + 2
    raise ValueError(f"invalid position {head!r}")


def increment_integer(x):
    head, digits = x[0], list(x[1:])
    for i in range(len(digits) - 1, -1, -1):
        d = DIGITS.index(digits[i]) + 1
        if d < len(DIGITS):
            digits[i] = DIGITS[d]
            return head + "".join(digits)
        digits[i] = "0"
    if head == "Z":
        return "a0"
    if head == "z":
        return None
    head = chr(ord(head) + 1)
    if head > "a":
        digits.append("0")
    else:
        digits.pop()
    return head + "".join(digits)


def position_after(last):
    """A position key after `last` (None: first child), with Docmost-style jitter."""
    base = "a0" if last is None else increment_integer(last[: integer_length(last[0])])
    if base is None:
        # Integer space exhausted: extend the fraction instead.
        base = last
    jitter = "".join(secrets.choice(DIGITS[1:]) for _ in range(3))
    return (base + jitter)[: max(12, len(base) + 1)]


def children(email, space_id=None, page_id=None):
    body = {"limit": 100, **({"pageId": page_id} if page_id else {"spaceId": space_id})}
    items, cursor = [], None
    while True:
        res = sessions.call(email, "/pages/sidebar-pages", {**body, **({"cursor": cursor} if cursor else {})})
        items += res.get("items", [])
        cursor = (res.get("meta") or {}).get("nextCursor")
        if not cursor:
            return items


# --- tools ---------------------------------------------------------------------


class Markdown:
    """Tool result: metadata as JSON, then the Markdown body as plain text."""

    def __init__(self, meta, body):
        self.meta, self.body = meta, body


def tool_list_spaces(email, args):
    spaces, cursor = [], None
    while True:
        res = sessions.call(email, "/spaces", {"limit": 100, **({"cursor": cursor} if cursor else {})})
        spaces += [
            {k: s.get(k) for k in ("id", "name", "slug", "description") if s.get(k)}
            | {"role": (s.get("membership") or {}).get("role")}
            for s in res.get("items", [])
        ]
        cursor = (res.get("meta") or {}).get("nextCursor")
        if not cursor:
            return {"spaces": spaces}


def tool_search(email, args):
    body = {"query": args["query"], "limit": min(int(args.get("limit") or 20), 50)}
    if args.get("spaceId"):
        body["spaceId"] = args["spaceId"]
    if args.get("titleOnly"):
        body["titleOnly"] = True
    res = sessions.call(email, "/search", body)
    items = res.get("items", res) if isinstance(res, dict) else res
    out = []
    for item in items or []:
        hit = page_summary(item)
        if item.get("highlight"):
            hit["highlight"] = html.unescape(re.sub(r"</?b>", "**", item["highlight"]))
        out.append(hit)
    return {"results": out}


def tool_list_pages(email, args):
    if not args.get("spaceId") and not args.get("parentPageId"):
        raise ValueError("give spaceId (top-level pages) or parentPageId (its child pages)")
    depth = max(1, min(int(args.get("depth") or 1), 5))
    budget = [300]

    def level(space_id, page_id, d):
        pages = []
        for p in children(email, space_id, page_id):
            if budget[0] <= 0:
                break
            budget[0] -= 1
            entry = {k: p[k] for k in ("id", "slugId", "title") if p.get(k)}
            if p.get("hasChildren"):
                if d < depth:
                    entry["children"] = level(space_id, p["id"], d + 1)
                else:
                    entry["hasChildren"] = True
            pages.append(entry)
        return pages

    if args.get("parentPageId"):
        parent = sessions.call(email, "/pages/info", {"pageId": args["parentPageId"]})
        tree = level(parent["spaceId"], parent["id"], 1)
    else:
        tree = level(args["spaceId"], None, 1)
    out = {"pages": tree}
    if budget[0] <= 0:
        out["truncated"] = "stopped at 300 pages; list a subtree with parentPageId"
    return out


def tool_get_page(email, args):
    page, md, doc = read_page(email, args["pageId"])
    meta = page_summary(page)
    meta["canEdit"] = (page.get("permissions") or {}).get("canEdit")
    lost = lossy(doc)
    if lost:
        meta["notShownInMarkdown"] = lost
    return Markdown(meta, md)


def tool_create_page(email, args):
    body = {"spaceId": args["spaceId"], "title": args["title"]}
    if args.get("parentPageId"):
        body["parentPageId"] = page_uuid(email, args["parentPageId"])
    latex = []
    if args.get("content"):
        body["content"], latex = math_to_docmost(args["content"])
        body["format"] = "markdown"
    page = sessions.call(email, "/pages/create", body)
    fix_math(email, page["id"], latex, "replace")
    page.setdefault("space", (sessions.call(email, "/pages/info", {"pageId": page["id"]}) or {}).get("space"))
    return page_summary(page)


def tool_update_page(email, args):
    title = {"title": args["title"]} if args.get("title") is not None else {}
    if args.get("content") is None:
        if not title:
            raise ValueError("nothing to update: give title and/or content")
        return page_summary(sessions.call(email, "/pages/update", {"pageId": page_uuid(email, args["pageId"]), **title}))
    operation = args.get("operation") or "replace"
    page = sessions.call(email, "/pages/info", {"pageId": args["pageId"]})
    if operation == "replace":
        ensure_markdown_safe(args["pageId"], page.get("content"), args)
    return page_summary(write_markdown(email, page["id"], args["content"], operation, title))


MD_SYNTAX = re.compile(r"[\\`*_\[\]<>#|$~=\n]|^\s*([-+>]|\d+\.)\s")


def edit_in_json(doc, edits):
    """Apply plain-text edits inside text nodes, or None if any edit needs Markdown.

    Keeps everything else (comments, mentions, colours, …) exactly as is.
    """
    doc = json.loads(json.dumps(doc))
    texts = [n for n in walk(doc) if n.get("type") == "text"]
    for edit in edits:
        old, new = edit["oldText"], edit["newText"]
        if MD_SYNTAX.search(old) or MD_SYNTAX.search(new):
            return None
        hits = [n for n in texts if old in n["text"]]
        total = sum(n["text"].count(old) for n in hits)
        if total == 0 or (total > 1 and not edit.get("replaceAll")):
            return None
        for node in hits:
            node["text"] = node["text"].replace(old, new)
    return doc


def tool_edit_page(email, args):
    edits = args.get("edits")
    if not isinstance(edits, list) or not edits:
        raise ValueError("edits: give a list of {oldText, newText}")
    for n, edit in enumerate(edits, 1):
        if not isinstance(edit.get("oldText"), str) or not edit["oldText"] or not isinstance(edit.get("newText"), str):
            raise ValueError(f"edit {n}: oldText (non-empty) and newText are required")
    page, md, doc = read_page(email, args["pageId"])

    patched = edit_in_json(doc, edits) if doc else None
    if patched is not None:
        updated = sessions.call(
            email, "/pages/update", {"pageId": page["id"], "content": patched, "format": "json", "operation": "replace"}
        )
        out = page_summary({**updated, "space": updated.get("space") or page.get("space")})
        out["editsApplied"] = len(edits)
        return out

    for n, edit in enumerate(edits, 1):
        old, new = edit["oldText"], edit["newText"]
        count = md.count(old)
        if count == 0:
            raise ValueError(
                f"edit {n}: oldText not found. Match the Markdown exactly as get_page returns it "
                "(Docmost normalises e.g. *italic* to _italic_ and pads table cells). No edits were saved."
            )
        if count > 1 and not edit.get("replaceAll"):
            raise ValueError(f"edit {n}: oldText occurs {count} times; add context or set replaceAll. No edits were saved.")
        md = md.replace(old, new)
    ensure_markdown_safe(args["pageId"], doc, args)
    updated = write_markdown(email, page["id"], md)
    out = page_summary({**updated, "space": updated.get("space") or page.get("space")})
    out["editsApplied"] = len(edits)
    return out


def tool_move_page(email, args):
    if "parentPageId" not in args and not args.get("spaceId"):
        raise ValueError("give parentPageId (a page, or null for the top level) and/or spaceId")
    page = sessions.call(email, "/pages/info", {"pageId": args["pageId"]})
    space_id = page["spaceId"]
    parent = None
    if args.get("parentPageId"):
        parent = sessions.call(email, "/pages/info", {"pageId": args["parentPageId"]})
    target_space = args.get("spaceId") or (parent or {}).get("spaceId") or space_id
    if parent and parent["spaceId"] != target_space:
        raise ValueError("parentPageId is not in spaceId")
    if target_space != space_id:
        # Lands at the top level of the target space.
        sessions.call(email, "/pages/move-to-space", {"pageId": page["id"], "spaceId": target_space})
    if parent or (target_space == space_id and page.get("parentPageId")):
        siblings = children(email, target_space, parent["id"] if parent else None)
        last = max((p["position"] for p in siblings if p.get("position") and p["id"] != page["id"]), default=None)
        sessions.call(
            email,
            "/pages/move",
            {"pageId": page["id"], "parentPageId": parent["id"] if parent else None, "position": position_after(last)},
        )
    return page_summary(sessions.call(email, "/pages/info", {"pageId": page["id"]}))


def tool_delete_page(email, args):
    sessions.call(email, "/pages/delete", {"pageId": page_uuid(email, args["pageId"])})
    return {"deleted": args["pageId"], "note": "moved to the space trash; restore it in the Docmost UI"}


# --- file upload -------------------------------------------------------------


def public_address(host, port):
    """A public IP for host. The bridge sits on the internal Docker network, so
    a URL to fetch must never reach Docmost's database, SWAG or other services."""
    try:
        infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    except socket.gaierror as e:
        raise ValueError(f"url: cannot resolve {host}") from e
    for info in infos:
        ip = ipaddress.ip_address(info[4][0])
        if not ip.is_global or ip.is_multicast:
            raise ValueError(f"url: {host} is not a public address")
    return infos[0][4][0]


class PinnedHTTPS(http.client.HTTPSConnection):
    """HTTPS to an already checked IP, with SNI and certificate for the host."""

    def __init__(self, host, ip, port, **kw):
        super().__init__(host, port, context=ssl.create_default_context(), **kw)
        self.ip = ip

    def connect(self):
        sock = socket.create_connection((self.ip, self.port), self.timeout)
        self.sock = self._context.wrap_socket(sock, server_hostname=self.host)


def fetch_url(url):
    """(bytes, file name, Content-Type) of a public http(s) URL, redirects followed."""
    for _ in range(5):
        parts = urllib.parse.urlsplit(url)
        if parts.scheme not in ("http", "https") or not parts.hostname:
            raise ValueError("url: only http(s) URLs")
        port = parts.port or (443 if parts.scheme == "https" else 80)
        ip = public_address(parts.hostname, port)
        if parts.scheme == "https":
            conn = PinnedHTTPS(parts.hostname, ip, port, timeout=30)
        else:
            conn = http.client.HTTPConnection(ip, port, timeout=30)
        target = urllib.parse.urlunsplit(("", "", parts.path or "/", parts.query, ""))
        try:
            conn.request("GET", target, headers={"Host": parts.netloc, "User-Agent": "neo-docmost-mcp"})
            res = conn.getresponse()
            if res.status in (301, 302, 303, 307, 308) and res.getheader("Location"):
                url = urllib.parse.urljoin(url, res.getheader("Location"))
                continue
            if res.status >= 400:
                raise ValueError(f"url: answered {res.status}")
            data = res.read(MAX_FILE + 1)
        finally:
            conn.close()
        if len(data) > MAX_FILE:
            raise ValueError(f"url: file is larger than {MAX_FILE // (1024 * 1024)} MB")
        name = urllib.parse.unquote(parts.path.rsplit("/", 1)[-1])
        return data, name, (res.getheader("Content-Type") or "").split(";")[0].strip()
    raise ValueError("url: too many redirects")


def file_node(attachment, alt):
    """The editor node Docmost's own upload inserts for this attachment."""
    src = f"/api/files/{attachment['id']}/{attachment['fileName']}"
    base = {"attachmentId": attachment["id"], "size": attachment.get("fileSize")}
    mime = attachment.get("mimeType") or ""
    if mime.startswith("image/"):
        return {"type": "image", "attrs": {**base, "src": src, **({"alt": alt} if alt else {})}}
    if mime.startswith("video/"):
        return {"type": "video", "attrs": {**base, "src": src, "title": attachment["fileName"]}}
    if mime.startswith("audio/"):
        return {"type": "audio", "attrs": {**base, "src": src}}
    return {
        "type": "attachment",
        "attrs": {**base, "url": src, "name": attachment["fileName"], "mime": mime},
    }


def tool_upload_file(email, args):
    sources = [k for k in ("url", "base64", "text") if args.get(k) is not None]
    if len(sources) != 1:
        raise ValueError("give exactly one of url, base64 or text")
    name, mime = (args.get("fileName") or "").strip(), (args.get("mimeType") or "").strip()
    if args.get("url") is not None:
        try:
            data, url_name, url_mime = fetch_url(args["url"])
        except OSError as e:
            raise ValueError(f"url: could not fetch it ({e})") from e
        name, mime = name or url_name, mime or url_mime
    elif args.get("base64") is not None:
        raw = re.sub(r"^data:[^,]*;base64,", "", args["base64"].strip())
        try:
            data = base64.b64decode(raw, validate=True)
        except binascii.Error as e:
            raise ValueError("base64: not valid base64") from e
    else:
        data = args["text"].encode()
    if not name:
        raise ValueError("fileName is required (with an extension, e.g. diagram.png)")
    if not data:
        raise ValueError("the file is empty")
    if len(data) > MAX_FILE:
        raise ValueError(f"file is larger than {MAX_FILE // (1024 * 1024)} MB")
    mime = mime or mimetypes.guess_type(name)[0] or "application/octet-stream"

    page = sessions.call(email, "/pages/info", {"pageId": args["pageId"]})
    attachment = sessions.upload(email, page["id"], name, mime, data)
    node = file_node(attachment, args.get("alt"))
    src = node["attrs"].get("src") or node["attrs"]["url"]
    out = {
        "attachmentId": attachment["id"],
        "fileName": attachment.get("fileName"),
        "mimeType": attachment.get("mimeType"),
        "size": attachment.get("fileSize"),
        "url": f"{PUBLIC_URL}{src}",
        "page": page_summary(page),
    }
    insert = args.get("insert") or "append"
    if insert in ("append", "prepend"):
        # JSON append/prepend goes through Docmost's collaboration document,
        # so the rest of the page and anyone editing it are left alone.
        sessions.call(
            email,
            "/pages/update",
            {"pageId": page["id"], "content": {"type": "doc", "content": [node]}, "format": "json", "operation": insert},
        )
        out["inserted"] = insert
    elif insert != "none":
        raise ValueError("insert: append, prepend or none")
    if node["type"] == "image":
        out["markdown"] = f"![{args.get('alt') or ''}]({src})"
    return out


PAGE_ID = {"type": "string", "description": "Page id (UUID) or slugId (last part of a page URL)"}
MARKDOWN = {
    "type": "string",
    "description": (
        "Markdown (GFM). Single newlines become line breaks. Also: task lists, tables, "
        "callouts (:::info / :::warning / :::danger / :::success … :::), math ($…$, $$…$$), "
        "<details><summary>, footnotes ([^1])."
    ),
}
DROP_UNSUPPORTED = {
    "type": "boolean",
    "description": (
        "Rewrite even though the page has content Markdown cannot carry (mentions, attachments, "
        "diagrams, comments, colours, …); that content is deleted. get_page lists it as notShownInMarkdown."
    ),
}

TOOLS = [
    {
        "name": "list_spaces",
        "title": "List spaces",
        "description": "List the Docmost spaces you can see, with your role in each.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": True},
        "fn": tool_list_spaces,
    },
    {
        "name": "search_pages",
        "title": "Search pages",
        "description": (
            "Full-text search over Docmost pages (titles and content, English stemming). "
            "Returns page ids, titles, spaces, URLs and highlighted matches."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search words"},
                "spaceId": {"type": "string", "description": "Only search this space"},
                "titleOnly": {"type": "boolean", "description": "Match page titles only (substring)"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 20},
            },
            "required": ["query"],
        },
        "annotations": {"readOnlyHint": True},
        "fn": tool_search,
    },
    {
        "name": "list_pages",
        "title": "List pages",
        "description": (
            "Page tree in sidebar order: the top-level pages of a space, or the children of a page. "
            "depth > 1 nests children (up to 300 pages)."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "spaceId": {"type": "string", "description": "Space id: list its top-level pages"},
                "parentPageId": {**PAGE_ID, "description": "List the children of this page instead"},
                "depth": {"type": "integer", "minimum": 1, "maximum": 5, "default": 1},
            },
        },
        "annotations": {"readOnlyHint": True},
        "fn": tool_list_pages,
    },
    {
        "name": "get_page",
        "title": "Read page",
        "description": (
            "Read a page: metadata (title, space, URL, canEdit) as JSON, then the body as Markdown. "
            "The Markdown can be edited with edit_page or written back with update_page unchanged. "
            "notShownInMarkdown lists content (mentions, attachments, diagrams, comments, …) the Markdown leaves out."
        ),
        "inputSchema": {"type": "object", "properties": {"pageId": PAGE_ID}, "required": ["pageId"]},
        "annotations": {"readOnlyHint": True},
        "fn": tool_get_page,
    },
    {
        "name": "create_page",
        "title": "Create page",
        "description": "Create a page in a space, optionally under a parent page, with a Markdown body.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "spaceId": {"type": "string", "description": "Space id (from list_spaces)"},
                "title": {"type": "string"},
                "content": MARKDOWN,
                "parentPageId": {**PAGE_ID, "description": "Create as a child of this page (same space)"},
            },
            "required": ["spaceId", "title"],
        },
        "annotations": {"readOnlyHint": False, "destructiveHint": False},
        "fn": tool_create_page,
    },
    {
        "name": "edit_page",
        "title": "Edit page",
        "description": (
            "Change parts of a page by exact find-and-replace on its Markdown (as get_page returns it). "
            "Each oldText must match exactly once unless replaceAll; all edits apply or none. "
            "Plain-text edits (no Markdown syntax, within one formatting run) change only that text and keep "
            "comments, mentions and other content; edits with Markdown rewrite the page. "
            "Prefer this over update_page for changes to existing pages."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "pageId": PAGE_ID,
                "edits": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "oldText": {"type": "string", "description": "Exact Markdown to find"},
                            "newText": {"type": "string", "description": "Replacement Markdown"},
                            "replaceAll": {"type": "boolean", "default": False},
                        },
                        "required": ["oldText", "newText"],
                    },
                },
                "dropUnsupported": DROP_UNSUPPORTED,
            },
            "required": ["pageId", "edits"],
        },
        "annotations": {"readOnlyHint": False, "destructiveHint": True},
        "fn": tool_edit_page,
    },
    {
        "name": "update_page",
        "title": "Update page",
        "description": (
            "Set a page's title and/or body. The Markdown body replaces the whole page (operation replace, "
            "the default) or is added at the end / start (append / prepend, which keep everything else)."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "pageId": PAGE_ID,
                "title": {"type": "string"},
                "content": MARKDOWN,
                "operation": {"type": "string", "enum": ["replace", "append", "prepend"], "default": "replace"},
                "dropUnsupported": DROP_UNSUPPORTED,
            },
            "required": ["pageId"],
        },
        "annotations": {"readOnlyHint": False, "destructiveHint": True},
        "fn": tool_update_page,
    },
    {
        "name": "move_page",
        "title": "Move page",
        "description": (
            "Move a page (with its children) under another page, to the top level (parentPageId null), "
            "and/or to another space. It is placed last among its new siblings."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "pageId": PAGE_ID,
                "parentPageId": {
                    "type": ["string", "null"],
                    "description": "New parent page (id or slugId); null for the top level of the space",
                },
                "spaceId": {"type": "string", "description": "Target space id, to move across spaces"},
            },
            "required": ["pageId"],
        },
        "annotations": {"readOnlyHint": False, "destructiveHint": False},
        "fn": tool_move_page,
    },
    {
        "name": "delete_page",
        "title": "Delete page",
        "description": "Move a page (and its children) to the space trash. It can be restored in the Docmost UI.",
        "inputSchema": {"type": "object", "properties": {"pageId": PAGE_ID}, "required": ["pageId"]},
        "annotations": {"readOnlyHint": False, "destructiveHint": True},
        "fn": tool_delete_page,
    },
    {
        "name": "upload_file",
        "title": "Upload file",
        "description": (
            "Upload a file to a page and add it at the end (default) or start: images show inline, "
            "video and audio as players, anything else as a file attachment. Give the file as url "
            "(public http(s), fetched by the server), base64, or text (UTF-8, e.g. a CSV). "
            f"Up to {MAX_FILE // (1024 * 1024)} MB. "
            "insert none only uploads; for an image the result's markdown can then be placed "
            "with edit_page."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "pageId": PAGE_ID,
                "fileName": {
                    "type": "string",
                    "description": "Name with extension (sets the type); defaults to the url's file name",
                },
                "url": {"type": "string", "description": "Public http(s) URL to fetch the file from"},
                "base64": {"type": "string", "description": "File content, base64 (a data: URL works too)"},
                "text": {"type": "string", "description": "File content as UTF-8 text"},
                "mimeType": {"type": "string", "description": "Override the type guessed from fileName / the url"},
                "alt": {"type": "string", "description": "Alt text for an image"},
                "insert": {"type": "string", "enum": ["append", "prepend", "none"], "default": "append"},
            },
            "required": ["pageId"],
        },
        "annotations": {"readOnlyHint": False, "destructiveHint": False, "openWorldHint": True},
        "fn": tool_upload_file,
    },
]
TOOLS_BY_NAME = {t["name"]: t for t in TOOLS}


def docmost_message(e):
    try:
        msg = json.loads(e.body).get("message")
        msg = ", ".join(msg) if isinstance(msg, list) else msg
    except (ValueError, AttributeError):
        msg = None
    return f"Docmost answered {e.status}: {msg or e.body[:200] or 'no details'}"


def rpc(email, msg):
    """Answer one JSON-RPC message; None for notifications."""
    if not isinstance(msg, dict) or msg.get("jsonrpc") != "2.0" or not isinstance(msg.get("method"), str):
        return {"jsonrpc": "2.0", "id": None, "error": {"code": -32600, "message": "Invalid Request"}}
    if "id" not in msg:
        return None
    mid, method, params = msg["id"], msg["method"], msg.get("params") or {}

    def ok(result):
        return {"jsonrpc": "2.0", "id": mid, "result": result}

    if method == "initialize":
        asked = params.get("protocolVersion")
        return ok(
            {
                "protocolVersion": asked if asked in PROTOCOL_VERSIONS else PROTOCOL_VERSIONS[0],
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "neo-docmost", "title": "Docmost", "version": "1.2.0"},
                "instructions": (
                    f"Docmost wiki at {PUBLIC_URL}, used as {email}; Docmost permissions apply. "
                    "Find pages with search_pages or list_spaces + list_pages, read them with get_page (Markdown). "
                    "For changes to an existing page use edit_page (find/replace on that Markdown); "
                    "update_page replaces the whole body, or appends/prepends. Pages with content Markdown "
                    "cannot carry (get_page: notShownInMarkdown) refuse whole-body rewrites unless dropUnsupported. "
                    "Add images and other files with upload_file (Markdown cannot embed them)."
                ),
            }
        )
    if method == "ping":
        return ok({})
    if method == "tools/list":
        return ok({"tools": [{k: v for k, v in t.items() if k != "fn"} for t in TOOLS]})
    if method == "tools/call":
        tool = TOOLS_BY_NAME.get(params.get("name"))
        if tool is None:
            return {"jsonrpc": "2.0", "id": mid, "error": {"code": -32602, "message": f"Unknown tool: {params.get('name')}"}}
        try:
            result = tool["fn"](email, params.get("arguments") or {})
        except DocmostError as e:
            return ok({"content": [{"type": "text", "text": docmost_message(e)}], "isError": True})
        except (KeyError, ValueError, TypeError) as e:
            text = f"missing argument {e}" if isinstance(e, KeyError) else str(e)
            return ok({"content": [{"type": "text", "text": text}], "isError": True})
        except OSError as e:
            log(f"{tool['name']} for {email}: {e!r}")
            return ok({"content": [{"type": "text", "text": "Docmost is not reachable"}], "isError": True})
        if isinstance(result, Markdown):
            content = [
                {"type": "text", "text": json.dumps(result.meta, ensure_ascii=False, indent=1)},
                {"type": "text", "text": result.body},
            ]
        else:
            content = [{"type": "text", "text": json.dumps(result, ensure_ascii=False, indent=1)}]
        return ok({"content": content})
    return {"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": f"Method not found: {method}"}}


# --- HTTP ----------------------------------------------------------------------


class Handler(BaseHTTPRequestHandler):
    def do_OPTIONS(self):
        self.send_response(204)
        self.cors()
        self.send_header("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")
        self.send_header(
            "Access-Control-Allow-Headers",
            "Authorization, Content-Type, Accept, Mcp-Session-Id, Mcp-Protocol-Version, Last-Event-ID",
        )
        self.send_header("Access-Control-Max-Age", "86400")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        path = urllib.parse.urlsplit(self.path).path
        if path in ("/.well-known/oauth-protected-resource", "/.well-known/oauth-protected-resource/mcp"):
            return self.json(200, resource_metadata())
        if path == "/.well-known/oauth-authorization-server":
            return self.json(200, server_metadata())
        if path == "/mcp/oauth/authorize":
            return self.authorize(dict(urllib.parse.parse_qsl(urllib.parse.urlsplit(self.path).query)))
        if path == "/mcp":
            # No server-initiated stream: every answer comes back on the POST.
            return self.json(405, {"error": "use POST"}, [("Allow", "POST, DELETE")])
        self.json(404, {"error": "not found"})

    def do_DELETE(self):
        # Stateless: there is no session to end.
        self.json(405, {"error": "no sessions"}, [("Allow", "POST")])

    def do_POST(self):
        path = urllib.parse.urlsplit(self.path).path
        length = int(self.headers.get("Content-Length") or 0)
        if length > MAX_BODY:
            return self.json(413, {"error": "request too large"})
        raw = self.rfile.read(length).decode("utf-8", "replace")

        if path == "/mcp/oauth/authorize":
            return self.authorize(dict(urllib.parse.parse_qsl(raw)), post=True)
        if path == "/mcp/oauth/token":
            ctype = self.headers.get("Content-Type", "")
            form = json_or_empty(raw) if "json" in ctype else dict(urllib.parse.parse_qsl(raw))
            status, body = token(form)
            return self.json(status, body, [("Pragma", "no-cache")])
        if path == "/mcp/oauth/register":
            return self.json(*register(json_or_empty(raw)))
        if path == "/mcp":
            return self.mcp(raw)
        self.json(404, {"error": "not found"})

    def mcp(self, raw):
        auth = self.headers.get("Authorization", "")
        data = verify("access", auth[7:]) if auth[:7].lower() == "bearer " else None
        if not data:
            challenge = f'Bearer resource_metadata="{RESOURCE_METADATA}", scope="docmost"'
            if auth:
                challenge += ', error="invalid_token"'
            return self.json(401, {"error": "unauthorized"}, [("WWW-Authenticate", challenge)])
        try:
            msg = json.loads(raw)
        except ValueError:
            return self.json(400, {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "Parse error"}})
        if isinstance(msg, list):
            answers = [a for a in (rpc(data["sub"], m) for m in msg) if a is not None]
        else:
            answers = rpc(data["sub"], msg)
        if not answers:
            return self.reply(202, b"", "application/json")
        self.json(200, answers)

    def authorize(self, params, post=False):
        # Only SWAG (after tinyauth) sets the key and the Remote-* headers.
        if not hmac.compare_digest(self.headers.get("X-Neo-Sso-Key", ""), sso.SWAG_KEY):
            return self.html(403, sso.page("Forbidden", "This endpoint is only reachable through SWAG."))
        client = verify("client", params.get("client_id"))
        uri = params.get("redirect_uri", "")
        if not client or not redirect_matches(client["r"], uri):
            # Never redirect to an unverified URI.
            return self.html(400, sso.page("Unknown client", "Unknown client_id or redirect_uri. Reconnect the MCP client."))

        def back(**query):
            if params.get("state"):
                query["state"] = params["state"]
            return self.reply(302, b"", headers=[("Location", with_query(uri, query))])

        if params.get("response_type") != "code":
            return back(error="unsupported_response_type")
        if params.get("code_challenge_method") != "S256" or not params.get("code_challenge"):
            return back(error="invalid_request", error_description="PKCE with S256 is required")

        username = self.headers.get("Remote-User", "").strip()
        email = self.headers.get("Remote-Email", "").strip().lower()
        if not email and username:
            email = username.lower() if "@" in username else f"{username.lower()}@{sso.MAIL_DOMAIN}"
        if not username or not email or email == sso.SERVICE_EMAIL:
            return self.html(403, sso.page("No user", "Tinyauth did not pass a usable user."))

        keep = ("response_type", "client_id", "redirect_uri", "state", "code_challenge", "code_challenge_method", "scope", "resource")
        request = {k: params[k] for k in keep if params.get(k)}
        if not post:
            consent = sign("consent", {"sub": email, "q": request}, CONSENT_TTL)
            return self.html(200, consent_page(client, request, email, consent))

        consent = verify("consent", params.get("consent"))
        if not consent or consent["sub"] != email or consent["q"] != request:
            return self.html(400, sso.page("Expired", "This approval expired. Start the connection again from the MCP client."))
        if params.get("decision") != "allow":
            return back(error="access_denied")

        # Same as a browser sign-in: create the account / sync the role.
        name = self.headers.get("Remote-Name", "").strip() or username
        name = name if len(name) >= 2 else email.split("@")[0]
        groups = [g.strip() for g in self.headers.get("Remote-Groups", "").split(",") if g.strip()]
        try:
            sso.sign_in(username, email, name[:60], groups, self.headers.get("X-Real-IP") or None)
            member = sso.find_member(email)
        except PermissionError as e:
            return self.html(403, sso.page("Access denied", str(e)))
        except Exception as e:  # noqa: BLE001 — always answer with a page
            log(f"mcp sign-in failed for {email}: {e!r}")
            return self.html(502, sso.page("Sign-in failed", "Docmost did not accept the sign-in. Check the docmost-mcp logs."))
        # Docmost matches the stored spelling of the address.
        email = (member or {}).get("email") or email
        log(f"authorized {client['n']!r} for {email}")
        code = sign(
            "code",
            {"sub": email, "cid": params["client_id"], "ru": uri, "cc": params["code_challenge"]},
            CODE_TTL,
        )
        back(code=code)

    def cors(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Expose-Headers", "WWW-Authenticate, Mcp-Session-Id")

    def json(self, status, body, headers=()):
        self.reply(status, json.dumps(body, ensure_ascii=False).encode(), "application/json", headers)

    def html(self, status, body):
        # The consent page must not be framed (clickjacking the Allow button).
        self.reply(
            status,
            body.encode(),
            "text/html; charset=utf-8",
            [("X-Frame-Options", "DENY"), ("Content-Security-Policy", "frame-ancestors 'none'")],
        )

    def reply(self, status, data, ctype=None, headers=()):
        self.send_response(status)
        for k, v in headers:
            self.send_header(k, v)
        self.cors()
        self.send_header("Cache-Control", "no-store")
        if ctype:
            self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, fmt, *args):
        # Paths only: query strings carry codes and states.
        log(f"{self.address_string()} {self.command} {urllib.parse.urlsplit(self.path).path} {args[1] if len(args) > 1 else ''}")


def json_or_empty(raw):
    try:
        data = json.loads(raw)
    except ValueError:
        return {}
    return data if isinstance(data, dict) else {}


if __name__ == "__main__":
    log(f"docmost-mcp listening on :{PORT}, resource {RESOURCE}")
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
