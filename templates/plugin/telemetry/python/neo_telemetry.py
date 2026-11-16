"""Neo app telemetry for Python apps.

Copied from neo `templates/plugin/telemetry/python/neo_telemetry.py`; keep the
copy unchanged so every app exports the same names (docs/APP-TELEMETRY.md).
Needs `prometheus_client`.

    import neo_telemetry as tel

    tel.setup_logging()            # JSON lines on stdout, errors counted
    tel.start_metrics()            # :$NEO_METRICS_PORT/metrics, only when set
    tel.build_info(__version__)

    with tel.llm_call("anthropic", model, feature="chat") as call:
        resp = client.messages.create(...)
        call.usage = resp.usage    # Anthropic, OpenAI/OpenRouter, Gemini or a dict

    tel.feature("export_pdf")
    tel.login("password", ok=True, user="alice")
    tel.seen_user("alice")         # active users 1h / 24h / 7d
    with tel.job("nightly_sync"):
        sync()
    tel.data_updated("simap")
"""

from __future__ import annotations

import json
import logging
import os
import sys
import threading
import time
from contextlib import contextmanager
from typing import Any

from prometheus_client import Counter, Gauge, Histogram, start_http_server

# prometheus_client appends _total to counters.
LLM_REQUESTS = Counter("neo_app_llm_requests", "AI calls by result.", ["provider", "model", "feature", "status"])
LLM_TOKENS = Counter("neo_app_llm_tokens", "AI tokens; kind=input is uncached input.", ["provider", "model", "feature", "kind"])
LLM_COST = Counter("neo_app_llm_cost_usd", "AI cost in USD as reported by the provider.", ["provider", "model", "feature"])
LLM_DURATION = Histogram(
    "neo_app_llm_request_duration_seconds",
    "Time for one AI call.",
    ["provider", "model", "feature"],
    buckets=(0.5, 1, 2, 5, 10, 20, 30, 60, 120, 300, 600),
)
LOGINS = Counter("neo_app_logins", "Sign-ins by result.", ["method", "result"])
ACTIVE_USERS = Gauge("neo_app_active_users", "Different users seen in the window.", ["window"])
FEATURES = Counter("neo_app_feature_used", "Uses of a feature.", ["feature"])
ERRORS = Counter("neo_app_errors", "Errors by component and kind.", ["component", "kind"])
JOB_RUNS = Counter("neo_app_job_runs", "Background job runs.", ["job", "status"])
JOB_LAST_SUCCESS = Gauge("neo_app_job_last_success_timestamp_seconds", "Last successful run.", ["job"])
JOB_DURATION = Histogram(
    "neo_app_job_duration_seconds",
    "Background job run time.",
    ["job"],
    buckets=(1, 5, 15, 60, 300, 900, 3600, 4 * 3600),
)
DATA_UPDATED = Gauge("neo_app_data_updated_timestamp_seconds", "Last update of a data source.", ["source"])
BUILD_INFO = Gauge("neo_app_build_info", "App version.", ["version"])

log = logging.getLogger("neo_telemetry")

# ── Logging ──────────────────────────────────────────────────────────────────


class JsonFormatter(logging.Formatter):
    """One JSON object per line: ts, level, logger, msg, extra fields, exc."""

    _skip = set(vars(logging.makeLogRecord({}))) | {"message", "asctime"}

    def format(self, record: logging.LogRecord) -> str:
        out: dict[str, Any] = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime(record.created)) + f".{int(record.msecs):03d}Z",
            "level": record.levelname.lower(),
            "logger": record.name,
            "msg": record.getMessage(),
        }
        for k, v in vars(record).items():
            if k not in self._skip and not k.startswith("_"):
                out[k] = v if isinstance(v, (str, int, float, bool, type(None))) else repr(v)
        if record.exc_info:
            out["exc"] = self.formatException(record.exc_info)
            out.setdefault("error", type(record.exc_info[1]).__name__)
        return json.dumps(out, ensure_ascii=False)


class _ErrorCounter(logging.Handler):
    def emit(self, record: logging.LogRecord) -> None:
        if record.levelno >= logging.ERROR:
            kind = type(record.exc_info[1]).__name__ if record.exc_info and record.exc_info[1] else "log"
            ERRORS.labels(record.name.split(".")[0], kind).inc()


