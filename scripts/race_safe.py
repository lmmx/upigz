"""Safe vs unchecked builds vs rapidgzip, P4, paired shuffled rounds.
usage: race_safe.py <rounds> <file.gz> <raw-bytes>"""
import random, subprocess, sys, time
rounds, f, raw = int(sys.argv[1]), sys.argv[2], int(sys.argv[3])
ARMS = {
    'rapidgzip-P4':     'taskset -c 0-3 rapidgzip -P 4 -dc {f}',
    'upigz-P4-safe':    'taskset -c 0-3 target/release/inflate-ilp zcat {f} 4 1024',
    'upigz-P4-unchecked': 'taskset -c 0-3 target/unchecked/release/inflate-ilp zcat {f} 4 1024',
}
rng = random.Random(4); names = list(ARMS)
for r in range(rounds + 1):
    rng.shuffle(names)
    for a in names:
        t = time.perf_counter_ns()
        subprocess.run(ARMS[a].format(f=f), shell=True, check=True, stdout=subprocess.DEVNULL)
        print(r, a, time.perf_counter_ns() - t, raw, flush=True)
