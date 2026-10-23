"""Tinyauth single sign-on bridge for Docmost (community edition).

Docmost CE only knows email + password. SWAG sends `/login` here after
tinyauth, with the user in Remote-User / Remote-Email / Remote-Name /
Remote-Groups. The bridge then:

  1. invites + accepts new users through the Docmost API (members join the
     default "Everyone" group, so they can edit the shared spaces),
  2. syncs the workspace role from ADMINS / ADMIN_GROUPS,
  3. logs the user in and redirects with Docmost's own authToken cookie.

Each user's password is HMAC(SSO_KEY, email): nobody types or sees it, and
SWAG blocks Docmost's password endpoints. The service account that does the
invites is created by `setup` (fresh workspace) or by docmost-sso-setup in SQL.
Stdlib only; runs in a plain python image.

Usage: sso.py            serve on PORT
       sso.py setup      create the workspace if Docmost has none (idempotent)
"""

import hashlib
import hmac
import html
import http.client
import json
import os
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock

PORT = int(os.environ.get("PORT", "8080"))
DOCMOST = os.environ.get("DOCMOST_URL", "http://docmost:3000")
SSO_KEY = os.environ["SSO_KEY"]
SWAG_KEY = os.environ["SWAG_KEY"]
SERVICE_EMAIL = os.environ["SERVICE_EMAIL"].lower()
MAIL_DOMAIN = os.environ.get("MAIL_DOMAIN", "")
LOGOUT_URL = os.environ.get("LOGOUT_URL", "")
WORKSPACE_NAME = os.environ.get("WORKSPACE_NAME", "Docmost")
ADMINS = {a.strip().lower() for a in os.environ.get("ADMINS", "").split(",") if a.strip()}
ADMIN_GROUPS = {g.strip() for g in os.environ.get("ADMIN_GROUPS", "").split(",") if g.strip()}
TIMEOUT = 15


def log(msg):
    print(msg, file=sys.stderr, flush=True)


def password_for(email):
    # Same derivation as docmost-sso-setup (pgcrypto hmac + encode hex).
    return hmac.new(SSO_KEY.encode(), email.lower().encode(), hashlib.sha256).hexdigest()


class DocmostError(Exception):
    def __init__(self, status, body):
        super().__init__(f"docmost {status}: {body[:300]}")
        self.status = status
        self.body = body


def call(path, body, token=None, client_ip=None):
    """POST JSON to the Docmost API. Returns (data, Set-Cookie headers)."""
    url = urllib.parse.urlsplit(DOCMOST)
    conn = http.client.HTTPConnection(url.hostname, url.port or 80, timeout=TIMEOUT)
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if client_ip:
        # Docmost trusts the proxy; its auth throttle then counts per user IP.
        headers["X-Forwarded-For"] = client_ip
    try:
        conn.request("POST", f"/api{path}", json.dumps(body), headers)
        res = conn.getresponse()
        raw = res.read().decode("utf-8", "replace")
        cookies = res.headers.get_all("Set-Cookie") or []
    finally:
        conn.close()
    if res.status >= 400:
        raise DocmostError(res.status, raw)
    data = json.loads(raw).get("data") if raw else None
    return data, cookies


def auth_token(cookies):
    for c in cookies:
        name, _, rest = c.partition("=")
        if name.strip() == "authToken":
            return rest.split(";", 1)[0]
    raise DocmostError(500, "no authToken cookie in Docmost response")


def login(email, client_ip=None):
    _, cookies = call(
        "/auth/login", {"email": email, "password": password_for(email)}, client_ip=client_ip
    )
    return cookies


class Service:
    """Docmost session of the service account, renewed on 401."""

    def __init__(self):
        self.lock = Lock()
        self.token = None

    def call(self, path, body):
        with self.lock:
            if self.token is None:
                self.token = auth_token(login(SERVICE_EMAIL))
            token = self.token
        try:
            return call(path, body, token)[0]
        except DocmostError as e:
            if e.status != 401:
                raise
            with self.lock:
                self.token = auth_token(login(SERVICE_EMAIL))
                token = self.token
            return call(path, body, token)[0]

    def paged(self, path, body=None):
        cursor = None
        while True:
            page = self.call(path, {**(body or {}), "limit": 100, **({"cursor": cursor} if cursor else {})})
            yield from page.get("items", [])
            cursor = (page.get("meta") or {}).get("nextCursor")
            if not cursor:
                return


service = Service()


def find_member(email):
    for user in service.paged("/workspace/members", {"query": email}):
        if (user.get("email") or "").lower() == email:
            return user
    return None


def has_human_members():
    for user in service.paged("/workspace/members"):
        if (user.get("email") or "").lower() != SERVICE_EMAIL:
            return True
    return False


def wanted_role(username, email, groups):
    """'admin' / 'member', or None to leave roles to the Docmost UI."""
    if not ADMINS and not ADMIN_GROUPS:
        return None
    if username.lower() in ADMINS or email in ADMINS or ADMIN_GROUPS & set(groups):
        return "admin"
    return "member"


