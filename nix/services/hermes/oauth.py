#!/usr/bin/env python3
"""Hermes OAuth helper for the Neo providerAuth widget.

Stdin JSON: {action, provider, session, code}
Stdout JSON only. Never prints access tokens.

Actions: status | login | poll | submit | refresh | poll-worker
"""
from __future__ import annotations

import json
import os
import re
import secrets
import subprocess
import sys
import time
import traceback
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SESSION_TTL = 15 * 60
PROVIDER_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")
SESSION_RE = re.compile(r"^[A-Za-z0-9_-]{8,64}$")


def _out(payload: dict[str, Any], exit_code: int = 0) -> None:
    sys.stdout.write(json.dumps(payload, default=str) + "\n")
    sys.stdout.flush()
    raise SystemExit(exit_code)


def _ok(**kwargs: Any) -> None:
    payload: dict[str, Any] = {"ok": True}
    payload.update(kwargs)
    _out(payload, 0)


def _err(message: str, **kwargs: Any) -> None:
    payload: dict[str, Any] = {"ok": False, "error": message}
    payload.update(kwargs)
    _out(payload, 1)


def _hermes_home() -> Path:
    raw = os.environ.get("HERMES_HOME") or ""
    if raw:
        return Path(raw)
    return Path.home() / ".hermes"


def _session_dir() -> Path:
    d = _hermes_home() / "neo-oauth"
    d.mkdir(parents=True, mode=0o700, exist_ok=True)
    return d


def _session_path(sid: str) -> Path:
    if not SESSION_RE.match(sid):
        _err("invalid session id")
    return _session_dir() / f"{sid}.json"


def _read_session(sid: str) -> dict[str, Any]:
    path = _session_path(sid)
    if not path.is_file():
        _err("session not found or expired", status="expired")
    try:
        data = json.loads(path.read_text())
    except Exception:
        _err("session unreadable")
    if not isinstance(data, dict):
        _err("session corrupt")
    created = float(data.get("created_at") or 0)
    if created and (time.time() - created) > SESSION_TTL:
        try:
            path.unlink()
        except OSError:
            pass
        _err("session expired", status="expired")
    return data


def _write_session(data: dict[str, Any]) -> None:
    sid = str(data.get("session_id") or "")
    path = _session_path(sid)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, default=str))
    os.chmod(tmp, 0o600)
    tmp.replace(path)


def _public_status(raw: dict[str, Any]) -> dict[str, Any]:
    """Strip secrets. Never forward access tokens / api keys."""
    logged_in = bool(raw.get("logged_in") or raw.get("configured"))
    source = raw.get("source") or raw.get("source_label") or raw.get("key_source")
    label = None
    if isinstance(source, str) and source.startswith("pool:"):
        label = source.split(":", 1)[1]
    expires = raw.get("expires_at") or raw.get("access_expires_at")
    return {
        "logged_in": logged_in,
        "source": source if isinstance(source, str) else None,
        "label": label,
        "expires_at": expires,
        "has_refresh_token": bool(raw.get("has_refresh_token") or raw.get("refresh_token")),
        "auth_mode": raw.get("auth_mode") or raw.get("auth_type"),
        "error": raw.get("error") if not logged_in else None,
        "rate_limited": bool(raw.get("rate_limited")),
    }


def do_status(provider: str) -> None:
    try:
        from hermes_cli.auth import get_auth_status
    except ImportError:
        _err("Hermes Python libraries are not available on this host.")
    try:
        raw = get_auth_status(provider) or {}
    except Exception as exc:
        _err(str(exc) or "status failed")
    if not isinstance(raw, dict):
        raw = {"logged_in": False}
    _ok(provider=provider, status=_public_status(raw))


