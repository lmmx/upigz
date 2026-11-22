# bench/: the workstation's infrastructure

No upigz bench recipe exists yet (`.just/bench.just`). This directory holds the machinery a
recipe's run goes through, from lmmx/mibz2:

| file | what it is | origin |
|---|---|---|
| `run-queue.sh`, `await-submit.sh`, `submit.sh`, `bundle.sh` | the workstation runner: the queue, the ready-tag waiter, evidence submission and the one-tarball bundle. They name no repository: `submit.sh` and `await-submit.sh` read `# repo:` and `# channel:` from `workstation-queue.txt` | mibz2 at dd6450e, byte for byte (keep them so: fixes go to mibz2 and are copied back) |
| `test_submit.py`, `test_workstation_queue.py`, `test_run_queue_multi.py` | their tests, on scratch repositories (the `lmmx/mibz2` in them is a fixture name) | the same, byte for byte |
| `workstation-queue.txt` | the queue: empty; it declares `# repo: lmmx/upigz` and `# channel: 2` | mibz2's header |
| `two_arm.py`, `test_two_arm.py`, `two_arm_fixtures/` | a two-arm adoption run's verdict from its records: Holm's gain test, the loss screen, the memory gate and the controls | mibz2's adjudicator; its fixture's arms renamed, its citations of mibz2's registrations kept as `lmmx/mibz2#N` |

The CI bench's report and its PR comment are mibz2's (`bench/report.py`, `bench/comment.py`
there). They are built on its bzip2 harness and corpora, so they aren't copied: upigz's are
written with its own harness, when `.github/workflows/bench.yml`'s gate lifts.
