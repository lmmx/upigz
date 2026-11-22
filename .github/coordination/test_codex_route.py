#!/usr/bin/env python3
"""Offline sanity checks for Codex issue routing; run: python3 .github/coordination/test_codex_route.py.

No network or GitHub token required. Adapted from lmmx/mibz2's: upigz's Codex inbox is issue #9,
and every inbox starts from offset 0.
"""
import importlib.util
from pathlib import Path

p = Path(__file__).with_name("coord.py")
spec = importlib.util.spec_from_file_location("coord", p)
assert spec and spec.loader
coord = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coord)

assert coord.ROLES["codex"]["inbox"] == 9
assert coord.ROLES["codex"]["offset"] == 0
assert coord.CUTOVER == min(r["offset"] for r in coord.ROLES.values()) == 0
assert coord.ROLES["codex"]["cursor"] == ("team-state/codex", "state/cursor.json")
assert coord.canonical("codex") == "codex"
assert coord.roles_named("codex") == ["codex"]
assert coord.completed("codex", {"processed_comment_ids": [100, 101]}) == {100, 101}
incoming = "FROM: coordinator\nTO: codex\nRE: a slice @ abcdef\nTYPE: CHECKPOINT\n\nACTOR: ChatGPT Work (acting)"
outgoing = "FROM: codex\nTO: coordinator\nRE: inbox 123\nTYPE: REVIEW"
assert coord.misrouted(9, incoming, 9000000000) is None
assert coord.misrouted(2, outgoing, 9000000001) is None
assert coord.misrouted(9, outgoing, 9000000002) is not None
assert coord.misrouted(2, incoming, 9000000003) is not None
assert coord.misrouted(9, incoming.replace("TO: codex", "TO: monodromy"), 9000000004) is not None
assert coord.misrouted(9, incoming.replace("TYPE: CHECKPOINT", "TYPE: QUEUED"), 9000000005) is not None
print("Codex routing static checks passed; real GitHub delivery remains untested.")