def do_refresh(provider: str) -> None:
    try:
        from hermes_cli import auth as hauth
    except ImportError:
        _err("Hermes Python libraries are not available on this host.")
    try:
        if provider == "openai-codex":
            hauth.resolve_codex_runtime_credentials()
        elif provider == "xai-oauth":
            hauth.resolve_xai_oauth_runtime_credentials()
        elif provider == "nous":
            hauth.get_nous_auth_status()
        elif provider == "qwen-oauth":
            hauth.resolve_qwen_runtime_credentials(refresh_if_expiring=True)
        elif provider == "minimax-oauth":
            hauth.get_minimax_oauth_auth_status()
        elif provider == "anthropic":
            # PKCE file + pool: status path is enough; there is no separate refresh CLI.
            hauth.get_auth_status(provider)
        else:
            hauth.get_auth_status(provider)
    except Exception as exc:
        _err(str(exc) or "refresh failed — try Log in again")
    do_status(provider)


def _new_session(provider: str, flow: str) -> dict[str, Any]:
    sid = secrets.token_urlsafe(16)
    data = {
        "session_id": sid,
        "provider": provider,
        "flow": flow,
        "status": "pending",
        "created_at": time.time(),
        "error_message": None,
    }
    _write_session(data)
    return data


def _spawn_worker(session_id: str) -> None:
    env = os.environ.copy()
    env["PYTHONUNBUFFERED"] = "1"
    proc = subprocess.Popen(
        [sys.executable, os.path.abspath(__file__)],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
        env=env,
    )
    assert proc.stdin is not None
    proc.stdin.write(json.dumps({"action": "poll-worker", "session": session_id}).encode())
    proc.stdin.close()


def login_anthropic(provider: str) -> None:
    try:
        from agent.anthropic_adapter import (
            _OAUTH_CLIENT_ID,
            _OAUTH_REDIRECT_URI,
            _OAUTH_SCOPES,
            _generate_pkce,
        )
    except ImportError:
        _err("Anthropic OAuth adapter is not available")
    verifier, challenge = _generate_pkce()
    sess = _new_session(provider, "pkce")
    sess["verifier"] = verifier
    sess["state"] = verifier
    _write_session(sess)
    from urllib.parse import urlencode

    params = {
        "code": "true",
        "client_id": _OAUTH_CLIENT_ID,
        "response_type": "code",
        "redirect_uri": _OAUTH_REDIRECT_URI,
        "scope": _OAUTH_SCOPES,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
        "state": verifier,
    }
    auth_url = f"https://claude.ai/oauth/authorize?{urlencode(params)}"
    _ok(
        session_id=sess["session_id"],
        flow="pkce",
        auth_url=auth_url,
        expires_in=SESSION_TTL,
    )


def login_nous(provider: str) -> None:
    import httpx
    from hermes_cli.auth import PROVIDER_REGISTRY, _request_device_code

    pconfig = PROVIDER_REGISTRY["nous"]
    portal_base_url = (
        os.getenv("HERMES_PORTAL_BASE_URL")
        or os.getenv("NOUS_PORTAL_BASE_URL")
        or pconfig.portal_base_url
    ).rstrip("/")
    client_id = pconfig.client_id
    scope = pconfig.scope
    with httpx.Client(timeout=httpx.Timeout(15.0), headers={"Accept": "application/json"}) as client:
        device_data = _request_device_code(
            client=client,
            portal_base_url=portal_base_url,
            client_id=client_id,
            scope=scope,
        )
    sess = _new_session(provider, "device_code")
    sess.update(
        {
            "device_code": str(device_data["device_code"]),
            "interval": int(device_data["interval"]),
            "expires_at": time.time() + int(device_data["expires_in"]),
            "portal_base_url": portal_base_url,
            "client_id": client_id,
            "scope": scope,
            "user_code": str(device_data["user_code"]),
            "verification_url": str(device_data["verification_uri_complete"]),
        }
    )
    _write_session(sess)
    _spawn_worker(sess["session_id"])
    _ok(
        session_id=sess["session_id"],
        flow="device_code",
        user_code=sess["user_code"],
        verification_url=sess["verification_url"],
        expires_in=int(device_data["expires_in"]),
        poll_interval=int(device_data["interval"]),
    )


