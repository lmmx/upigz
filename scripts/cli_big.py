"""Paired rounds on one big file: single-thread CLIs vs rapidgzip at P threads.
usage: cli_big.py <rounds> <file.gz> <raw-bytes>"""
import random, subprocess, sys, time
rounds, f, raw = int(sys.argv[1]), sys.argv[2], int(sys.argv[3])
ARMS = {
    'libdeflate': 'taskset -c 2 libdeflate-gunzip -c {f}',
    'igzip': 'taskset -c 2 igzip -dc {f}',
    'rapidgzip-P1': 'taskset -c 2 rapidgzip -P 1 -dc {f}',
    'rapidgzip-P2': 'taskset -c 2,3 rapidgzip -P 2 -dc {f}',
    'rapidgzip-P3': 'taskset -c 1-3 rapidgzip -P 3 -dc {f}',
    'rapidgzip-P4': 'taskset -c 0-3 rapidgzip -P 4 -dc {f}',
}
rng = random.Random(2); names = list(ARMS)
for r in range(rounds + 1):
    rng.shuffle(names)
    for a in names:
        t = time.perf_counter_ns()
        subprocess.run(ARMS[a].format(f=f), shell=True, check=True, stdout=subprocess.DEVNULL)
        print(r, a, time.perf_counter_ns() - t, raw, flush=True)
