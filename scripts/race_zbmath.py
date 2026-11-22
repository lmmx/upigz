"""Paired, shuffled rounds on the zbMATH Open KG dump (one 10.77 GB gzip member), whole
decodes to /dev/null, recording wall time, CPU time and peak RSS (wait4). Round 0 is a
warm-up. usage: race_zbmath.py ROUNDS GZ RAPIDGZIP  > runs.txt (round arm ns bytes cpu_s rss_kib)"""
import os, random, subprocess, sys, time
rounds, gz, rg = int(sys.argv[1]), sys.argv[2], sys.argv[3]
RAW = 10765271040
ARMS = {
    "upigz-P4": ["taskset", "-c", "0-3", "target/release/upigz", "zcat", gz, "4", "1024"],
    "upigz-unchecked-P4": ["taskset", "-c", "0-3", "target/unchecked/release/upigz", "zcat", gz, "4", "1024"],
    "rapidgzip-P4": ["taskset", "-c", "0-3", rg, "-P", "4", "--verify", "-dc", gz],
    "rapidgzip-P1": ["taskset", "-c", "2", rg, "-P", "1", "--verify", "-dc", gz],
    "igzip-1t": ["taskset", "-c", "2", "igzip", "-dc", gz],
}
rng = random.Random(7); names = list(ARMS)
with open(os.devnull, "wb") as null:
    for r in range(rounds + 1):
        rng.shuffle(names)
        for a in names:
            t = time.perf_counter_ns()
            p = subprocess.Popen(ARMS[a], stdout=null)
            _, st, ru = os.wait4(p.pid, 0)
            ns = time.perf_counter_ns() - t
            assert os.waitstatus_to_exitcode(st) == 0, a
            print(r, a, ns, RAW, round(ru.ru_utime + ru.ru_stime, 2), ru.ru_maxrss, flush=True)
