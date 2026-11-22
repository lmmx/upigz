//! Paired, interleaved A/B over DEFLATE decoders, one thread.
//!
//! usage: upigz <file.gz> <reference-raw-file> <rounds> <arm,arm,...>
//!
//! Members come from `<file.gz>.idx` ("comp_off comp_len raw_len" per line) when present,
//! otherwise the file is one member. Every arm decodes every member's raw DEFLATE body into
//! a preallocated buffer; no arm checks CRCs. Each round runs every arm once, in a shuffled
//! order, and prints one line per arm: `round arm ns bytes`.

mod fast;
mod inflate;
mod spec;
mod par;

use inflate::{decode_group, Stream, IN_PAD, SLACK};
use std::time::Instant;

struct Member {
    body: usize, // offset of the DEFLATE body in the file
    end: usize,  // offset just past the member (trailer included)
    raw: usize,
    out_off: usize, // offset of the member's output in the reference
}

fn gz_body(f: &[u8], at: usize) -> usize {
    assert!(f[at] == 0x1f && f[at + 1] == 0x8b && f[at + 2] == 8, "not gzip at {at}");
    let flg = f[at + 3];
    let mut p = at + 10;
    if flg & 4 != 0 {
        let xlen = u16::from_le_bytes([f[p], f[p + 1]]) as usize;
        p += 2 + xlen;
    }
    for bit in [8u8, 16] {
        if flg & bit != 0 {
            while f[p] != 0 {
                p += 1;
            }
            p += 1;
        }
    }
    if flg & 2 != 0 {
        p += 2;
    }
    p
}

fn members(path: &str, f: &[u8]) -> Vec<Member> {
    let mut v = Vec::new();
    let mut out_off = 0;
    match std::fs::read_to_string(format!("{path}.idx")) {
        Ok(idx) => {
            for l in idx.lines() {
                let n: Vec<usize> = l.split(' ').map(|x| x.parse().unwrap()).collect();
                v.push(Member { body: gz_body(f, n[0]), end: n[0] + n[1], raw: n[2], out_off });
                out_off += n[2];
            }
        }
        Err(_) => {
            let raw = u32::from_le_bytes(f[f.len() - 4..].try_into().unwrap()) as usize;
            v.push(Member { body: gz_body(f, 0), end: f.len(), raw, out_off });
        }
    }
    v
}

// ---- baselines -------------------------------------------------------------------------

extern "C" {
    fn calloc(n: usize, s: usize) -> *mut core::ffi::c_void;
    fn free(p: *mut core::ffi::c_void);
}
unsafe extern "C" fn za(_: *mut core::ffi::c_void, n: u32, s: u32) -> *mut core::ffi::c_void {
    calloc(n as usize, s as usize)
}
unsafe extern "C" fn zf(_: *mut core::ffi::c_void, p: *mut core::ffi::c_void) {
    free(p)
}

struct Zng(Box<libz_ng_sys::z_stream>);
impl Zng {
    fn new() -> Self {
        let mut s: Box<libz_ng_sys::z_stream> = Box::new(libz_ng_sys::z_stream {
            next_in: core::ptr::null_mut(),
            avail_in: 0,
            total_in: 0,
            next_out: core::ptr::null_mut(),
            avail_out: 0,
            total_out: 0,
            msg: core::ptr::null_mut(),
            state: core::ptr::null_mut(),
            zalloc: za,
            zfree: zf,
            opaque: core::ptr::null_mut(),
            data_type: 0,
            adler: 0,
            reserved: 0,
        });
        assert_eq!(unsafe { libz_ng_sys::zng_inflateInit2(&mut *s, -15) }, 0);
        Zng(s)
    }
    fn decode(&mut self, inp: &[u8], out: &mut [u8]) -> usize {
        let s = &mut *self.0;
        unsafe { libz_ng_sys::inflateReset(s) };
        s.next_in = inp.as_ptr() as *mut u8;
        s.avail_in = inp.len() as u32;
        s.next_out = out.as_mut_ptr();
        s.avail_out = out.len() as u32;
        let r = unsafe { libz_ng_sys::inflate(s, 4) };
        assert_eq!(r, 1, "zlib-ng: {r}");
        s.total_out as usize
    }
}