def setup_logging(level: str | None = None) -> None:
    """JSON logs on stdout for the root logger and uvicorn/gunicorn/werkzeug."""
    level = (level or os.environ.get("LOG_LEVEL") or "INFO").upper()
    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(JsonFormatter())
    root = logging.getLogger()
    root.handlers[:] = [handler, _ErrorCounter()]
    root.setLevel(level)
    for name in ("uvicorn", "uvicorn.error", "uvicorn.access", "gunicorn.error", "gunicorn.access", "werkzeug"):
        lg = logging.getLogger(name)
        lg.handlers[:] = []
        lg.propagate = True
    # Access logs duplicate SWAG's; keep them out unless debugging.
    if level != "DEBUG":
        logging.getLogger("uvicorn.access").setLevel(logging.WARNING)
        logging.getLogger("werkzeug").setLevel(logging.WARNING)

    def _excepthook(tp, val, tb):
        logging.getLogger("unhandled").critical("unhandled exception", exc_info=(tp, val, tb))

    sys.excepthook = _excepthook
    threading.excepthook = lambda a: _excepthook(a.exc_type, a.exc_value, a.exc_traceback)


# ── Metrics endpoint ─────────────────────────────────────────────────────────


def start_metrics(port: int | None = None) -> bool:
    """Serve /metrics on $NEO_METRICS_PORT (Neo sets it only while monitoring collects)."""
    port = port or int(os.environ.get("NEO_METRICS_PORT") or 0)
    if not port:
        return False
    start_http_server(port, addr="0.0.0.0")
    log.info("metrics on :%d", port)
    return True


def build_info(version: str) -> None:
    BUILD_INFO.labels(version or "unknown").set(1)


# ── AI calls ─────────────────────────────────────────────────────────────────


def _get(obj: Any, *path: str) -> Any:
    for key in path:
        if obj is None:
            return None
        obj = obj.get(key) if isinstance(obj, dict) else getattr(obj, key, None)
    return obj


def _num(v: Any) -> float:
    try:
        return float(v or 0)
    except (TypeError, ValueError):
        return 0.0


def normalize_usage(usage: Any) -> dict[str, float]:
    """Token counts as {input (uncached), cache_read, cache_write, output}.

    Anthropic / Claude Code: input_tokens is already uncached.
    OpenAI / OpenRouter: prompt_tokens includes cached_tokens.
    Gemini: promptTokenCount includes cachedContentTokenCount.
    """
    if usage is None:
        return {}
    if hasattr(usage, "model_dump"):
        usage = usage.model_dump()
    if _get(usage, "input_tokens") is not None or _get(usage, "cache_read_input_tokens") is not None:
        return {
            "input": _num(_get(usage, "input_tokens")),
            "cache_read": _num(_get(usage, "cache_read_input_tokens")),
            "cache_write": _num(_get(usage, "cache_creation_input_tokens")),
            "output": _num(_get(usage, "output_tokens")),
        }
    if _get(usage, "prompt_tokens") is not None:
        cached = _num(_get(usage, "prompt_tokens_details", "cached_tokens"))
        write = _num(_get(usage, "prompt_tokens_details", "cache_write_tokens"))
        return {
            "input": max(_num(_get(usage, "prompt_tokens")) - cached - write, 0),
            "cache_read": cached,
            "cache_write": write,
            "output": _num(_get(usage, "completion_tokens")),
        }
    if _get(usage, "promptTokenCount") is not None or _get(usage, "prompt_token_count") is not None:
        prompt = _num(_get(usage, "promptTokenCount") or _get(usage, "prompt_token_count"))
        cached = _num(_get(usage, "cachedContentTokenCount") or _get(usage, "cached_content_token_count"))
        return {
            "input": max(prompt - cached, 0),
            "cache_read": cached,
            "cache_write": 0,
            "output": _num(_get(usage, "candidatesTokenCount") or _get(usage, "candidates_token_count")),
        }
    return {}


def classify_error(exc: BaseException | None = None, status_code: int | None = None) -> str:
    """ok|error|timeout|rate_limited|auth from an exception or HTTP status."""
    code = status_code or _get(exc, "status_code") or _get(exc, "status") or _get(exc, "response", "status_code")
    try:
        code = int(code) if code is not None else None
    except (TypeError, ValueError):
        code = None
    name = type(exc).__name__.lower() if exc else ""
    if code in (401, 403) or "authentication" in name or "permission" in name:
        return "auth"
    if code == 402:
        return "auth"  # out of credit: same action, check the account
    if code == 429 or "ratelimit" in name:
        return "rate_limited"
    if code in (408, 504) or "timeout" in name:
        return "timeout"
    return "error"


