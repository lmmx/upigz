# Workstation bench recipes: none yet.
#
# Each recipe here follows CLAUDE.md's "Workstation recipes: one tarball back": one new
# timestamped directory per attempt with its own console log, nothing written to the home
# directory, and `bash bench/bundle.sh "$DIR"` at the end. A recipe whose run is evidence is run
# as `just submit <recipe>` (.just/submit.just), queued in bench/workstation-queue.txt and
# frozen, with its decision rule, before its data (docs/team/research-charter/README.md).
#
# The first candidates are research-charter/FRONTIER.md's F1 (the re-decode thread sweep) and
# F5 (single thread against libdeflate). The research host's harnesses are scripts/ (see the
# journals); mibz2's recipes (its .just/bench.just) show the shape.
