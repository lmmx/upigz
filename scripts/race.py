"""Paired, shuffled rounds of whole CLI decompressions (stdout to /dev/null).
usage: race.py <rounds> <file.gz> <raw-bytes>"""
import random, subprocess, sys, time
rounds, f, raw = int(sys.argv[1]), sys.argv[2], int(sys.argv[3])
U = 'target/unchecked/release/inflate-ilp zcat {f} {p} {c}'
ARMS = {
    'igzip-1t':            'taskset -c 2 igzip -dc {f}',
    'rapidgzip-P4':        'taskset -c 0-3 rapidgzip -P 4 -dc {f}',
    'rapidgzip-P4-verify': 'taskset -c 0-3 rapidgzip -P 4 --verify -dc {f}',
    'upigz-P2-2M':         'taskset -c 2,3 ' + U.replace('{p}', '2').replace('{c}', '2048'),
    'upigz-P3-2M':         'taskset -c 1-3 ' + U.replace('{p}', '3').replace('{c}', '2048'),
    'upigz-P4-1M':         'taskset -c 0-3 ' + U.replace('{p}', '4').replace('{c}', '1024'),
    'upigz-P4-2M':         'taskset -c 0-3 ' + U.replace('{p}', '4').replace('{c}', '2048'),
}
rng = random.Random(3); names = list(ARMS)
for r in range(rounds + 1):
    rng.shuffle(names)
    for a in names:
        t = time.perf_counter_ns()
        subprocess.run(ARMS[a].format(f=f), shell=True, check=True, stdout=subprocess.DEVNULL)
        print(r, a, time.perf_counter_ns() - t, raw, flush=True)
