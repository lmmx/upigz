"""The i9 race: upigz against rapidgzip and igzip on one single-member gzip file.

Registered in docs/journal/2026-11-22-i9-race-registration.md before any run; the rule below
is that registration's, and nothing here may change it after data exists.

  python3 bench/i9_race.py run OUT.jsonl --corpus GZ --raw-sha256 HEX --rounds N \
      --upigz BIN --upigz-unchecked BIN --rapidgzip BIN --igzip BIN
  python3 bench/i9_race.py report OUT.jsonl --out PREFIX      (writes PREFIX.json, PREFIX.md)

`run` first decodes the corpus once with every arm, hashing its output against the registered
digest of the uncompressed text (the validation pass, untimed). It then runs rounds + 1 rounds,
each running every arm once in a shuffled order (seeded), output to /dev/null; round 0 is a
warm-up and is never judged. Each timed run records its wall time, user and system CPU time and
peak RSS (wait4).

The rule (the registration's):
- primary: R = time(upigz-P10) / time(rapidgzip-P10), the geometric mean of per-round ratios
  over rounds 1..N, with a 95% t interval on the log scale;
- VALID: every arm byte-exact in the validation pass, every arm with N timed rounds and exit
  status 0, and the A/A ratio time(upigz-P10-aa) / time(upigz-P10) with its 95% interval inside
  [0.95, 1.05];
- VALID PASS: valid, and R's interval lies wholly below 1.00 (upigz decodes faster at 10
  threads). VALID FAIL: valid, and it does not. INVALID: not valid.
- reported, not decided: the same ratio at 4, 8 and 20 threads; igzip (one thread) against
  upigz-P10; the unchecked build against the safe one at 10 threads; GB/s, CPU time and peak
  RSS for every arm.
"""

import hashlib
import json
import math
import os
import random
import subprocess
import sys
import tempfile
import time

THREADS = (4, 8, 10, 20)
PRIMARY = 10
SEED = 20261122
T975 = {1: 12.71, 2: 4.30, 3: 3.18, 4: 2.78, 5: 2.57, 6: 2.45, 7: 2.36, 8: 2.31, 9: 2.26, 10: 2.23,
        12: 2.18, 15: 2.13, 20: 2.09, 25: 2.06, 30: 2.04, 40: 2.02, 60: 2.00, 120: 1.98}


def arms(a):
    gz = a["corpus"]
    out = {}
    for t in THREADS:
        out[f"upigz-P{t}"] = [a["upigz"], "zcat", gz, str(t), "1024"]
        out[f"rapidgzip-P{t}"] = [a["rapidgzip"], "-P", str(t), "--verify", "-dc", gz]
    out[f"upigz-P{PRIMARY}-aa"] = list(out[f"upigz-P{PRIMARY}"])
    out[f"upigz-unchecked-P{PRIMARY}"] = [a["upigz_unchecked"], "zcat", gz, str(PRIMARY), "1024"]
    out["igzip-1t"] = [a["igzip"], "-dc", gz]
    return out


def timed(argv, stdout):
    # (stderr to a file, not a pipe: a pipe read only after wait4 could fill and stall the run)
    with tempfile.TemporaryFile() as e:
        t0 = time.perf_counter_ns()
        p = subprocess.Popen(argv, stdout=stdout, stderr=e)
        _, status, ru = os.wait4(p.pid, 0)
        ns = time.perf_counter_ns() - t0
        p.returncode = os.waitstatus_to_exitcode(status)
        e.seek(0)
        err = e.read().decode(errors="replace")[-400:]
    return {"ns": ns, "status": p.returncode, "user_s": ru.ru_utime, "sys_s": ru.ru_stime,
            "maxrss_kib": ru.ru_maxrss, "stderr": err}


def validate(argv):
    p = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    h, n = hashlib.sha256(), 0
    for b in iter(lambda: p.stdout.read(1 << 20), b""):
        h.update(b)
        n += len(b)
    err = p.stderr.read().decode(errors="replace")[-400:]
    return {"status": p.wait(), "sha256": h.hexdigest(), "bytes": n, "stderr": err}


def run(args):
    a = {}
    it = iter(args[1:])
    out = args[0]
    for k in it:
        a[k.lstrip("-").replace("-", "_")] = next(it)
    rounds = int(a["rounds"])
    arm = arms(a)
    with open(out, "w") as f:
        def rec(x):
            f.write(json.dumps(x) + "\n")
            f.flush()
        rec({"kind": "setup", "arms": arm, "rounds": rounds, "seed": SEED, "raw_sha256": a["raw_sha256"],
             "corpus": a["corpus"], "corpus_bytes": os.path.getsize(a["corpus"])})
        for name, argv in arm.items():
            v = validate(argv)
            v.update(kind="validate", arm=name, exact=v["status"] == 0 and v["sha256"] == a["raw_sha256"])
            rec(v)
            print(f"validate {name}: {'byte-exact' if v['exact'] else 'NOT EXACT'} ({v['bytes']} bytes)", flush=True)
        rng = random.Random(SEED)
        names = list(arm)
        with open(os.devnull, "wb") as null:
            for r in range(rounds + 1):
                rng.shuffle(names)
                for name in names:
                    x = timed(arm[name], null)
                    x.update(kind="run", round=r, arm=name)
                    rec(x)
                print(f"round {r}/{rounds} done", flush=True)


