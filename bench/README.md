# bench/: the workstation's infrastructure

No upigz bench recipe exists yet (`.just/bench.just`). This directory holds the machinery a
recipe's run goes through, from lmmx/mibz2:

| file | what it is | origin |
|---|---|---|
| `run-queue.sh`, `await-submit.sh`, `submit.sh`, `bundle.sh` | the workstation runner: the queue, the ready-tag waiter, evidence submission and the one-tarball bundle. They name no repository: `submit.sh` and `await-submit.sh` read `# repo:` and `# channel:` from `workstation-queue.txt` | mibz2 at dd6450e, byte for byte (keep them so: fixes go to mibz2 and are copied back) |
| `test_submit.py`, `test_workstation_queue.py`, `test_run_queue_multi.py` | their tests, on scratch repositories (the `lmmx/mibz2` in them is a fixture name) | the same, byte for byte |
| `workstation-queue.txt` | the queue: empty; it declares `# repo: lmmx/upigz` and `# channel: 2` | mibz2's header |
| `two_arm.py`, `test_two_arm.py`, `two_arm_fixtures/` | a two-arm adoption run's verdict from its records | mibz2, unchanged |
| `report.py` | mibz2's CI bench report (its arms, corpora and full-dump model are bzip2's). Kept as the template for upigz's; nothing calls it until `.github/workflows/bench.yml`'s gate lifts | mibz2, unchanged |
| `comment.py` | edits one benchmark comment on a PR in place | mibz2, its marker renamed |