def record_llm(
    provider: str,
    model: str,
    feature: str,
    usage: Any = None,
    *,
    status: str = "ok",
    duration: float | None = None,
    cost_usd: float | None = None,
) -> None:
    """Count one finished AI call (use llm_call() when you can)."""
    model = model or "unknown"
    LLM_REQUESTS.labels(provider, model, feature, status).inc()
    for kind, n in normalize_usage(usage).items():
        if n:
            LLM_TOKENS.labels(provider, model, feature, kind).inc(n)
    if cost_usd is None:
        cost_usd = _get(usage, "cost")  # OpenRouter usage accounting
    if cost_usd:
        LLM_COST.labels(provider, model, feature).inc(_num(cost_usd))
    if duration is not None:
        LLM_DURATION.labels(provider, model, feature).observe(duration)


class _Call:
    def __init__(self, provider: str, model: str, feature: str):
        self.provider, self.model, self.feature = provider, model, feature
        self.usage: Any = None
        self.cost_usd: float | None = None
        self.status = "ok"


@contextmanager
def llm_call(provider: str, model: str, feature: str):
    """Time and count an AI call; set call.usage (and call.cost_usd, call.model)."""
    call = _Call(provider, model, feature)
    start = time.monotonic()
    try:
        yield call
    except BaseException as exc:
        call.status = classify_error(exc)
        log.error(
            "AI call failed",
            exc_info=True,
            extra={"event": "llm_error", "provider": provider, "model": call.model, "feature": feature, "status": call.status},
        )
        raise
    finally:
        record_llm(
            call.provider,
            call.model,
            call.feature,
            call.usage,
            status=call.status,
            duration=time.monotonic() - start,
            cost_usd=call.cost_usd,
        )


# ── Use, sign-ins, jobs, data ────────────────────────────────────────────────


def feature(name: str, n: int = 1) -> None:
    FEATURES.labels(name).inc(n)


_seen: dict[str, float] = {}
_seen_lock = threading.Lock()
_WINDOWS = {"1h": 3600, "24h": 86400, "7d": 7 * 86400}


def _active(seconds: int) -> int:
    cutoff = time.time() - seconds
    with _seen_lock:
        for user in [u for u, t in _seen.items() if t < time.time() - _WINDOWS["7d"]]:
            del _seen[user]
        return sum(1 for t in _seen.values() if t >= cutoff)


for _w, _s in _WINDOWS.items():
    ACTIVE_USERS.labels(_w).set_function(lambda s=_s: _active(s))


def seen_user(user: str | None) -> None:
    """Note a request by a signed-in user (counts restart from zero on restart)."""
    if user:
        with _seen_lock:
            _seen[user] = time.time()


def login(method: str, ok: bool, user: str | None = None, reason: str | None = None) -> None:
    """A sign-in attempt; logged with the user (never the password)."""
    LOGINS.labels(method, "success" if ok else "failure").inc()
    log.log(
        logging.INFO if ok else logging.WARNING,
        "sign-in %s" % ("ok" if ok else "failed"),
        extra={"event": "login", "method": method, "result": "success" if ok else "failure", "user": user, "reason": reason},
    )
    if ok:
        seen_user(user)


@contextmanager
def job(name: str):
    """A background job run: counted, timed, last success, failures logged."""
    start = time.monotonic()
    try:
        yield
    except BaseException:
        JOB_RUNS.labels(name, "failure").inc()
        log.error("job %s failed", name, exc_info=True, extra={"event": "job_failed", "job": name})
        raise
    else:
        JOB_RUNS.labels(name, "success").inc()
        JOB_LAST_SUCCESS.labels(name).set(time.time())
        log.info("job %s done", name, extra={"event": "job_done", "job": name, "seconds": round(time.monotonic() - start, 3)})
    finally:
        JOB_DURATION.labels(name).observe(time.monotonic() - start)


def data_updated(source: str, ts: float | None = None) -> None:
    DATA_UPDATED.labels(source).set(ts if ts is not None else time.time())