struct Zrs(Box<libz_rs_sys::z_stream>);
impl Zrs {
    fn new() -> Self {
        let mut s: Box<libz_rs_sys::z_stream> = Box::new(unsafe { core::mem::zeroed() });
        let r = unsafe {
            libz_rs_sys::inflateInit2_(
                &mut *s,
                -15,
                libz_rs_sys::zlibVersion(),
                core::mem::size_of::<libz_rs_sys::z_stream>() as i32,
            )
        };
        assert_eq!(r, 0);
        Zrs(s)
    }
    fn decode(&mut self, inp: &[u8], out: &mut [u8]) -> usize {
        let s = &mut *self.0;
        unsafe { libz_rs_sys::inflateReset(s) };
        s.next_in = inp.as_ptr();
        s.avail_in = inp.len() as u32;
        s.next_out = out.as_mut_ptr();
        s.avail_out = out.len() as u32;
        let r = unsafe { libz_rs_sys::inflate(s, 4) };
        assert_eq!(r, 1, "zlib-rs: {r}");
        s.total_out as usize
    }
}

extern "C" {
    fn isal_shim_new() -> *mut core::ffi::c_void;
    fn isal_shim_raw(s: *mut core::ffi::c_void, inp: *const u8, in_len: u32, out: *mut u8, out_len: u32) -> i64;
}
struct Isal(*mut core::ffi::c_void);
impl Isal {
    fn decode(&mut self, inp: &[u8], out: &mut [u8]) -> usize {
        let n = unsafe { isal_shim_raw(self.0, inp.as_ptr(), inp.len() as u32, out.as_mut_ptr(), out.len() as u32) };
        assert!(n >= 0, "isa-l failed");
        n as usize
    }
}

// ---- arms ------------------------------------------------------------------------------

enum Arm {
    Ours { k: usize, b: usize, rolled: bool },
    Fast(fast::Decoder),
    Libdeflate(libdeflater::Decompressor),
    LibdeflateThreads(usize),
    ZlibNg(Zng),
    Isal(Isal),
    ZlibRs(Zrs),
    Miniz(Box<miniz_oxide::inflate::core::DecompressorOxide>),
}

fn arm(name: &str) -> Arm {
    // ours-k<K>[-b<B>][-r] (-r: the rolled loop), e.g. ours-k1 (sequential), ours-k4, ours-k4-b4; "-aa" suffix = duplicate
    let base = name.trim_end_matches("-aa");
    let rolled = base.ends_with("-r");
    let base = base.trim_end_matches("-r");
    if let Some(r) = base.strip_prefix("ours-k") {
        let mut it = r.split("-b");
        let k = it.next().unwrap().parse().unwrap();
        let b = it.next().map_or(1, |x| x.parse().unwrap());
        return Arm::Ours { k, b, rolled };
    }
    if let Some(t) = base.strip_prefix("libdeflate-t") {
        return Arm::LibdeflateThreads(t.parse().unwrap());
    }
    match base {
        "fast" => Arm::Fast(fast::Decoder::default()),
        "libdeflate" => Arm::Libdeflate(libdeflater::Decompressor::new()),
        "zlib-ng" => Arm::ZlibNg(Zng::new()),
        "isal" => Arm::Isal(Isal(unsafe { isal_shim_new() })),
        "zlib-rs" => Arm::ZlibRs(Zrs::new()),
        "miniz" => Arm::Miniz(Box::default()),
        _ => panic!("unknown arm {name}"),
    }
}

fn ours<const B: usize>(k: usize, rolled: bool, inp: &[u8], ms: &[Member], outs: &mut [Vec<u8>]) {
    for (g, og) in ms.chunks(k).zip(outs.chunks_mut(k)) {
        let mut ss: Vec<Stream<'_>> = g
            .iter()
            .zip(og.iter_mut())
            .map(|(m, o)| Stream::new(inp, m.body, &mut o[..]))
            .collect();
        decode_group::<B>(&mut ss, rolled).unwrap();
        // decode_group reorders streams as they finish: match them to members by start
        for s in &ss {
            let m = g.iter().find(|m| m.body == s.start).unwrap();
            assert_eq!(s.o, m.raw);
            assert_eq!(s.byte_pos() + 8, m.end, "member end vs trailer");
        }
    }
}

