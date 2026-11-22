# Setup: what remains

The team machinery was copied from lmmx/mibz2 and adapted. Its GitHub objects now exist:

- [x] **The channel:** draft PR #2 (head `team/channel`), never merged.
- [x] **The inboxes:** draft PRs #3 Optimiser, #4 Guardian, #5 Researcher, #6 Observer,
  #7 Pathfinder, #8 MONODROMY; issue #9 Codex. Every offset is 0 (they started empty).
  `coord.py`, the docs and `channel-routing.yml` carry these numbers.
- [x] **The cursors:** orphan branches `team-state/<role>` for all seven roles, each with an
  empty `state/cursor.json` in its role's format (SPEC.<ROLE>.md).
- [x] **The queue's header:** `bench/workstation-queue.txt` declares `# repo: lmmx/upigz` and
  `# channel: 2`. `bench/submit.sh` refuses to submit without a numeric channel there, and
  reads it from the checkout's copy or else origin's default branch: it reaches master only
  when this machinery merges.

Still to do:

- [ ] **1. Register the checkout on the workstation** (Louis): in an up-to-date mibz2 checkout,
  `just workstation-repos . <path to the upigz checkout>`. From then on `just workstation-queue`
  serves both queues in turns, under one machine lock (mibz2's
  `docs/journal/2026-11-22-multi-repo-runner.md`).
- [ ] **2. The roles' wakes.**
  - The Work roles (Optimiser, Guardian, Researcher, Observer): create each Work task from
    `WORK-TASKS.md`, pinned to a master commit that has this machinery, subscribed to its
    inbox PR's webhook.
  - The Claude Code roles (Pathfinder, MONODROMY): Louis opens each session and subscribes it
    to its inbox; put its session id in PROTOCOL.md §1a.
  - Codex: no webhook by design (#9 is an issue); it is launched by hand.
- [ ] **3. Two more branches.** `observatory/intention`, an orphan branch for the Observer's
  record; `evidence`, an orphan branch for submitted workstation bundles (`bench/submit.sh`).
- [ ] **4. The decision record.** Start `docs/journal/decisions.md` (OWNERSHIP.md §5, RULES):
  Louis's decisions for upigz, verbatim. mibz2's stay in mibz2.
- [ ] **5. The gated workflows** (each file's header names its gate):
  - `bench.yml`: once upigz has a CI bench harness and recipe;
  - `site.yml`'s deploy job: once GitHub Pages is set to serve the `gh-pages` branch.
- [ ] **6. The workstation queue's first recipe.** The queue is empty: its first entry is the
  first registered i9 recipe (research-charter/FRONTIER.md F1 or F5).
- [ ] **7. Optional:** mibz2's `CHATGPT.md` (the relay procedure of PROTOCOL.md §7), and its
  release workflows (`deploy.yml`, `tag.yml`, `release-notes.yml`) when upigz first publishes
  (docs/RELEASING.md).
