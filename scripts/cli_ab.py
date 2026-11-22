"""Paired, interleaved wall-clock rounds over CLI decompressors (stdout to /dev/null).
usage: cli_ab.py <rounds> <file.gz>...   prints "file round arm ns bytes" lines."""
import os, random, subprocess, sys, time, zlib
ARMS = {
    'gzip':        'taskset -c 2 gzip -dc {f}',
    'pigz':        'taskset -c 2 pigz -dc {f}',
    'igzip':       'taskset -c 2 igzip -dc {f}',
    'libdeflate':  'taskset -c 2 libdeflate-gunzip -c {f}',
    'rapidgzip-P1':'taskset -c 2 rapidgzip -P 1 -dc {f}',
    'rapidgzip-P4':'taskset -c 0-3 rapidgzip -P 4 -dc {f}',
}
rounds = int(sys.argv[1]); rng = random.Random(1)
for f in sys.argv[2:]:
    raw = os.path.getsize(f.rsplit('.', 2)[0]) if f.count('.') >= 2 else 0
    names = list(ARMS)
    for r in range(rounds + 1):  # round 0 is warm-up
        rng.shuffle(names)
        for a in names:
            t = time.perf_counter_ns()
            subprocess.run(ARMS[a].format(f=f), shell=True, check=True, stdout=subprocess.DEVNULL)
            print(f, r, a, time.perf_counter_ns() - t, raw, flush=True)