fn run(a: &mut Arm, inp: &[u8], ms: &[Member], outs: &mut [Vec<u8>]) {
    match a {
        Arm::Ours { k, b, rolled } => match *b {
            1 => ours::<1>(*k, *rolled, inp, ms, outs),
            2 => ours::<2>(*k, *rolled, inp, ms, outs),
            4 => ours::<4>(*k, *rolled, inp, ms, outs),
            8 => ours::<8>(*k, *rolled, inp, ms, outs),
            _ => panic!("batch must be 1, 2, 4 or 8"),
        },
        Arm::Fast(d) => {
            for (m, o) in ms.iter().zip(outs) {
                let (n, end) = d.decode(inp, m.body, &mut o[..], m.raw).unwrap();
                assert_eq!(n, m.raw);
                assert_eq!(end + 8, m.end, "member end vs trailer");
            }
        }
        Arm::Libdeflate(d) => {
            for (m, o) in ms.iter().zip(outs) {
                let n = d.deflate_decompress(&inp[m.body..m.end - 8], &mut o[..m.raw]).unwrap();
                assert_eq!(n, m.raw);
            }
        }
        Arm::LibdeflateThreads(t) => {
            // Members dealt round-robin to t scoped threads (threads spawned per call)
            let t = *t;
            let mut lanes: Vec<Vec<(&Member, &mut Vec<u8>)>> = (0..t).map(|_| Vec::new()).collect();
            for (i, p) in ms.iter().zip(outs.iter_mut()).enumerate() {
                lanes[i % t].push(p);
            }
            std::thread::scope(|sc| {
                for lane in lanes {
                    sc.spawn(move || {
                        let mut d = libdeflater::Decompressor::new();
                        for (m, o) in lane {
                            let n = d.deflate_decompress(&inp[m.body..m.end - 8], &mut o[..m.raw]).unwrap();
                            assert_eq!(n, m.raw);
                        }
                    });
                }
            });
        }
        Arm::ZlibNg(z) => {
            for (m, o) in ms.iter().zip(outs) {
                assert_eq!(z.decode(&inp[m.body..m.end - 8], &mut o[..m.raw]), m.raw);
            }
        }
        Arm::Isal(z) => {
            for (m, o) in ms.iter().zip(outs) {
                assert_eq!(z.decode(&inp[m.body..m.end - 8], &mut o[..m.raw]), m.raw);
            }
        }
        Arm::ZlibRs(z) => {
            for (m, o) in ms.iter().zip(outs) {
                assert_eq!(z.decode(&inp[m.body..m.end - 8], &mut o[..m.raw]), m.raw);
            }
        }
        Arm::Miniz(d) => {
            use miniz_oxide::inflate::core::{decompress, inflate_flags::*};
            for (m, o) in ms.iter().zip(outs) {
                d.init();
                let (st, _, n) = decompress(
                    d,
                    &inp[m.body..m.end - 8],
                    &mut o[..m.raw],
                    0,
                    TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
                );
                assert_eq!(st, miniz_oxide::inflate::TINFLStatus::Done);
                assert_eq!(n, m.raw);
            }
        }
    }
}

