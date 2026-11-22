"""Offline tests for coord.py: run with `python3 -m unittest discover -s .github/coordination`.

upigz's channel is #2 and its inboxes #3-#9, every offset 0 (they start empty). Adapted from
lmmx/mibz2's tests, less the ones that replay mibz2's channel history (its fixtures.json):
upigz has no history to replay yet.
"""

import io
import unittest
from contextlib import redirect_stdout
from unittest import mock

import coord

CH = 2
SETUP = {
    "optimizer": (3, 0),
    "guardian": (4, 0),
    "researcher": (5, 0),
    "observer": (6, 0),
    "pathfinder": (7, 0),
    "monodromy": (8, 0),
    "codex": (9, 0),
}
UNSET = {role: (None, None) for role in SETUP}


class Configuration(unittest.TestCase):
    def test_the_channel_and_the_inboxes(self):
        self.assertEqual(coord.CHANNEL, CH)
        self.assertEqual({r: (v["inbox"], v["offset"]) for r, v in coord.ROLES.items()}, SETUP)
        self.assertEqual(coord.INBOXES, {i: r for r, (i, _) in SETUP.items()})
        self.assertEqual(coord.CUTOVER, 0)
        self.assertEqual(coord.RESOLVED, {})
        self.assertEqual(coord.ROLES["observer"]["cursor"],
                         ("team-state/observer", "state/cursor.json"))


def msg(**h):
    return "\n".join(f"{k}: {v}" for k, v in h.items()) + "\n\nbody"


def comment(issue, cid, body, bot=False):
    return {"id": cid, "body": body, "created_at": "2026-11-22T09:00:00Z",
            "issue_url": f"https://api.github.com/repos/{coord.REPO}/issues/{issue}",
            "user": {"login": "github-actions[bot]" if bot else "lmmx",
                     "type": "Bot" if bot else "User"}}


class NotSetUp(unittest.TestCase):
    def setUp(self):
        coord.configure(None, UNSET)

    def tearDown(self):
        coord.configure(CH, SETUP)

    def test_route_verify_and_audit_refuse_until_set_up(self):
        for argv in (["route", "--to", "guardian", "--type", "CHECKPOINT"],
                     ["verify", "--comment-id", "1", "--to", "guardian", "--type", "CHECKPOINT"],
                     ["audit"]):
            with redirect_stdout(io.StringIO()), mock.patch("sys.stderr", io.StringIO()):
                self.assertEqual(coord.main(argv), 2, argv)

    def test_nothing_is_misrouted_while_nothing_exists(self):
        body = msg(FROM="coordinator", TO="guardian", TYPE="CHECKPOINT")
        self.assertEqual(coord.INBOXES, {})
        self.assertEqual(coord.CUTOVER, 0)
        self.assertIsNone(coord.misrouted(CH, body, 5))


class Channel(unittest.TestCase):
    def setUp(self):
        self.new = coord.CUTOVER + 10_000_000

    def test_publications_pass(self):
        for kind in ("ACK", "REVIEW", "DECISION", "QUEUED"):
            body = msg(FROM="coordinator", TO="optimizer", TYPE=kind)
            self.assertIsNone(coord.misrouted(CH, body, self.new), kind)

    def test_a_coordinator_checkpoint_with_no_role_is_a_record(self):
        for to in (None, "record", "coordinator"):
            h = {"FROM": "coordinator", "TYPE": "CHECKPOINT"}
            if to:
                h["TO"] = to
            self.assertIsNone(coord.misrouted(CH, msg(**h), self.new))

    def test_assignments_to_any_role_are_flagged(self):
        for to, inbox in (("optimizer", "#3"), ("Optimiser", "#3"), ("guardian", "#4"),
                          ("researcher", "#5"), ("observer", "#6"), ("pathfinder", "#7"),
                          ("monodromy", "#8"), ("codex", "#9")):
            for kind in ("CHECKPOINT", "PROPOSAL", "CORRECTION"):
                reason = coord.misrouted(CH, msg(FROM="coordinator", TO=to, TYPE=kind),
                                         self.new)
                self.assertIsNotNone(reason, (to, kind))
                self.assertIn(inbox, reason)
                self.assertIn("QUEUED", reason)

    def test_a_broadcast_names_every_inbox(self):
        reason = coord.misrouted(
            CH, msg(FROM="coordinator", TO="guardian, researcher, optimizer",
                    TYPE="CHECKPOINT"), self.new)
        for inbox in ("#3", "#4", "#5"):
            self.assertIn(inbox, reason)

    def test_aliases_name_the_same_role(self):
        self.assertEqual(coord.canonical("Optimiser"), "optimizer")
        self.assertEqual(coord.canonical("observer"), "observer")
        self.assertEqual(coord.roles_named("optimiser and guardian"), ["guardian", "optimizer"])

    def test_other_senders_on_the_channel_pass(self):
        body = msg(FROM="optimizer", TO="coordinator, guardian", TYPE="REVIEW")
        self.assertIsNone(coord.misrouted(CH, body, self.new))

    def test_the_warning_itself_never_trips(self):
        text = coord.warning(CH, 1, "a CHECKPOINT ...")
        self.assertIsNone(coord.misrouted(CH, text, self.new))
        self.assertIn(coord.MARKER.format(id=1), text)


