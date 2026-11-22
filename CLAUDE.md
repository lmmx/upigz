# upigz

**Ownership first:** [`docs/team/OWNERSHIP.md`](docs/team/OWNERSHIP.md). We are the engineering team that owns upigz, and shipping is the objective. Correctness and memory safety never bend. Performance trade-offs follow [`docs/team/PERFORMANCE-POLICY.md`](docs/team/PERFORMANCE-POLICY.md). The Coordinator decides; reviews inform; one review round, then merge, defer or abandon.

upigz is a parallel gzip/DEFLATE decoder in Rust, aimed at the large single-member file (the
Wikidata JSON dump is one 157 GB member). Its team machinery, rules and workstation runner were
seeded from [lmmx/mibz2](https://github.com/lmmx/mibz2), a parallel bzip2 decoder; the two
repositories share the workstation runner, and nothing else.

## Where upigz stands

From the journals (`docs/journal/`, 2026-11-22; the frontier is
[`docs/team/research-charter/FRONTIER.md`](docs/team/research-charter/FRONTIER.md)):
- **The single-thread kernel** (`src/fast.rs`) is libdeflate-style. On the research host it is
  second in memory: ahead of ISA-L, zlib-rs and zlib-ng, behind libdeflate by 4–12% (the
  `unchecked` build) or about 20% (the safe build).
- **The parallel decoder** (`src/par.rs`, `upigz zcat`) splits one member into chunks: a finder
  for block starts, pass 1 to 16-bit symbols with window markers, windows handed on in order,
  then resolution and CRC in parallel, with bounded buffers. On the research host's 4 cores it
  decodes the 1.34 GB single-member Wikidata file at 1.21 GB/s (unchecked) or 1.15 GB/s (safe),
  against rapidgzip -P4's 0.80.
- **Interleaving members within one thread** (mibz2's first idea) is closed: it doesn't pay on
  real gzip data.
- **The speculative-decode theorem** holds under `spectest` (16,046 block starts, no
  counterexample). Its chaining lemma means a finder false positive can cost time, never
  correctness.
- **Open, in order:** the re-decode bet (F1), pass 1's cost alone (F2), the slow 16-bit pass-1
  kernel (F3), parallel robustness (F4: multi-member files, finder recovery, short chunks,
  zlib error parity, fuzzing), closing on libdeflate single-thread (F5), and the Lean proof (F6).
- **The research host** is a shared 4-vCPU cloud VM with no hardware PMU. Its figures are
  indicative only.

## Standing rules

- **The i9 decides.** A same-binary A/B on the research host chooses what to try next. A
  default, a merge on performance grounds or a published ranking is decided on the i9-10900K
  through a registered recipe (RULES C5, C6; [the research charter](docs/team/research-charter/README.md)).
- **Decision rules before data.** Every experiment's rule is posted on #2 (RULES C1a), or frozen
  in a registration for an adoption question (C6), before its data. It never moves after the
  data is seen (C2).
- **`unsafe`.** New `unsafe` code needs a written safety argument; Miri, differential, fuzz and
  mutation testing; and an adversarial review by an agent other than its author (RULES B5). The
  `unchecked` feature's fast-loop accesses (`src/fast.rs`) are research: the default build is
  the safe one, and closing the safe-build tax without `unsafe` is on the frontier (F5).
- **No allocator changes.** The global allocator stays the system's.
- **Exact checks never weaken:** byte-exact output against the reference before any timing,
  the CRC-32 and ISIZE checked on every member, `spectest` green.
- **Attribution.** Every commit is authored and committed as Louis Maddox
  <louismmx@gmail.com>. No AI attribution anywhere: commits, PR bodies, release notes, files or
  metadata.
- **No polling, no cron jobs, no force-pushes, no silence taken as approval.**

## The team channel and the inboxes

The channel is draft PR #2; the role inboxes are draft PRs #3 Optimiser, #4 Guardian,
#5 Researcher, #6 Observer, #7 Pathfinder and #8 MONODROMY, and issue #9 for the Codex delegate.
Route assignments with `.github/coordination/coord.py`, never from memory. The #9 inbox is an
issue and **does not wake Codex**. During a Claude quota pause, an explicitly owner-activated
acting Coordinator may operate this same role, never concurrently; follow
`docs/team/COORDINATOR-FAILOVER.md` and attribute every acting LOG with `ACTOR:`.

**Waking a role that runs as a Claude Code session** (the Pathfinder, MONODROMY): a post on #2
does not wake it. After posting, send its session a direct `send_message` naming the comment
ids waiting on it (`docs/team/PROTOCOL.md` §1a). Never ask Louis to relay.

- **At session start, and every time the coordinator wakes:**
  `python3 .github/coordination/coord.py audit` lists any misrouted message, and fails on any
  not yet in its `RESOLVED` record. Correct a misroute, log it, then add it to `RESOLVED` with
  how it was handled. The tripwire (`channel-routing.yml`) also records each one as a failing
  check on #2, but wakes nobody.
- **Before posting an assignment:** `coord.py route --to <role> --type CHECKPOINT` gives the
  destination: the role's inbox, or #2 as `TYPE: QUEUED` while the role is busy, or an older
  QUEUED item to dispatch first.
- **After posting:** `coord.py verify --comment-id <id> --to <role> --type <type>` must pass.
  A comment that exists is not a message delivered.
- **Before any merge:** `coord.py head-sha --pr <n> --sha <sha>` must pass, and its SHA is the
  merge's expected head (the merge tool's `expectedHeadSha`; it silently drops an unknown name
  such as `sha`). After the merge, `coord.py head-sha --pr <n> --sha <sha> --merged` must pass.
- **Green means both kinds of check:** the Actions check runs *and* the legacy commit statuses
  (`gh api repos/lmmx/upigz/commits/<sha>/status`). A status in `error` or `pending` on the head
  is not green.
- **When not to wait for CI** (Louis's rulings in mibz2, 17–18 November 2026): a PR that only
  touches `bench/workstation-queue.txt`; a head whose tree differs from a green commit only in
  files no check reads; a job whose inputs this PR's diff can't reach. Name the jobs not waited
  for in the merge's LOG, with `git diff --stat <commit> <head>` as the record. In doubt, wait.
  GitHub's macOS runners never hold a merge (upigz has no macOS job yet).

## A red CI check

Follow [`docs/team/CI-INCIDENTS.md`](docs/team/CI-INCIDENTS.md): classify each failed job from
its log (A, the change failed; B, setup failed before the project ran; C, flaky; D,
unexplained), re-run only the failed job, paced to the outage. The green-head gate in
`docs/STEWARDSHIP.md` holds.

## Releases

upigz isn't published yet (`publish = false`). The coordinator is its release steward
(`docs/STEWARDSHIP.md`); [`docs/RELEASING.md`](docs/RELEASING.md) is the runbook, to be filled
in with the release machinery at the first release.

## Workstation recipes: one tarball back

Every `just` recipe the maintainer runs on the i9 and sends results back from:
- **Writes everything into one new directory per attempt,** never reused (`mkdir`, not
  `mkdir -p`, on a timestamped path), its own console log included:
  `exec > >(tee -a "$DIR/console.log") 2>&1` right after the directory is made. The maintainer
  runs the bare `just` command, with no `| tee`.
- **Writes nothing to the maintainer's home directory itself.** Results go in the checkout
  (`target/`). The queue's logs and once-per-commit markers are under
  `${XDG_STATE_HOME:-~/.local/state}/upigz/workstation/` (`logs/`, `ran/`); anything disposable
  is cache, under `${XDG_CACHE_HOME:-~/.cache}/upigz/`.
- **Ends with `bash bench/bundle.sh "$DIR"`,** which packs the directory into one
  `DIR.tar.gz` beside it (no symbolic links, no file over 50 MB) and prints the path.
- **Is given to the maintainer as the git commands to get the branch and the `just` command,**
  nothing else. For a frozen recipe whose run is evidence, the command is
  `just submit <recipe>`: it runs the recipe unchanged, archives the bundle on the `evidence`
  branch and posts its identity and digest on #2 (RULES C5). If the submission fails, the
  bundle is kept and `just submit-bundle <tarball>` retries it. The coordinator reads the bundle
  from the `evidence` branch at the commit the post names, and checks its SHA-256.

**The runner is shared with mibz2.** `bench/run-queue.sh`, `bench/await-submit.sh`,
`bench/submit.sh`, `bench/bundle.sh` and `.just/submit.just` are mibz2's, byte for byte: fix them
there and copy them back. One runner serves every checkout listed in
`${XDG_CONFIG_HOME:-~/.config}/workstation/repos` in turns, one run each, under one machine lock
(`${XDG_STATE_HOME:-~/.local/state}/workstation/queue.lock`). This repository's queue,
tags, evidence branch and channel are its own: `bench/workstation-queue.txt` declares
`# repo: lmmx/upigz` and `# channel: 2`, and `submit.sh` refuses to submit without them.

**Unattended:** `just submit-when-ready <recipe> <branch>` waits for a `ready/<recipe>` tag on
origin, runs `just submit <recipe>` once at exactly the tagged commit, then suspends;
`hold/<recipe>` stops it. The coordinator sets the tag only once the recipe is frozen, reviewed
and green, through the dispatch-only `.github/workflows/run-signal.yml` (`signal`: ready, hold
or clear). A run is queued only once `git ls-remote --tags origin ready/<recipe>` shows the
commit: a dispatch is not a tag. Prepared runs wait in `bench/workstation-queue.txt`
(PROTOCOL.md §9), and `just workstation-queue` runs every ready entry once, then suspends.
Before setting a ready tag, check the queue file's history and the `evidence` branch, so a run
is never repeated by accident.

## Decisions: decide, record why, carry on

The Coordinator decides within the team's authority, records why in one LOG on #2, and
proceeds. It asks the Guardian only about the admissibility of a registered gate or a safety
call, and proceeds if no HOLD comes within one assignment cycle. Louis is asked only for what
RULES D reserves to him (his workstation runs, new infrastructure or spending, role and rule
changes), or for facts only he has. Experiment and bench design, merge acceptance and adoption
policy are delegated as a class (RULES C6): an adoption registration is frozen before its data
with one review; any other experiment needs only a decision rule posted on #2 first (C1a).

## The agent team

The team's live machinery is `docs/team/`: `MODE.md` (the register; reload it at session start),
`RULES.md` (who may decide what), `PROTOCOL.md` (how messages move), `ROLES.md` and the specs,
and `SETUP-TODO.md` (what remains to set up). ChatGPT Work task prompts come only from
`docs/team/WORK-TASKS.md`, copied with their placeholders filled in, given with the task's
name, model and effort beside them. The Observer (#6) keeps its account on the orphan branch
`observatory/intention`; dispatch it one batched CHECKPOINT per wake, listing the material
moves.

Publishing a page of the site (a blog post above all): the `publish-page` skill. Commissioning a
post from another agent: `coordinate-blog-post`. Writing one: `write-blog-post`.