/// markers <file.gz> <reference> <samples> <span bytes>: speculative runs from sampled real
/// block starts with an unknown window; each checked against the reference, symbol by symbol.
fn markers(a: &[String]) {
    let (path, refp) = (&a[2], &a[3]);
    let (samples, span): (usize, usize) = (a[4].parse().unwrap(), a[5].parse().unwrap());
    let file = std::fs::read(path).unwrap();
    let reference = std::fs::read(refp).unwrap();
    let body = gz_body(&file, 0);
    let mut inp = file.clone();
    inp.resize(file.len() + IN_PAD, 0);
    let mut sp = spec::Spec::default();
    let full = sp.run(&inp, body as u64 * 8, usize::MAX, u64::MAX).unwrap();
    assert!(full.finished);
    let dec = &full.out[spec::WINDOW..];
    assert_eq!(dec.len(), reference.len());
    assert!(dec.iter().zip(&reference).all(|(&x, &y)| x == y as u16), "sequential decode differs");
    let nb = full.blocks.len();
    println!("{path}: {nb} blocks, sequential decode byte-exact; sampling {samples} starts, span {span}");
    println!("{:>6} {:>9} {:>10} {:>9} {:>14} {:>10} {:>11} {:>12} {:>13}", "block", "out MiB", "decoded", "markers%", "last marker KiB", "win refs", "clean@ KiB", "mean run B", "marker-match%");
    let mut lasts = Vec::new();
    for k in 1..=samples {
        let bi = k * (nb - 1) / (samples + 1);
        let (bp, outpos) = full.blocks[bi];
        let r = sp.run(&inp, bp, span, u64::MAX).unwrap();
        // check: literals equal the reference; markers name the right window byte
        for (i, &v) in r.out[spec::WINDOW..].iter().enumerate() {
            let want = reference[outpos + i];
            if v < 256 {
                assert_eq!(v, want as u16, "literal mismatch at block {bi} +{i}");
            } else {
                let w = (v - 256) as usize;
                assert_eq!(reference[outpos - spec::WINDOW + w], want, "marker resolves wrong at block {bi} +{i}");
            }
        }
        let c = spec::census(&r);
        let last = c.last_marker.map_or(0, |x| x + 1);
        lasts.push(last);
        println!(
            "{bi:>6} {:>9.2} {:>10} {:>9.3} {:>14.1} {:>10} {:>11.1} {:>12.1} {:>13.2}",
            outpos as f64 / (1 << 20) as f64,
            c.decoded,
            100.0 * c.markers as f64 / c.decoded as f64,
            last as f64 / 1024.0,
            c.window_refs,
            (last + spec::WINDOW) as f64 / 1024.0,
            c.markers as f64 / c.runs.max(1) as f64,
            100.0 * r.marker_matches as f64 / r.matches.max(1) as f64
        );
    }
    lasts.sort();
    println!(
        "last-marker offset KiB: median {:.1}, p90 {:.1}, max {:.1} (span {} KiB)",
        lasts[lasts.len() / 2] as f64 / 1024.0,
        lasts[lasts.len() * 9 / 10] as f64 / 1024.0,
        *lasts.last().unwrap() as f64 / 1024.0,
        span / 1024
    );
}

/// par <file.gz> <reference> <rounds> <threads,threads,..> <chunk KiB>: the parallel
/// single-member decoder at each thread count vs single-thread arms, paired rounds.
fn par_mode(a: &[String]) {
    let (path, refp, rounds) = (&a[2], &a[3], a[4].parse::<usize>().unwrap());
    let threads: Vec<usize> = a[5].split(',').map(|x| x.parse().unwrap()).collect();
    let chunk = a[6].parse::<usize>().unwrap() * 1024;
    let file = std::fs::read(path).unwrap();
    let reference = std::fs::read(refp).unwrap();
    let body = gz_body(&file, 0);
    let body_end = file.len() - 8;
    let mut inp = file.clone();
    inp.resize(file.len() + IN_PAD, 0);
    let mut pool = Vec::new();
    for &t in &threads {
        let st = par::decompress(&inp, body, body_end, t, chunk, &mut pool).unwrap();
        let mut off = 0;
        for (k, c) in pool.iter().enumerate() {
            assert!(c.bytes[..c.len] == reference[off..off + c.len], "par t={t}: chunk {k} differs");
            off += c.len;
        }
        assert_eq!(off, reference.len());
        eprintln!("par t={t}: {} chunks, byte-exact, markers {:.1}% of output", st.chunks, 100.0 * st.marker_frac);
    }
    let mut single = vec![0u8; reference.len() + SLACK];
    let mut fd = fast::Decoder::default();
    let mut ld = libdeflater::Decompressor::new();
    let mut names: Vec<String> = threads.iter().map(|t| format!("par-t{t}")).collect();
    names.push("fast".into());
    names.push("libdeflate".into());
    let mut seed = 0x2545F4914F6CDD1Du64;
    for r in 0..rounds {
        for i in (1..names.len()).rev() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            names.swap(i, (seed % (i as u64 + 1)) as usize);
        }
        for nm in &names {
            let t0 = Instant::now();
            if let Some(t) = nm.strip_prefix("par-t") {
                par::decompress(&inp, body, body_end, t.parse().unwrap(), chunk, &mut pool).unwrap();
            } else if nm == "fast" {
                fd.decode(&inp, body, &mut single, reference.len()).unwrap();
            } else {
                ld.deflate_decompress(&inp[body..body_end], &mut single[..reference.len()]).unwrap();
            }
            println!("{r} {nm} {} {}", t0.elapsed().as_nanos(), reference.len());
        }
    }
}