def create_member(email, name, role, client_ip):
    service.call("/workspace/invites/create", {"emails": [email], "role": role})
    invite = next(
        (i for i in service.paged("/workspace/invites") if (i.get("email") or "").lower() == email),
        None,
    )
    if invite is None:
        raise DocmostError(500, f"invitation for {email} not found after create")
    link = service.call("/workspace/invites/link", {"invitationId": invite["id"]})["inviteLink"]
    token = urllib.parse.parse_qs(urllib.parse.urlsplit(link).query)["token"][0]
    _, cookies = call(
        "/workspace/invites/accept",
        {
            "invitationId": invite["id"],
            "token": token,
            "name": name,
            "password": password_for(email),
        },
        client_ip=client_ip,
    )
    if invite.get("role") != role:
        # An older pending invite (onConflict doNothing) keeps its own role.
        user = find_member(email)
        service.call("/workspace/members/change-role", {"userId": user["id"], "role": role})
    log(f"created {email} as {role}")
    return cookies


def sign_in(username, email, name, groups, client_ip):
    """Return the Set-Cookie headers of a Docmost session for this user."""
    role = wanted_role(username, email, groups)
    user = find_member(email)
    if user is None:
        # No admin rules: the first person to sign in administers the workspace.
        if role is None:
            role = "member" if has_human_members() else "admin"
        return create_member(email, name, role, client_ip)
    if user.get("deactivatedAt"):
        raise PermissionError("Your Docmost account is deactivated. Ask a workspace admin.")
    if role and user.get("role") not in ("owner", role):
        service.call("/workspace/members/change-role", {"userId": user["id"], "role": role})
        log(f"{email}: role {user.get('role')} -> {role}")
    # Docmost matches the stored spelling of the address.
    return login(user["email"], client_ip)


def safe_redirect(target):
    # Same-origin paths only; "//host" and "/\host" would leave the site.
    if not target or not target.startswith("/") or target.startswith(("//", "/\\")):
        return "/home"
    return target


def page(title, message):
    return (
        "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>"
        f"<title>{html.escape(title)}</title>"
        "<body style='font-family:system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem'>"
        f"<h1>{html.escape(title)}</h1><p>{html.escape(message)}</p>"
        "<p><a href='/login'>Try again</a></p>"
    )


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if not hmac.compare_digest(self.headers.get("X-Neo-Sso-Key", ""), SWAG_KEY):
            return self.reply(403, page("Forbidden", "This endpoint is only reachable through SWAG."))
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
        if "logout" in query and LOGOUT_URL:
            # Docmost's logout lands on /login?logout=1; end the tinyauth session too,
            # or the next /login would sign straight back in.
            return self.reply(302, headers=[("Location", LOGOUT_URL)])

        username = self.headers.get("Remote-User", "").strip()
        email = self.headers.get("Remote-Email", "").strip().lower()
        if not email and username:
            email = username.lower() if "@" in username else f"{username.lower()}@{MAIL_DOMAIN}"
        name = self.headers.get("Remote-Name", "").strip() or username
        name = name if len(name) >= 2 else email.split("@")[0]
        groups = [g.strip() for g in self.headers.get("Remote-Groups", "").split(",") if g.strip()]
        client_ip = self.headers.get("X-Real-IP") or None
        if not username or not email or email == SERVICE_EMAIL:
            return self.reply(403, page("No user", "Tinyauth did not pass a usable user."))

        try:
            cookies = sign_in(username, email, name[:60], groups, client_ip)
        except PermissionError as e:
            return self.reply(403, page("Access denied", str(e)))
        except Exception as e:  # noqa: BLE001 — always answer with a page
            log(f"sign-in failed for {email}: {e!r}")
            return self.reply(502, page("Sign-in failed", "Docmost did not accept the sign-in. Check the docmost-sso logs."))

        target = safe_redirect(query.get("redirect", [""])[0])
        self.reply(302, headers=[("Location", target)] + [("Set-Cookie", c) for c in cookies])

    def reply(self, status, body="", headers=()):
        data = body.encode()
        self.send_response(status)
        for k, v in headers:
            self.send_header(k, v)
        self.send_header("Cache-Control", "no-store")
        if data:
            self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, fmt, *args):
        log(f"{self.address_string()} {fmt % args}")


def setup():
    """Create the workspace with the service account as owner (fresh Docmost)."""
    try:
        call(
            "/auth/setup",
            {
                "name": "Neo SSO",
                "email": SERVICE_EMAIL,
                "password": password_for(SERVICE_EMAIL),
                "workspaceName": WORKSPACE_NAME,
            },
        )
        log("created workspace with the SSO service account as owner")
    except DocmostError as e:
        if e.status != 403:
            raise
        log("workspace already set up")


if __name__ == "__main__":
    if sys.argv[1:] == ["setup"]:
        try:
            setup()
        except (DocmostError, OSError) as e:
            sys.exit(f"docmost not ready: {e}")
    else:
        log(f"docmost-sso listening on :{PORT}")
        ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