class Inbox(unittest.TestCase):
    def test_a_checkpoint_to_its_role_passes(self):
        for role, (issue, off) in SETUP.items():
            self.assertIsNone(coord.misrouted(
                issue, msg(FROM="coordinator", TO=role, TYPE="CHECKPOINT"), off + 1), role)

    def test_an_inbox_takes_only_assignments_after_its_offset(self):
        issue, off = SETUP["observer"]
        ack = msg(FROM="coordinator", TO="observer", TYPE="ACK")
        self.assertIsNone(coord.misrouted(issue, ack, off))  # the bootstrap ACK at the offset
        for kind in ("ACK", "REVIEW", "DECISION", "LOG"):
            body = msg(FROM="coordinator", TO="observer", TYPE=kind)
            self.assertIsNotNone(coord.misrouted(issue, body, off + 1), kind)
        for kind in ("CHECKPOINT", "CORRECTION"):
            body = msg(FROM="coordinator", TO="observer", TYPE=kind)
            self.assertIsNone(coord.misrouted(issue, body, off + 1), kind)

    def test_the_wrong_role_is_flagged(self):
        self.assertIn("guardian", coord.misrouted(
            3, msg(FROM="coordinator", TO="guardian", TYPE="CHECKPOINT")))

    def test_several_roles_in_one_inbox_are_flagged(self):
        self.assertIsNotNone(coord.misrouted(
            4, msg(FROM="coordinator", TO="guardian, researcher", TYPE="CHECKPOINT")))

    def test_only_the_coordinator_writes_in_an_inbox(self):
        self.assertIn("Only the coordinator", coord.misrouted(
            3, msg(FROM="guardian", TYPE="REVIEW")))
        self.assertIsNotNone(coord.misrouted(3, "no header at all"))

    def test_queued_belongs_on_the_channel(self):
        self.assertIn(f"#{CH}", coord.misrouted(
            5, msg(FROM="coordinator", TO="researcher", TYPE="QUEUED")))

    def test_other_prs_are_not_checked(self):
        self.assertIsNone(coord.misrouted(183, msg(FROM="coordinator", TO="optimizer",
                                                   TYPE="CHECKPOINT")))

    def test_each_role_reads_its_cursor(self):
        self.assertEqual(coord.completed("optimizer", {"inbox_briefs": [
            {"comment_id": 7, "completed": True}, {"comment_id": 8}]}), {7})
        self.assertEqual(coord.completed("guardian", {"consumed_incoming_comment_ids": [9]}), {9})
        for role in ("researcher", "observer", "pathfinder", "monodromy", "codex"):
            self.assertEqual(coord.completed(role, {"processed_comment_ids": [3]}), {3})


class Audit(unittest.TestCase):
    def test_only_unresolved_misroutes_fail_the_audit(self):
        good = comment(CH, coord.CUTOVER + 5, msg(FROM="coordinator", TYPE="ACK"))
        bad = comment(CH, coord.CUTOVER + 6, msg(FROM="coordinator", TO="guardian",
                                                 TYPE="CHECKPOINT"))
        history = {n: [] for n in (CH, *coord.INBOXES)}
        history[CH] = [good]
        with mock.patch.object(coord, "comments", lambda n: history[n]), \
                redirect_stdout(io.StringIO()):
            self.assertEqual(coord.main(["audit"]), 0)
        history[CH] = [good, bad]
        with mock.patch.object(coord, "comments", lambda n: history[n]), \
                redirect_stdout(io.StringIO()):
            self.assertEqual(coord.main(["audit"]), 1)
        with mock.patch.object(coord, "comments", lambda n: history[n]), \
                mock.patch.dict(coord.RESOLVED, {bad["id"]: "re-dispatched (a test)"}), \
                redirect_stdout(io.StringIO()):
            self.assertEqual(coord.main(["audit"]), 0)


