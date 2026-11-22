ruff-check mode="":
   ruff check . {{mode}}

ruff-fix:
   just ruff-check --fix

ruff-fmt:
   ruff format .

# Type checking
ty *args:
   #!/usr/bin/env bash
   ty check --exit-zero {{args}} 2> >(grep -v "WARN ty is pre-release software" >&2)

t:
   just ty --output-format=concise

# -------------------------------------

# The Python tests: the coordination scripts and the workstation runner (no Python package)
py-test *args:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 -m pytest .github/coordination {{args}}
    for t in bench/test_*.py; do python3 "$t"; done

# -------------------------------------

setup:
   #!/usr/bin/env bash
   uv venv
   source .venv/bin/activate
   uv sync

sync:
   uv sync