/// cat <file.gz> <threads> <chunk KiB>: like `zcat`, for single-member gzip: read the file,
/// decode in parallel, check the CRC-32 and size against the trailer (per-chunk CRCs computed
/// in parallel and combined), write the bytes to stdout.
fn cat_mode(a: &[String]) {
    use std::io::Write;
    let path = &a[2];
    let threads: usize = a[3].parse().unwrap();
    let chunk = a[4].parse::<usize>().unwrap() * 1024;
    let mut inp = std::fs::read(path).unwrap();
    let flen = inp.len();
    inp.resize(flen + IN_PAD, 0);
    let body = gz_body(&inp, 0);
    let body_end = flen - 8;
    let want_crc = u32::from_le_bytes(inp[flen - 8..flen - 4].try_into().unwrap());
    let want_len = u32::from_le_bytes(inp[flen - 4..flen].try_into().unwrap());
    let mut pool = Vec::new();
    par::decompress(&inp, body, body_end, threads, chunk, &mut pool).unwrap();
    // CRC per chunk in parallel, then combined in order
    let next = std::sync::atomic::AtomicUsize::new(0);
    let crcs: Vec<std::sync::Mutex<Option<crc32fast::Hasher>>> = pool.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if k >= pool.len() {
                    break;
                }
                let mut h = crc32fast::Hasher::new();
                h.update(&pool[k].bytes[..pool[k].len]);
                *crcs[k].lock().unwrap() = Some(h);
            });
        }
    });
    let mut h = crc32fast::Hasher::new();
    let mut total = 0usize;
    for (c, k) in crcs.into_iter().zip(&pool) {
        h.combine(&c.into_inner().unwrap().unwrap());
        total += k.len;
    }
    assert_eq!(h.finalize(), want_crc, "CRC-32 mismatch");
    assert_eq!(total as u32, want_len, "ISIZE mismatch");
    let mut out = std::io::stdout().lock();
    for c in &pool {
        out.write_all(&c.bytes[..c.len]).unwrap();
    }
}

/// zcat <file.gz> <threads> <chunk KiB> [cap]: the streaming parallel decoder, bounded
/// memory, CRC-32 and ISIZE checked, bytes to stdout.
fn zcat_mode(a: &[String]) {
    let path = &a[2];
    let threads: usize = a[3].parse().unwrap();
    let chunk = a[4].parse::<usize>().unwrap() * 1024;
    let cap: usize = a.get(5).map_or(threads + 2, |x| x.parse().unwrap());
    let mut inp = std::fs::read(path).unwrap();
    let flen = inp.len();
    inp.resize(flen + IN_PAD, 0);
    let body = gz_body(&inp, 0);
    let want_crc = u32::from_le_bytes(inp[flen - 8..flen - 4].try_into().unwrap());
    let want_len = u32::from_le_bytes(inp[flen - 4..flen].try_into().unwrap());
    let out = std::io::stdout().lock();
    let st = par::stream(&inp, body, flen - 8, threads, chunk, cap, std::io::BufWriter::with_capacity(1 << 20, out)).unwrap();
    assert_eq!(st.crc, want_crc, "CRC-32 mismatch");
    assert_eq!(st.bytes as u32, want_len, "ISIZE mismatch");
    if std::env::var_os("UPIGZ_STATS").is_some() {
        eprintln!("{} chunks, finder {:.3}s", st.chunks, st.finder_s);
    }
}

