# Releasing upigz

The release runbook, as a template: upigz has not been released yet (`publish = false` in
`Cargo.toml`). The coordinator follows it for every release, and it is referenced from
`CLAUDE.md`. It is mibz2's runbook (lmmx/mibz2's `docs/RELEASING.md`), cut to what applies to a
Rust crate with no Python package. The steps marked **TODO** are filled in at the first release.

The maintainer's standing instruction (in mibz2, carried over) is that the coordinator releases
itself, without asking first, whenever merged work is worth releasing.

## 1. Is there something worth releasing?

After every merge to master, look at what has merged since the last tag:

```sh
git log <last tag>..master -- src Cargo.toml
```

Worth releasing:
- **A performance gain** shown under its PR's registered rule: the CI bench once it exists, or
  a registered workstation run on the i9 admitted under RULES C5.
- **A memory gain** accepted under PERFORMANCE-POLICY.md §2 ("A memory gain is a gain").
- **A change that is definitely committed to:** a fix users need, or an interface or
  packaging change.

Docs, journal, bench-harness or blog changes alone are not.

## 2. When

- **Release unless the maintainer has said they are asleep or otherwise indisposed.** A broken
  release needs someone to hear of it. Never infer indisposition from silence, a quiet session
  or the time of day (Louis, 17 November 2026, in mibz2).
- **If work becomes releasable while they are indisposed,** release it when they are next
  available, and say so.

## 3. Before

1. **Master is green:** Test and the site.
2. **The last release against master, paired.** **TODO:** the CI bench's dispatch with
   `base=<the last tag's commit SHA>` once `bench.yml` is ungated; until then, a registered
   workstation recipe. It checks that nothing got worse; it is not a gate to re-run until the
   result looks good.
3. **Review the research features** (`Cargo.toml`'s `[features]`: `census`, `unchecked`). A
   feature stays while it has a specific purpose and a removal condition not yet met. Remove
   one whose reason has lapsed in its own PR, never in the release's.

## 4. The release

**TODO** (docs/team/SETUP-TODO.md item 7): bring over mibz2's `deploy.yml` and `tag.yml`,
adapted to the crate alone (release-plz with Trusted Publishing, a `vX.Y.Z` tag), and set
`publish` in `Cargo.toml`. Then:
1. dispatch the release workflow with its `bump`: patch, or minor for an interface change;
2. verify the publication: crates.io lists the version, docs.rs has built it, the GitHub
   release exists;
3. fix any publishing failure with a follow-up patch release. A partial publication is never
   left unresolved.

## 5. Release scaling: the new release against the last, on the workstation

**TODO:** a `bench-release-scaling` recipe (one tarball back, CLAUDE.md), queued for the i9
after each release: both releases at 1 to 20 threads on the registered corpora, paired, with
peak RSS. mibz2's recipe (`just bench-release-scaling`) is the model.

## 6. After

1. **A CHANGELOG PR** with the release's notes: what changed, the paired run's table and its
   CPU, the cost, the caveats, and the release-scaling figures once they are in.
2. **Report the release:**
   - to the maintainer, in one short message;
   - in the channel log, as a LOG on #2;
   - to the Observer, as a CHECKPOINT.
