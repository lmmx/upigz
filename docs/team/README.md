# The upigz agent team

The live machinery of the agent team that works on upigz. Its objective is STEWARDSHIP.md's: a
measurably faster, correct production decoder, properly released. MODE.md says the same.

| file | what it is |
|---|---|
| [MODE.md](MODE.md) | the register every role writes in; read first on every run. Changed only by Louis ([why](MODE.RATIONALE.md)) |
| [RULES.md](RULES.md) | who may decide what: the team's standing rules, with STEWARDSHIP.md (`docs/STEWARDSHIP.md`) |
| [PROTOCOL.md](PROTOCOL.md) | how messages move: the channel #2, the inboxes #3-#9, dispatch and format |
| [ROLES.md](ROLES.md) | the roles: Coordinator, Optimiser, Guardian, Researcher, Observer, Pathfinder, MONODROMY, and the Codex delegate |
| [SPEC.md](SPEC.md) | what every Work task's spec has in common, and the command that loads them |
| [WORK-TASKS.md](WORK-TASKS.md) | the only prompts for creating or re-pinning a ChatGPT Work task, each marked known to work or untested |
| `SPEC.<ROLE>.md` | each Work role's own spec: OPTIMIZER, GUARDIAN, RESEARCHER, OBSERVER |
| [SPEC.PATHFINDER.md](SPEC.PATHFINDER.md) | the Pathfinder's spec: a Claude Code session, not a Work task (ROLES.md) |
| [COORDINATOR-FAILOVER.md](COORDINATOR-FAILOVER.md) | owner-activated coordination handoff between Claude and Work, with safety boundaries |
| [OWNERSHIP.md](OWNERSHIP.md) / [PERFORMANCE-POLICY.md](PERFORMANCE-POLICY.md) | how work moves, and what a performance change must show to merge |
| [CI-INCIDENTS.md](CI-INCIDENTS.md) | what to do with a red CI check |
| [DELIVERED.md](DELIVERED.md) | what the team has shipped |
| [research-charter/](research-charter/README.md) | the research lines' charter and the frontier ([FRONTIER.md](research-charter/FRONTIER.md)) |
| [SETUP-TODO.md](SETUP-TODO.md) | what remains to set up: the workstation, the roles' wakes, a few branches |

**Where the rest lives:**
- **Each role's state:** `team-state/<role>`, holding its cursor and records. These branches have no PR, and the workflows' filters run no CI on them.
- **The Observer's record of what the team is doing and why:** `observatory/intention`, an orphan
  branch with its own history.
- **Routing:** `.github/coordination/coord.py`.
- **Louis's requests from ChatGPT:** PROTOCOL.md §7 is the rule; mibz2's `CHATGPT.md` is the
  procedure a ChatGPT conversation follows to send one (not yet copied here).
- **History:** this machinery was built in lmmx/mibz2 (a parallel bzip2 decoder) and copied
  here, adapted, when upigz was seeded. Issue and PR numbers are placeholders (`#2`,
  `#3`, ...) until they exist; a number written `lmmx/mibz2#N` is a precedent
  in mibz2's record, and the dated rulings of Louis's cited throughout were made there. Files
  cited under `docs/journal/` that this repository doesn't have (the compact30 record, the
  dated ownership and handoff entries) are mibz2's. `decisions.md` is mibz2's until upigz's is
  started (SETUP-TODO.md item 5).