/// spectest <cases>: counterexample hunt for "speculative decode from a block start with a
/// symbolic window, then resolution against the true window, equals the sequential output's
/// suffix". Random streams (levels 0..=10 via miniz_oxide: stored, fixed and dynamic blocks;
/// random, text-like, run-heavy and overlap-heavy data); every block start; both the
/// reference-style decoder (spec.rs) and the production u16 kernel (fast.rs).
fn spectest(a: &[String]) {
    let cases: usize = a[2].parse().unwrap();
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let (mut checked, mut short_hist) = (0usize, 0usize);
    for case in 0..cases {
        let n = 1 + (rnd() % 300_000) as usize;
        let kind = rnd() % 4;
        let alpha = 2 + rnd() % 40;
        let mut data = Vec::with_capacity(n);
        while data.len() < n {
            match kind {
                0 => data.push(rnd() as u8),
                1 => data.push(b'a' + (rnd() % alpha) as u8),
                2 => {
                    let b = rnd() as u8;
                    let r = 1 + rnd() % 300;
                    for _ in 0..r {
                        data.push(b);
                    }
                }
                _ => {
                    // overlap-heavy: short periodic patterns and long-distance repeats
                    let p = 1 + (rnd() % 12) as usize;
                    let start = data.len().saturating_sub((rnd() % 40000) as usize);
                    let r = 3 + (rnd() % 500) as usize;
                    if data.len() > p && rnd() % 2 == 0 {
                        for _ in 0..r {
                            let v = data[data.len() - p];
                            data.push(v);
                        }
                    } else if start < data.len() {
                        for i in 0..r {
                            let v = data[start + i % (data.len() - start)];
                            data.push(v);
                        }
                    } else {
                        data.push(rnd() as u8);
                    }
                }
            }
        }
        data.truncate(n);
        let level = (rnd() % 11) as u8;
        let comp = miniz_oxide::deflate::compress_to_vec(&data, level);
        let mut inp = comp.clone();
        inp.resize(comp.len() + IN_PAD, 0);
        // sequential: block starts and output offsets
        let mut sp = spec::Spec::default();
        let full = sp.run(&inp, 0, usize::MAX, u64::MAX).unwrap();
        assert!(full.out[spec::WINDOW..].iter().zip(&data).all(|(&x, &y)| x == y as u16));
        let mut fd = fast::Decoder::default();
        for &(bp, outpos) in &full.blocks {
            // true window: the 32 KiB before outpos (positions before the stream start are absent)
            let win_true = |w: usize| -> Option<u8> {
                let back = spec::WINDOW - w; // bytes back from outpos
                if back <= outpos { Some(data[outpos - back]) } else { None }
            };
            let check = |sym: &[u16], who: &str| {
                assert_eq!(sym.len(), data.len() - outpos, "case {case} {who}: length at block {bp}");
                for (i, &v) in sym.iter().enumerate() {
                    let got = if v < 256 { v as u8 } else {
                        win_true((v - 256) as usize).unwrap_or_else(|| panic!("case {case} {who}: marker into absent history at block {bp}"))
                    };
                    assert_eq!(got, data[outpos + i], "case {case} {who}: block {bp} +{i}");
                }
            };
            let r = sp.run(&inp, bp, usize::MAX, u64::MAX).unwrap();
            assert!(r.finished);
            check(&r.out[spec::WINDOW..], "spec");
            let mut buf: Vec<u16> = (0..spec::WINDOW as u16).map(|i| 256 + i).collect();
            let o = fd.run::<u16>(&inp, bp, &mut buf, spec::WINDOW, u64::MAX).unwrap();
            assert!(o.finished);
            check(&buf[spec::WINDOW..o.o], "fast-u16");
            checked += 1;
            short_hist += (outpos < spec::WINDOW) as usize;
        }
    }
    println!("spectest: {cases} streams, {checked} block starts checked (both decoders), {short_hist} with history shorter than 32 KiB; no counterexample");
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a[1] == "spectest" {
        return spectest(&a);
    }
    if a[1] == "zcat" {
        return zcat_mode(&a);
    }
    if a[1] == "cat" {
        return cat_mode(&a);
    }
    if a[1] == "par" {
        return par_mode(&a);
    }
    if a[1] == "markers" {
        return markers(&a);
    }
    let (path, refp, rounds) = (&a[1], &a[2], a[3].parse::<usize>().unwrap());
    let names: Vec<&str> = a[4].split(',').collect();
    let file = std::fs::read(path).unwrap();
    let reference = std::fs::read(refp).unwrap();
    let ms = members(path, &file);
    let mut inp = file.clone();
    inp.resize(file.len() + IN_PAD, 0);
    let total: usize = ms.iter().map(|m| m.raw).sum();
    assert_eq!(total, reference.len(), "members' raw sizes vs reference");
    let mut outs: Vec<Vec<u8>> = ms.iter().map(|m| vec![0u8; m.raw + SLACK]).collect();
    let mut arms: Vec<(String, Arm)> = names.iter().map(|n| (n.to_string(), arm(n))).collect();

    // Validate every arm byte-exact once (untimed), poisoning outputs between arms.
    for (n, a) in arms.iter_mut() {
        outs.iter_mut().for_each(|o| o.fill(0xA5));
        run(a, &inp, &ms, &mut outs);
        for (m, o) in ms.iter().zip(&outs) {
            assert!(o[..m.raw] == reference[m.out_off..m.out_off + m.raw], "{n}: output differs");
        }
    }
    eprintln!("{path}: {} members, {total} bytes, all {} arms byte-exact", ms.len(), arms.len());
    #[cfg(feature = "census")]
    {
        use inflate::census::*;
        let (l, m, mb) = (get(&LIT), get(&MATCH), get(&MATCH_BYTES));
        let sy = l + m;
        let h: Vec<String> = LEN_HIST.iter().map(|c| format!("{:.3}", get(c) as f64 / m as f64)).collect();
        println!(
            "census {path}: symbols/byte {:.4} literal-share {:.3} mean-match-len {:.2} match-bytes-share {:.3} dist<8 {:.3} dist=1 {:.3} len-hist[3-4,5-8,9-16,17-32,33-64,65+] {}",
            sy as f64 / (l + mb) as f64, l as f64 / sy as f64, mb as f64 / m as f64, mb as f64 / (l + mb) as f64,
            get(&DIST_LT8) as f64 / m as f64, get(&DIST1) as f64 / m as f64, h.join(",")
        );
        if names.contains(&"fast") {
            use fast::prof;
            let t = std::time::Instant::now();
            run(&mut arms.iter_mut().find(|a| a.0 == "fast").unwrap().1, &inp, &ms, &mut outs);
            let all = t.elapsed().as_nanos() as f64;
            // the validation pass above also ran: halve the counters' two passes
            println!(
                "prof {path}: dynamic blocks {} (per pass), header+build {:.2}% of decode, build alone {:.2}%, {:.0} raw bytes per block",
                prof::get(&prof::BLOCKS) / 2,
                prof::get(&prof::HEADER_NS) as f64 / 2.0 / all * 100.0 * 2.0 / 2.0,
                prof::get(&prof::BUILD_NS) as f64 / 2.0 / all * 100.0,
                total as f64 / (prof::get(&prof::BLOCKS) / 2) as f64
            );
        }
        return;
    }

    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut order: Vec<usize> = (0..arms.len()).collect();
    for r in 0..rounds {
        for i in (1..order.len()).rev() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            order.swap(i, (seed % (i as u64 + 1)) as usize);
        }
        for &i in &order {
            let t = Instant::now();
            run(&mut arms[i].1, &inp, &ms, &mut outs);
            let ns = t.elapsed().as_nanos();
            println!("{r} {} {ns} {total}", arms[i].0);
        }
    }
}