def t975(df):
    return T975[min((k for k in T975 if k >= df), default=120)]


def ratio(runs, a, b, rounds):
    lr = []
    for r in range(1, rounds + 1):
        x, y = runs.get((r, a)), runs.get((r, b))
        if x and y:
            lr.append(math.log(x["ns"] / y["ns"]))
    n = len(lr)
    if n < 2:
        return None
    m = sum(lr) / n
    sd = math.sqrt(sum((v - m) ** 2 for v in lr) / (n - 1))
    h = t975(n - 1) * sd / math.sqrt(n)
    return {"ratio": round(math.exp(m), 4), "lo": round(math.exp(m - h), 4), "hi": round(math.exp(m + h), 4), "rounds": n}


def report(args):
    path, prefix = args[0], args[args.index("--out") + 1]
    setup, val, runs = None, {}, {}
    for line in open(path):
        x = json.loads(line)
        if x["kind"] == "setup":
            setup = x
        elif x["kind"] == "validate":
            val[x["arm"]] = x
        elif x["kind"] == "run":
            runs[(x["round"], x["arm"])] = x
    rounds, names, raw = setup["rounds"], list(setup["arms"]), None
    problems = []
    for n in names:
        if not val.get(n, {}).get("exact"):
            problems.append(f"{n} was not byte-exact in the validation pass")
        got = [runs.get((r, n)) for r in range(1, rounds + 1)]
        if any(g is None for g in got):
            problems.append(f"{n} lacks timed rounds")
        elif any(g["status"] != 0 for g in got):
            problems.append(f"{n} exited non-zero in a timed round")
        raw = raw or val.get(n, {}).get("bytes")
    aa = ratio(runs, f"upigz-P{PRIMARY}-aa", f"upigz-P{PRIMARY}", rounds)
    if aa is None or not (0.95 <= aa["lo"] and aa["hi"] <= 1.05):
        problems.append(f"the A/A interval {aa} is not inside [0.95, 1.05]")
    prim = ratio(runs, f"upigz-P{PRIMARY}", f"rapidgzip-P{PRIMARY}", rounds)
    if prim is None:
        problems.append("no primary ratio")
    valid = not problems
    verdict = ("VALID PASS" if prim["hi"] < 1.0 else "VALID FAIL") if valid else "INVALID"
    per = {}
    for n in names:
        rs = [runs[(r, n)] for r in range(1, rounds + 1) if (r, n) in runs]
        if not rs:
            continue
        ns = sorted(x["ns"] for x in rs)
        med = ns[len(ns) // 2]
        per[n] = {"gbps_median": round(raw / med, 3) if raw else None,
                  "cpu_s_median": round(sorted(x["user_s"] + x["sys_s"] for x in rs)[len(rs) // 2], 3),
                  "maxrss_mib_max": round(max(x["maxrss_kib"] for x in rs) / 1024, 1)}
    reported = {f"upigz-P{t}/rapidgzip-P{t}": ratio(runs, f"upigz-P{t}", f"rapidgzip-P{t}", rounds) for t in THREADS}
    reported["igzip-1t/upigz-P10"] = ratio(runs, "igzip-1t", f"upigz-P{PRIMARY}", rounds)
    reported["upigz-unchecked-P10/upigz-P10"] = ratio(runs, f"upigz-unchecked-P{PRIMARY}", f"upigz-P{PRIMARY}", rounds)
    out = {"verdict": verdict, "valid": valid, "pass": verdict == "VALID PASS", "problems": problems,
           "primary": prim, "aa": aa, "reported": reported, "arms": per, "rounds": rounds, "raw_bytes": raw}
    with open(prefix + ".json", "w") as f:
        json.dump(out, f, indent=2)
    md = [f"# i9 race: {verdict}", "",
          f"Primary, time(upigz-P10) / time(rapidgzip-P10): **{prim['ratio'] if prim else 'n/a'}** "
          f"[{prim['lo'] if prim else ''}, {prim['hi'] if prim else ''}] over {rounds} rounds. A/A {aa}.", ""]
    if problems:
        md += ["Problems:", ""] + [f"- {p}" for p in problems] + [""]
    md += ["| ratio (reported) | value [95% CI] |", "|---|---|"]
    md += [f"| {k} | {v['ratio']} [{v['lo']}, {v['hi']}] |" if v else f"| {k} | n/a |" for k, v in reported.items()]
    md += ["", "| arm | GB/s (median) | CPU s (median) | peak RSS MiB |", "|---|---|---|---|"]
    md += [f"| {n} | {v['gbps_median']} | {v['cpu_s_median']} | {v['maxrss_mib_max']} |" for n, v in per.items()]
    with open(prefix + ".md", "w") as f:
        f.write("\n".join(md) + "\n")
    print(verdict)
    return 0 if valid else 1


if __name__ == "__main__":
    if len(sys.argv) < 3 or sys.argv[1] not in ("run", "report"):
        sys.exit(__doc__)
    sys.exit(run(sys.argv[2:]) if sys.argv[1] == "run" else report(sys.argv[2:]))