class Queue(unittest.TestCase):
    """state() and route() against a fake inbox, channel and cursor."""

    inbox = SETUP["optimizer"][0]
    off = 7_000_000_000  # comment ids as GitHub makes them (coord.py reads 6+ digit citations)

    def run_route(self, inbox, channel, done):
        cur = {"inbox_briefs": [{"comment_id": i, "completed": True} for i in done]}
        with mock.patch.object(coord, "comments",
                               lambda n: {self.inbox: inbox, CH: channel}.get(n, [])), \
                mock.patch.object(coord, "cursor", lambda role: cur):
            return coord.route("optimizer", "CHECKPOINT")

    def item(self, cid, **h):
        return comment(self.inbox, cid, msg(FROM="coordinator", TO="optimizer",
                                            TYPE="CHECKPOINT", **h))

    def queued(self, cid, **h):
        return comment(CH, cid, msg(FROM="coordinator", TO="optimizer", TYPE="QUEUED", **h))

    def test_free_when_every_item_is_complete_and_reviewed(self):
        review = comment(CH, self.off + 2, msg(FROM="optimizer", TO="coordinator",
                                               TYPE="REVIEW") + f" inbox {self.off + 1}")
        r = self.run_route([self.item(self.off + 1)], [review], [self.off + 1])
        self.assertEqual((r["issue"], r["type"]), (self.inbox, "CHECKPOINT"))

    def test_busy_while_an_item_is_incomplete(self):
        r = self.run_route([self.item(self.off + 1)], [], [])
        self.assertEqual((r["issue"], r["type"]), (CH, "QUEUED"))

    def test_busy_while_a_complete_item_awaits_its_review(self):
        r = self.run_route([self.item(self.off + 1)], [], [self.off + 1])
        self.assertEqual(r["type"], "QUEUED")

    def test_an_older_queued_item_goes_first(self):
        r = self.run_route([], [self.queued(self.off + 5)], [])
        self.assertEqual(r["issue"], self.inbox)
        self.assertEqual(r["queued"], [self.off + 5])

    def test_a_queued_item_cited_by_the_inbox_is_dispatched(self):
        item = self.item(self.off + 6, RE=f"dispatching QUEUED {self.off + 5}")
        review = comment(CH, self.off + 7, msg(FROM="optimizer", TYPE="REVIEW")
                         + f" {self.off + 6}")
        r = self.run_route([item], [self.queued(self.off + 5), review], [self.off + 6])
        self.assertNotIn("queued", r)

    def test_a_queued_item_mentioned_only_in_a_body_stays_queued(self):
        item = comment(self.inbox, self.off + 6,
                       msg(FROM="coordinator", TO="optimizer", RE="another item",
                           TYPE="CHECKPOINT") + f" Next in your queue: QUEUED {self.off + 5}")
        review = comment(CH, self.off + 7, msg(FROM="optimizer", TYPE="REVIEW")
                         + f" {self.off + 6}")
        r = self.run_route([item], [self.queued(self.off + 5), review], [self.off + 6])
        self.assertEqual(r["queued"], [self.off + 5])

    def test_a_priority_1_item_dispatches_before_an_older_priority_3_one(self):
        older, urgent = self.queued(self.off + 5, PRIORITY=3), self.queued(self.off + 6, PRIORITY=1)
        r = self.run_route([], [older, urgent], [])
        self.assertEqual(r["queued"], [self.off + 6, self.off + 5])
        self.assertIn(f"QUEUED {self.off + 6} is next by priority then age", r["why"])

    def test_queued_items_of_equal_priority_dispatch_by_age(self):
        newer, older = self.queued(self.off + 6, PRIORITY=2), self.queued(self.off + 5)
        r = self.run_route([], [newer, older], [])
        self.assertEqual(r["queued"], [self.off + 5, self.off + 6])

    def test_publications_always_go_to_the_channel(self):
        self.assertEqual(coord.route("optimizer", "ACK")["issue"], CH)


