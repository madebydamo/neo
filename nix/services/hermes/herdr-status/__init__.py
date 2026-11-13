"""Report Hermes turns to herdr-core as OSC 7501 on the controlling terminal.

Claude Code and OpenCode send these reports themselves; Hermes does not. Only active when
hc started the terminal (HC_TERMINAL_ID, passed through the admin `hermes` sudo shim), so
the gateway and dashboard never write anything.

The interactive CLI is a client of the gateway: turns, tools and their plugin hooks run in
the `hermes gateway` service, which has neither the hc ids nor this terminal. So the plugin
reports from the CLI process instead, by wrapping the client's turn (`GatewayAgent`): working
while a turn runs, blocked while it waits on an approval or question, then done/error/idle.
/clear, /new, /reset and /resume report idle again, so the card starts over.
Subagents run in the gateway and never reach the client, so only the user's turns count.
"""

import functools
import logging
import os
import sys
import threading

logger = logging.getLogger(__name__)
_lock = threading.Lock()


def _report(state: str, kind: str = "") -> None:
    fields = f"state={state}:app=hermes" + (f":kind={kind}" if kind else "")
    try:
        # sudo's pty, which sudo relays to the hc terminal.
        with _lock, open("/dev/tty", "w") as tty:
            tty.write(f"\033]7501;{fields}\007")
    except OSError:
        pass


def _blocking(fn, kind):
    """Report blocked (herdr kinds: permission, question, auth) while ``fn`` waits on the user."""

    @functools.wraps(fn)
    def wrapper(*args, **kwargs):
        _report("blocked", kind(*args) if callable(kind) else kind)
        try:
            return fn(*args, **kwargs)
        finally:
            _report("working")

    return wrapper


def _turn(fn):
    @functools.wraps(fn)
    def wrapper(*args, **kwargs):
        _report("working")
        try:
            result = fn(*args, **kwargs)
        except BaseException:
            _report("error")
            raise
        result_ = result if isinstance(result, dict) else {}
        if result_.get("interrupted"):
            _report("idle")
        elif result_.get("failed"):
            _report("error")
        else:
            _report("done")
        return result

    return wrapper


def _request_kind(agent, frame, *_):
    method = (frame or {}).get("method", "") if isinstance(frame, dict) else ""
    return "question" if method == "clarify" else "auth" if method.startswith(("sudo", "secret", "vault")) else "permission"


def _patch_client() -> bool:
    try:
        from hermes_cli import gateway_agent as ga
    except ImportError:
        return False
    cls = getattr(ga, "GatewayAgent", None)
    if cls is None or getattr(cls, "_herdr_status", False):
        return cls is not None
    cls.run_conversation = _turn(cls.run_conversation)
    if hasattr(cls, "_answer_request"):
        cls._answer_request = _blocking(cls._answer_request, _request_kind)
    handlers = getattr(ga, "_HANDLERS", {})
    for event, kind in (("approval.request", "permission"), ("clarify.request", "question")):
        if event in handlers:
            handlers[event] = _blocking(handlers[event], kind)
    cls._herdr_status = True
    _patch_session_switch()
    return True


def _fresh(fn):
    """A new or resumed conversation starts the card over (Claude Code does the same on /clear)."""

    @functools.wraps(fn)
    def wrapper(*args, **kwargs):
        try:
            return fn(*args, **kwargs)
        finally:
            _report("idle")

    return wrapper


def _patch_session_switch() -> None:
    # /clear, /new and /reset all rotate the session through new_session. With the gateway
    # owning the session, the client never fires on_session_reset, so wrap the method itself.
    for module, cls_name, method in (
        ("hermes_cli.cli_session_mixin", "CLISessionMixin", "new_session"),
        ("hermes_cli.cli_commands_mixin", "CLICommandsMixin", "_handle_resume_command"),
    ):
        try:
            cls = getattr(__import__(module, fromlist=[cls_name]), cls_name)
            setattr(cls, method, _fresh(getattr(cls, method)))
        except (ImportError, AttributeError):
            logger.warning("herdr-status: no %s.%s; that reset is not reported", cls_name, method)


def _interactive_chat() -> bool:
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    return sys.stdin.isatty() and (not args or args[0] == "chat")


def register(ctx) -> None:
    if not os.environ.get("HC_TERMINAL_ID") or not _interactive_chat():
        return
    try:
        patched = _patch_client()
    except Exception:  # never break the CLI over a status card
        logger.warning("herdr-status: could not hook the CLI's turns", exc_info=True)
        patched = False
    if not patched:
        logger.warning("herdr-status: this Hermes has no GatewayAgent; only the idle card is reported")
    # The card appears when the chat opens, not only after the first prompt.
    _report("idle")
