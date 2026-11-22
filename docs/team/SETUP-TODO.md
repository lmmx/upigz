# Setup: what must exist before the team machinery runs

The team machinery was copied from lmmx/mibz2 before any of its GitHub objects existed here.
Until each item below is done, the docs use placeholders (`#<channel>`, `#<optimizer-inbox>`,
`<offset>`, `<pathfinder-session-id>`) and the parts that need it are gated. Each item says
what to fill in once it exists.

- [ ] **1. The channel PR.** One standing draft PR, never merged, locked (PROTOCOL.md §1).
  Then:
  - set `CHANNEL` in `.github/coordination/coord.py`;
  - set `channel=` in `bench/submit.sh`, and the channel in `bench/await-submit.sh`'s notice;
  - replace `#<channel>` in `docs/team/`, `CLAUDE.md` and `.claude/skills/`;
  - remove the `if: false` gate on the tripwire job in `.github/workflows/channel-routing.yml`.
- [ ] **2. The role inboxes.** One quiet draft PR per role, never merged: Optimiser, Guardian,
  Researcher, Observer, Pathfinder. For each, its opening comment is its offset. Then set each
  role's `inbox` and `offset` in `coord.py`'s `ROLES`, and replace `#<role-inbox>` and
  `<offset>` in `docs/team/SPEC.<ROLE>.md`, `PROTOCOL.md` and `ROLES.md`.
- [ ] **3. The branches.**
  - `team-state/<role>` for each role, holding `state/cursor.json` in its role's format
    (SPEC.<ROLE>.md);
  - `observatory/intention`, an orphan branch, for the Observer;
  - `evidence`, an orphan branch, for submitted workstation bundles (`bench/submit.sh`).
- [ ] **4. The Work tasks and the Pathfinder's session.** Create each Work task from
  `WORK-TASKS.md`, pinned to a master commit that has this machinery. Put the Pathfinder's
  session id in PROTOCOL.md §1a.
- [ ] **5. The decision record.** Start `docs/journal/decisions.md` (OWNERSHIP.md §5, RULES):
  Louis's decisions for upigz, verbatim. mibz2's stay in mibz2.
- [ ] **6. The gated workflows** (each file's header names its gate):
  - `bench.yml`: once upigz has a CI bench harness and recipe;
  - `site.yml`'s deploy and preview jobs: once GitHub Pages serves the `gh-pages` branch;
  - `channel-routing.yml`'s tripwire: item 1.
- [ ] **7. The workstation queue's first recipe.** `bench/workstation-queue.txt` is empty: the
  first entry is the first registered i9 recipe (research-charter/FRONTIER.md F1 or F5).
- [ ] **8. Optional:** mibz2's `CHATGPT.md` (the relay procedure of PROTOCOL.md §7), and its
  release workflows (`deploy.yml`, `tag.yml`, `release-notes.yml`) when upigz first publishes
  (docs/RELEASING.md).