class Verify(unittest.TestCase):
    cid = 7_000_000_000

    def check(self, issue, body, role="optimizer", kind="CHECKPOINT", state=None):
        c = comment(issue, self.cid, body)
        st = state or {"incomplete": [self.cid], "unreviewed": [self.cid], "queued": []}
        with mock.patch.object(coord, "get", lambda url: c), \
                mock.patch.object(coord, "state", lambda r: st):
            return coord.verify(self.cid, role, kind)

    def test_a_delivered_checkpoint_passes(self):
        r = self.check(3, msg(FROM="coordinator", TO="optimizer", TYPE="CHECKPOINT"))
        self.assertTrue(r["ok"], r)

    def test_a_checkpoint_on_the_channel_fails(self):
        r = self.check(CH, msg(FROM="coordinator", TO="optimizer", TYPE="CHECKPOINT"))
        self.assertFalse(r["ok"])
        self.assertTrue(any("#3" in p for p in r["problems"]))

    def test_a_second_assignment_in_flight_fails(self):
        st = {"incomplete": [self.cid - 1, self.cid], "unreviewed": [], "queued": []}
        r = self.check(3, msg(FROM="coordinator", TO="optimizer", TYPE="CHECKPOINT"),
                       state=st)
        self.assertFalse(r["ok"])

    def test_a_resume_request_for_the_item_in_flight_passes(self):
        st = {"incomplete": [self.cid - 1, self.cid], "unreviewed": [self.cid - 1], "queued": []}
        r = self.check(3, msg(FROM="coordinator", TO="optimizer",
                                RE=f"resume {self.cid - 1}", TYPE="CHECKPOINT"), state=st)
        self.assertTrue(r["ok"], r)

    def test_a_resume_request_does_not_cover_two_items(self):
        st = {"incomplete": [self.cid - 2, self.cid - 1, self.cid], "unreviewed": [],
              "queued": []}
        r = self.check(3, msg(FROM="coordinator", TO="optimizer",
                                RE=f"resume {self.cid - 1}", TYPE="CHECKPOINT"), state=st)
        self.assertFalse(r["ok"])

    def test_queued_must_be_on_the_channel(self):
        ok = self.check(CH, msg(FROM="coordinator", TO="optimizer", TYPE="QUEUED"),
                        kind="QUEUED")
        self.assertTrue(ok["ok"], ok)

    def test_wrong_type_or_role_fails(self):
        self.assertFalse(self.check(3, msg(FROM="coordinator", TO="optimizer",
                                             TYPE="ACK"))["ok"])
        self.assertFalse(self.check(4, msg(FROM="coordinator", TO="guardian",
                                             TYPE="CHECKPOINT"))["ok"])


class HeadSha(unittest.TestCase):
    head = "c32ba5db9df80a08147c35eeb1fb91259faab965"

    def run_guard(self, given, merged=False, is_merged=False):
        pull = {"head": {"sha": self.head}, "merged": is_merged}
        with mock.patch.object(coord, "get", lambda url: pull):
            return coord.head_sha(7, given, merged)["ok"]

    def test_only_the_exact_head_passes(self):
        self.assertTrue(self.run_guard(self.head))
        self.assertFalse(self.run_guard("c32ba5db"))  # a prefix
        self.assertFalse(self.run_guard("c32ba5db3b0f9eec2b3d0dc3cbe8e4cb24a71c51"))  # a guess
        self.assertFalse(self.run_guard(self.head.upper()))

    def test_after_the_merge_the_pr_is_merged_at_that_head(self):
        self.assertTrue(self.run_guard(self.head, merged=True, is_merged=True))
        self.assertFalse(self.run_guard(self.head, merged=True, is_merged=False))
        self.assertFalse(self.run_guard("c32ba5db3b0f9eec2b3d0dc3cbe8e4cb24a71c51",
                                        merged=True, is_merged=True))
        self.assertTrue(self.run_guard(self.head, merged=False, is_merged=False))

    def test_head_sha_works_before_the_channel_exists(self):
        coord.configure(None, UNSET)
        try:
            self.assertTrue(self.run_guard(self.head))
        finally:
            coord.configure(CH, SETUP)


class Cli(unittest.TestCase):
    def test_warn_prints_only_for_a_misroute(self):
        for body, want in ((msg(FROM="coordinator", TO="optimizer", TYPE="CHECKPOINT"), True),
                           (msg(FROM="coordinator", TO="optimizer", TYPE="ACK"), False)):
            out = io.StringIO()
            with mock.patch("sys.stdin", io.StringIO(body)), redirect_stdout(out):
                code = coord.main(["warn", "--issue", str(CH),
                                   "--comment-id", str(coord.CUTOVER + 1)])
            self.assertEqual(code, 0)
            self.assertEqual(bool(out.getvalue().strip()), want)


if __name__ == "__main__":
    unittest.main()
