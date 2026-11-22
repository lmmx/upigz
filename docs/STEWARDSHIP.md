# Stewardship of upigz's production code and releases

This document records who looks after upigz's production code and releases, what that role
may do without asking, and how a release is carried out and checked. It holds across research
campaigns: a campaign may fail, succeed and finish, or move on to other research, and this
role continues either way. Each campaign keeps its own rules in its own entry document.

## The decision

In mibz2, on 2026-11-04, the maintainer made the Coordinator mibz2's permanent production and
release steward, across research campaigns (mibz2's `docs/STEWARDSHIP.md`, "The decision").
upigz was seeded from mibz2 with the same arrangement: the steward is the Coordinator, the
maintainer's Claude Code session, and the agent team's standing rules are
`docs/team/RULES.md` (row A6 is this role).

## The standing objective

The objective is a measurably faster, correct production decoder, properly released. The
steward:
- keeps a failed or unqualified experiment out of production;
- doesn't let a qualified, merged improvement sit unreleased indefinitely;
- doesn't hold other legitimate maintenance back because of a campaign.

## What the steward may do without asking

The steward may open, review and merge pull requests into `master`, and may release, under
these gates.

**Changes that touch measured code** (`src/**`, `Cargo.toml`, `Cargo.lock`, the bench's inputs)
merge under PERFORMANCE-POLICY.md (2026-11-10): the tests and parity checks pass, CI is green on
the head that will merge, and the CI paired A/B bench (`.github/workflows/bench.yml`) is judged
by that policy.

**Maintenance route** (2026-11-10). A change that is neutral on the bench and improves the code
(simpler, safer, a better API) merges on green CI and one review.

**Target-gated changes** follow PERFORMANCE-POLICY.md §2: the gain on the target machine (C5), a
deterministic CI test that the predicate is false and master's plan is built, and a neutral CI
bench. A change is judged prospectively under this policy. (mibz2's earlier target-gated route is
retired there and was never adopted here.)

**A PR changing the default kernel** states its mechanism, its predicted end-to-end effect, and
what falsifies it. CI's end-to-end bench decides; until upigz has one, a registered workstation recipe on the i9
does (RULES C5). Instruction counts are optional diagnostics. An
instruction-count result gets a wall-clock CI bench within one working cycle
(PERFORMANCE-POLICY §4), or it is dropped.
- **Timing on the shared development machine** is never the evidence. A workstation run is
  evidence only under RULES.md C5.

mibz2's lmmx/mibz2#155 is why: a change that saved instructions in one stage and added more in
another measured slower end to end.

**Docs-only and measurement-only changes** merge when the CI their diff can reach is green and
nothing measured changed that wasn't meant to.

**Queue-only PRs, content CI has already passed, and jobs the diff can't reach** merge
without waiting for that CI (Louis, 17 and 18 November 2026; CLAUDE.md, "When not to wait for
CI"). A job isn't waited for when its inputs on the head are identical to those on the merge
base, or on an earlier commit of the PR where it passed; every other job must be green on the
head. The head-sha checks before and
after still run, and the merge's LOG names what made the wait unnecessary.

**A red check** is handled by `docs/team/CI-INCIDENTS.md`. It classifies the failure from its
log, re-runs only the failed job, paced to the outage, and records which checks did not run. It
does not relax the green-head rule above.

**For every merge:**
- merge only the head that CI measured;
- set `expectedHeadSha` to that head;
- a later commit that changes measured code needs a fresh bench.

## Release checkpoints

When accepted production changes have landed on `master`, the steward owns the release:
- Batch changes at sensible milestones. Don't release every research iteration.
- Never release unqualified experimental work.
- Don't leave accepted production improvements indefinitely unreleased.

upigz has not been released and isn't published (`publish = false` in `Cargo.toml`). Its first
release brings over mibz2's release machinery (`deploy.yml`, `tag.yml`, `release-notes.yml`, the
`just release` recipes) adapted to a crate-only release, and `docs/RELEASING.md` is filled in
then. A release counts as done only when every publication it claims is verified (the registry
lists the version, the GitHub release exists, `CHANGELOG.md` has its entry); a partial
publication is never left unresolved.

**Identity.** Every commit is the maintainer's: Louis Maddox <louismmx@gmail.com>, with no AI
attribution in commits, PR bodies, release notes or metadata. A release workflow's own commit,
made by `github-actions[bot]`, is the repository's release mechanism, not an AI-attributed
commit.

## What stays with the maintainer

These decisions still need the maintainer:
- relaxing an already registered rule, gate or acceptance minimum (RULES C2 and D);
- a run on the i9 or another workstation. The maintainer authorises it by running a recipe
  frozen before the run, and that authorises that run only (RULES.md C5);
- new infrastructure, credentials or spending;
- widening the scope beyond upigz.

Bench design, standing acceptance and adoption policy are not reserved: they follow RULES C6
and its review, recording and prospective-application safeguards. Relaxing an already
registered rule remains governed by RULES C2 and D.

A campaign's rules can reserve more for the maintainer. They cannot give the steward more than
this document and `docs/team/RULES.md` do without the maintainer's own words.

## Standing prohibitions

The steward never:
- reads from, writes to or calls the Hugging Face Hub;
- adds AI attribution;
- changes the allocator (the global allocator stays the system's);
- weakens the exact checks;
- polls, or sets up cron jobs;
- force-pushes;
- treats silence as approval;
- merges an unbenched head.

## Campaigns

Each campaign has an entry document that maps its charter, roles, protocol, state and evidence.
Read it at the start of a session, after context compaction, and before any consequential
decision. Resolve it to a commit and record the revision that a decision used.

There is no campaign yet. The agent team's standing work runs under `docs/team/`.