def login_xai(provider: str) -> None:
    import httpx
    from hermes_cli.auth import _xai_oauth_request_device_code

    with httpx.Client(timeout=httpx.Timeout(20.0), headers={"Accept": "application/json"}) as client:
        device_data = _xai_oauth_request_device_code(client)
    sess = _new_session(provider, "device_code")
    verification_url = str(
        device_data.get("verification_uri_complete") or device_data["verification_uri"]
    )
    sess.update(
        {
            "device_code": str(device_data["device_code"]),
            "interval": int(device_data["interval"]),
            "expires_at": time.time() + int(device_data["expires_in"]),
            "user_code": str(device_data["user_code"]),
            "verification_url": verification_url,
        }
    )
    _write_session(sess)
    _spawn_worker(sess["session_id"])
    _ok(
        session_id=sess["session_id"],
        flow="device_code",
        user_code=sess["user_code"],
        verification_url=verification_url,
        expires_in=int(device_data["expires_in"]),
        poll_interval=int(device_data["interval"]),
    )


def login_minimax(provider: str) -> None:
    import httpx
    from hermes_cli.auth import (
        MINIMAX_OAUTH_CLIENT_ID,
        MINIMAX_OAUTH_GLOBAL_BASE,
        _minimax_pkce_pair,
        _minimax_request_user_code,
    )

    verifier, challenge, state = _minimax_pkce_pair()
    portal_base_url = (os.getenv("MINIMAX_PORTAL_BASE_URL") or MINIMAX_OAUTH_GLOBAL_BASE).rstrip("/")
    with httpx.Client(
        timeout=httpx.Timeout(15.0),
        headers={"Accept": "application/json"},
        follow_redirects=True,
    ) as client:
        device_data = _minimax_request_user_code(
            client=client,
            portal_base_url=portal_base_url,
            client_id=MINIMAX_OAUTH_CLIENT_ID,
            code_challenge=challenge,
            state=state,
        )
    expired_in_raw = int(device_data["expired_in"])
    if expired_in_raw > 1_000_000_000_000:
        expires_at_ts = expired_in_raw / 1000.0
        expires_in_seconds = max(0, int(expires_at_ts - time.time()))
    else:
        expires_at_ts = time.time() + expired_in_raw
        expires_in_seconds = expired_in_raw
    interval_raw = device_data.get("interval")
    sess = _new_session(provider, "device_code")
    sess.update(
        {
            "user_code": str(device_data["user_code"]),
            "code_verifier": verifier,
            "state": state,
            "portal_base_url": portal_base_url,
            "client_id": MINIMAX_OAUTH_CLIENT_ID,
            "region": "global",
            "interval_ms": int(interval_raw) if interval_raw is not None else None,
            "expired_in_raw": expired_in_raw,
            "expires_at": expires_at_ts,
            "verification_url": str(device_data["verification_uri"]),
        }
    )
    _write_session(sess)
    _spawn_worker(sess["session_id"])
    _ok(
        session_id=sess["session_id"],
        flow="device_code",
        user_code=sess["user_code"],
        verification_url=sess["verification_url"],
        expires_in=expires_in_seconds,
        poll_interval=max(2, (sess["interval_ms"] or 2000) // 1000),
    )


def login_codex(provider: str) -> None:
    """Start Codex device-code in the worker; wait briefly for user_code."""
    sess = _new_session(provider, "device_code")
    _spawn_worker(sess["session_id"])
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        data = json.loads(_session_path(sess["session_id"]).read_text())
        if data.get("user_code") or data.get("status") != "pending":
            sess = data
            break
        time.sleep(0.15)
    if sess.get("status") == "error":
        _err(sess.get("error_message") or "device-auth failed")
    if not sess.get("user_code"):
        _err("device-auth timed out before returning a user code")
    _ok(
        session_id=sess["session_id"],
        flow="device_code",
        user_code=sess["user_code"],
        verification_url=sess.get("verification_url"),
        expires_in=int(sess.get("expires_in") or 900),
        poll_interval=int(sess.get("interval") or 5),
    )


def do_login(provider: str) -> None:
    if provider == "anthropic":
        login_anthropic(provider)
    elif provider == "nous":
        login_nous(provider)
    elif provider == "xai-oauth":
        login_xai(provider)
    elif provider == "minimax-oauth":
        login_minimax(provider)
    elif provider == "openai-codex":
        login_codex(provider)
    elif provider == "qwen-oauth":
        _err(
            "Qwen OAuth uses the Qwen CLI. On the homeserver run: "
            "sudo -u hermes env HERMES_HOME=$HERMES_HOME hermes auth add qwen-oauth",
            flow="external",
        )
    else:
        _err(
            f"In-browser OAuth is not implemented for {provider}. "
            f"On the homeserver run: sudo -u hermes env HERMES_HOME=$HERMES_HOME hermes auth add {provider}",
            flow="external",
        )


def do_poll(session_id: str) -> None:
    sess = _read_session(session_id)
    _ok(
        session_id=session_id,
        status=sess.get("status") or "pending",
        error_message=sess.get("error_message"),
        flow=sess.get("flow"),
        user_code=sess.get("user_code"),
        verification_url=sess.get("verification_url"),
        auth_url=sess.get("auth_url"),
    )


def do_submit(session_id: str, code_input: str) -> None:
    sess = _read_session(session_id)
    if sess.get("provider") != "anthropic" or sess.get("flow") != "pkce":
        _err("submit is only for Anthropic PKCE sessions")
    if sess.get("status") != "pending":
        _ok(status=sess.get("status"), message=sess.get("error_message"))
    parts = (code_input or "").strip().split("#", 1)
    code = parts[0].strip()
    if not code:
        _err("No code provided")
    state_from_callback = parts[1] if len(parts) > 1 else ""
    try:
        from agent.anthropic_adapter import (
            _OAUTH_CLIENT_ID,
            _OAUTH_REDIRECT_URI,
            _OAUTH_TOKEN_URLS,
        )
    except ImportError:
        _err("Anthropic OAuth adapter is not available")
    import urllib.request

    exchange_data = json.dumps(
        {
            "grant_type": "authorization_code",
            "client_id": _OAUTH_CLIENT_ID,
            "code": code,
            "state": state_from_callback or sess.get("state"),
            "redirect_uri": _OAUTH_REDIRECT_URI,
            "code_verifier": sess.get("verifier"),
        }
    ).encode()
    result = None
    last_exc = None
    for endpoint in _OAUTH_TOKEN_URLS:
        req = urllib.request.Request(
            endpoint,
            data=exchange_data,
            headers={"Content-Type": "application/json", "User-Agent": "neo-hermes-auth/1.0"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=20) as resp:
                result = json.loads(resp.read().decode())
            break
        except Exception as exc:
            last_exc = exc
            continue
    if result is None:
        sess["status"] = "error"
        sess["error_message"] = f"Token exchange failed: {last_exc}"
        _write_session(sess)
        _err(sess["error_message"], status="error")
    access_token = result.get("access_token", "")
    refresh_token = result.get("refresh_token", "")
    expires_in = int(result.get("expires_in") or 3600)
    if not access_token:
        sess["status"] = "error"
        sess["error_message"] = "No access token returned"
        _write_session(sess)
        _err(sess["error_message"], status="error")
    expires_at_ms = int(time.time() * 1000) + (expires_in * 1000)
    try:
        from agent.anthropic_adapter import _get_hermes_oauth_file
        from utils import atomic_json_write

        oauth_file = _get_hermes_oauth_file()
        atomic_json_write(
            oauth_file,
            {
                "accessToken": access_token,
                "refreshToken": refresh_token,
                "expiresAt": expires_at_ms,
            },
            indent=2,
            mode=0o600,
        )
        from agent.credential_pool import AUTH_TYPE_OAUTH, SOURCE_MANUAL, PooledCredential, load_pool
        import uuid

        pool = load_pool("anthropic")
        existing = [
            e
            for e in pool.entries()
            if str(getattr(e, "source", "")).startswith(f"{SOURCE_MANUAL}:dashboard_pkce")
            or str(getattr(e, "source", "")).startswith(f"{SOURCE_MANUAL}:neo_pkce")
        ]
        for e in existing:
            try:
                pool.remove_entry(e.id)
            except Exception:
                pass
        pool.add_entry(
            PooledCredential(
                provider="anthropic",
                id=uuid.uuid4().hex[:6],
                label="neo-pkce",
                auth_type=AUTH_TYPE_OAUTH,
                priority=0,
                source=f"{SOURCE_MANUAL}:neo_pkce",
                access_token=access_token,
                refresh_token=refresh_token,
                expires_at_ms=expires_at_ms,
            )
        )
    except Exception as exc:
        sess["status"] = "error"
        sess["error_message"] = f"Save failed: {exc}"
        _write_session(sess)
        _err(sess["error_message"], status="error")
    sess["status"] = "approved"
    _write_session(sess)
    _ok(status="approved")


def _set_error(sess: dict[str, Any], message: str) -> None:
    sess["status"] = "error"
    sess["error_message"] = message
    _write_session(sess)


def worker_nous(sess: dict[str, Any]) -> None:
    import httpx
    from hermes_cli.auth import _poll_for_token, persist_nous_credentials, refresh_nous_oauth_from_state

    portal_base_url = sess["portal_base_url"]
    client_id = sess["client_id"]
    device_code = sess["device_code"]
    interval = int(sess["interval"])
    scope = sess.get("scope")
    expires_in = max(60, int(sess["expires_at"] - time.time()))
    with httpx.Client(timeout=httpx.Timeout(15.0), headers={"Accept": "application/json"}) as client:
        token_data = _poll_for_token(
            client=client,
            portal_base_url=portal_base_url,
            client_id=client_id,
            device_code=device_code,
            expires_in=expires_in,
            poll_interval=interval,
        )
    now = datetime.now(timezone.utc)
    token_ttl = int(token_data.get("expires_in") or 0)
    auth_state = {
        "portal_base_url": portal_base_url,
        "inference_base_url": token_data.get("inference_base_url"),
        "client_id": client_id,
        "scope": token_data.get("scope") or scope,
        "token_type": token_data.get("token_type", "Bearer"),
        "access_token": token_data["access_token"],
        "refresh_token": token_data.get("refresh_token"),
        "obtained_at": now.isoformat(),
        "expires_at": (
            datetime.fromtimestamp(now.timestamp() + token_ttl, tz=timezone.utc).isoformat()
            if token_ttl
            else None
        ),
        "expires_in": token_ttl,
    }
    full_state = refresh_nous_oauth_from_state(auth_state, timeout_seconds=15.0, force_refresh=False)
    persist_nous_credentials(full_state)
    sess["status"] = "approved"
    _write_session(sess)


def worker_xai(sess: dict[str, Any]) -> None:
    import httpx
    from hermes_cli.auth import (
        _save_xai_oauth_tokens,
        _xai_oauth_discovery,
        _xai_oauth_poll_device_token,
        mark_provider_active_if_unset,
        unsuppress_credential_source,
    )

    device_code = sess["device_code"]
    interval = int(sess["interval"])
    expires_in = max(60, int(sess["expires_at"] - time.time()))
    discovery = _xai_oauth_discovery(20.0)
    with httpx.Client(timeout=httpx.Timeout(20.0), headers={"Accept": "application/json"}) as client:
        token_data = _xai_oauth_poll_device_token(
            client,
            token_endpoint=discovery["token_endpoint"],
            device_code=device_code,
            expires_in=expires_in,
            poll_interval=interval,
        )
    tokens = {
        "access_token": str(token_data.get("access_token", "") or "").strip(),
        "refresh_token": str(token_data.get("refresh_token", "") or "").strip(),
        "id_token": str(token_data.get("id_token", "") or "").strip(),
        "expires_in": token_data.get("expires_in"),
        "token_type": str(token_data.get("token_type") or "Bearer").strip() or "Bearer",
    }
    _save_xai_oauth_tokens(
        tokens,
        discovery=discovery,
        last_refresh=datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        auth_mode="oauth_device_code",
        set_active=False,
    )
    mark_provider_active_if_unset("xai-oauth")
    unsuppress_credential_source("xai-oauth", "device_code")
    sess["status"] = "approved"
    _write_session(sess)


def worker_minimax(sess: dict[str, Any]) -> None:
    import httpx
    from hermes_cli.auth import (
        MINIMAX_OAUTH_GLOBAL_INFERENCE,
        MINIMAX_OAUTH_SCOPE,
        _minimax_poll_token,
        _minimax_resolve_token_expiry_unix,
        _minimax_save_auth_state,
    )

    portal_base_url = sess["portal_base_url"]
    client_id = sess["client_id"]
    user_code = sess["user_code"]
    code_verifier = sess["code_verifier"]
    interval_ms = sess.get("interval_ms")
    expired_in_raw = sess["expired_in_raw"]
    with httpx.Client(
        timeout=httpx.Timeout(15.0),
        headers={"Accept": "application/json"},
        follow_redirects=True,
    ) as client:
        token_data = _minimax_poll_token(
            client=client,
            portal_base_url=portal_base_url,
            client_id=client_id,
            user_code=user_code,
            code_verifier=code_verifier,
            expired_in=expired_in_raw,
            interval_ms=interval_ms,
        )
    now = datetime.now(timezone.utc)
    expires_at_ts = _minimax_resolve_token_expiry_unix(int(token_data["expired_in"]), now=now)
    expires_in_s = max(0, int(expires_at_ts - now.timestamp()))
    auth_state = {
        "provider": "minimax-oauth",
        "region": sess.get("region", "global"),
        "portal_base_url": portal_base_url,
        "inference_base_url": MINIMAX_OAUTH_GLOBAL_INFERENCE,
        "client_id": client_id,
        "scope": MINIMAX_OAUTH_SCOPE,
        "token_type": token_data.get("token_type", "Bearer"),
        "access_token": token_data["access_token"],
        "refresh_token": token_data["refresh_token"],
        "resource_url": token_data.get("resource_url"),
        "obtained_at": now.isoformat(),
        "expires_at": datetime.fromtimestamp(expires_at_ts, tz=timezone.utc).isoformat(),
        "expires_in": expires_in_s,
    }
    _minimax_save_auth_state(auth_state)
    sess["status"] = "approved"
    _write_session(sess)


def worker_codex(sess: dict[str, Any]) -> None:
    import httpx
    from hermes_cli.auth import CODEX_OAUTH_CLIENT_ID, CODEX_OAUTH_TOKEN_URL, _save_codex_tokens

    issuer = "https://auth.openai.com"
    with httpx.Client(timeout=httpx.Timeout(15.0)) as client:
        resp = client.post(
            f"{issuer}/api/accounts/deviceauth/usercode",
            json={"client_id": CODEX_OAUTH_CLIENT_ID},
            headers={"Content-Type": "application/json"},
        )
    if resp.status_code != 200:
        raise RuntimeError(f"OpenAI device-code start failed (HTTP {resp.status_code})")
    device_data = resp.json()
    user_code = device_data.get("user_code", "")
    device_auth_id = device_data.get("device_auth_id", "")
    poll_interval = max(3, int(device_data.get("interval", "5")))
    if not user_code or not device_auth_id:
        raise RuntimeError("device-code response missing user_code or device_auth_id")
    sess["user_code"] = user_code
    sess["verification_url"] = f"{issuer}/codex/device"
    sess["device_auth_id"] = device_auth_id
    sess["interval"] = poll_interval
    sess["expires_in"] = 15 * 60
    sess["expires_at"] = time.time() + sess["expires_in"]
    _write_session(sess)

    deadline = time.monotonic() + sess["expires_in"]
    code_resp = None
    with httpx.Client(timeout=httpx.Timeout(15.0)) as client:
        while time.monotonic() < deadline:
            time.sleep(poll_interval)
            poll = client.post(
                f"{issuer}/api/accounts/deviceauth/token",
                json={"device_auth_id": device_auth_id, "user_code": user_code},
                headers={"Content-Type": "application/json"},
            )
            if poll.status_code == 200:
                code_resp = poll.json()
                break
            if poll.status_code in {403, 404}:
                continue
            raise RuntimeError(f"deviceauth/token poll returned {poll.status_code}")
    if code_resp is None:
        sess["status"] = "expired"
        sess["error_message"] = "Device code expired before approval"
        _write_session(sess)
        return
    authorization_code = code_resp.get("authorization_code", "")
    code_verifier = code_resp.get("code_verifier", "")
    if not authorization_code or not code_verifier:
        raise RuntimeError("device-auth response missing authorization_code/code_verifier")
    with httpx.Client(timeout=httpx.Timeout(15.0)) as client:
        token_resp = client.post(
            CODEX_OAUTH_TOKEN_URL,
            data={
                "grant_type": "authorization_code",
                "code": authorization_code,
                "redirect_uri": f"{issuer}/deviceauth/callback",
                "client_id": CODEX_OAUTH_CLIENT_ID,
                "code_verifier": code_verifier,
            },
            headers={"Content-Type": "application/x-www-form-urlencoded"},
        )
    if token_resp.status_code != 200:
        raise RuntimeError(f"token exchange returned {token_resp.status_code}")
    tokens = token_resp.json()
    access_token = tokens.get("access_token", "")
    refresh_token = tokens.get("refresh_token", "")
    if not access_token:
        raise RuntimeError("token exchange did not return access_token")
    _save_codex_tokens({"access_token": access_token, "refresh_token": refresh_token})
    sess["status"] = "approved"
    _write_session(sess)


def do_poll_worker(session_id: str) -> None:
    path = _session_path(session_id)
    if not path.is_file():
        return
    sess = json.loads(path.read_text())
    provider = sess.get("provider")
    try:
        if provider == "nous":
            worker_nous(sess)
        elif provider == "xai-oauth":
            worker_xai(sess)
        elif provider == "minimax-oauth":
            worker_minimax(sess)
        elif provider == "openai-codex":
            worker_codex(sess)
        else:
            _set_error(sess, f"no poller for {provider}")
    except Exception as exc:
        _set_error(sess, str(exc))


def main() -> None:
    raw = sys.stdin.read()
    try:
        req = json.loads(raw) if raw.strip() else {}
    except json.JSONDecodeError:
        _err("invalid JSON stdin")
    if not isinstance(req, dict):
        _err("stdin must be a JSON object")
    action = str(req.get("action") or "").strip()
    provider = str(req.get("provider") or "").strip().lower()
    session = str(req.get("session") or req.get("session_id") or "").strip()
    code = str(req.get("code") or "")
    if provider and not PROVIDER_RE.match(provider):
        _err("invalid provider id")
    try:
        if action == "status":
            if not provider:
                _err("provider is required")
            do_status(provider)
        elif action == "refresh":
            if not provider:
                _err("provider is required")
            do_refresh(provider)
        elif action == "login":
            if not provider:
                _err("provider is required")
            do_login(provider)
        elif action == "poll":
            if not session:
                _err("session is required")
            do_poll(session)
        elif action == "submit":
            if not session:
                _err("session is required")
            do_submit(session, code)
        elif action == "poll-worker":
            if not session:
                return
            do_poll_worker(session)
        else:
            _err(f"unknown action: {action}")
    except SystemExit:
        raise
    except Exception as exc:
        _err(str(exc) or "oauth helper failed", detail=traceback.format_exc()[-400:])


if __name__ == "__main__":
    main()
