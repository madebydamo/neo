"""Report Hermes turns to herdr-core as OSC 7501 on the controlling terminal.

Claude Code and OpenCode send these reports themselves; Hermes does not. Only active when
hc started the terminal (HC_TERMINAL_ID, passed through the admin `hermes` sudo shim), so
the gateway and dashboard never write anything. Subagent turns are ignored so a child
finishing does not end the parent's turn.
"""

import os
import sys
import threading

_lock = threading.Lock()
_children: set = set()


def _report(state: str, kind: str = "") -> None:
    fields = f"state={state}:app=hermes" + (f":kind={kind}" if kind else "")
    try:
        # The CLI runs the agent in-process on its controlling terminal (sudo's pty,
        # which sudo relays to the hc terminal).
        with _lock, open("/dev/tty", "w") as tty:
            tty.write(f"\033]7501;{fields}\007")
    except OSError:
        pass


def _child(kwargs: dict) -> bool:
    sid = kwargs.get("session_id")
    if kwargs.get("parent_session_id"):
        if sid:
            _children.add(sid)
        return True
    return bool(sid) and sid in _children


def _on_subagent_start(**kwargs) -> None:
    sid = kwargs.get("child_session_id")
    if sid:
        _children.add(sid)


def _on_working(**kwargs) -> None:
    if not _child(kwargs):
        _report("working")


def _on_input_request(**kwargs) -> None:
    # herdr kinds: permission, question, auth.
    kind = kwargs.get("kind")
    _report("blocked", "question" if kind == "clarify" else "permission")


def _on_input_resolved(**kwargs) -> None:
    _report("working")


def _on_session_end(**kwargs) -> None:
    # Only the canonical per-turn shape carries `completed`; CLI exit shapes are skipped.
    if "completed" not in kwargs or _child(kwargs):
        return
    if kwargs.get("interrupted"):
        _report("idle")
    elif kwargs.get("failed"):
        _report("error")
    else:
        _report("done")


def _interactive_chat() -> bool:
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    return sys.stdin.isatty() and (not args or args[0] == "chat")


def register(ctx) -> None:
    if not os.environ.get("HC_TERMINAL_ID"):
        return
    ctx.register_hook("subagent_start", _on_subagent_start)
    ctx.register_hook("pre_llm_call", _on_working)
    ctx.register_hook("pre_tool_call", _on_working)
    ctx.register_hook("on_human_input_request", _on_input_request)
    ctx.register_hook("on_human_input_resolved", _on_input_resolved)
    ctx.register_hook("on_session_end", _on_session_end)
    # The card appears when the chat opens, not only after the first prompt.
    if _interactive_chat():
        _report("idle")
