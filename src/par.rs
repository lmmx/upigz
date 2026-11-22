//! Parallel decoding of one single-member DEFLATE stream (rapidgzip's two-pass shape).
//!
//! 1. Chunk starts: the compressed body is cut into fixed-size chunks; for each chunk after
//!    the first, a block finder scans forward bit by bit for a dynamic-block header.
//! 2. Pass 1, in parallel: chunk 0 decodes to bytes; chunk k >= 1 decodes to 16-bit symbols
//!    from window markers, until a block starts at or past the next chunk's start, and must
//!    land on it exactly (that checks the finder; a miss merges the chunks).
//! 3. Windows, in order: each chunk's last 32 KiB is resolved through the previous window.
//! 4. Pass 2, in parallel: each chunk's symbols are resolved to bytes.

use crate::fast::{Decoder, Err};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub const WINDOW: usize = 32768;

/// Cheap prefilter at a bit position: BFINAL=0, BTYPE=2, HLIT <= 29, HDIST <= 29, and the
/// precode lengths form a complete code. Reads 64 bits from `inp` at `bitpos` (needs >= 16
/// bytes of padding).
#[inline(always)]
fn precheck(inp: &[u8], bitpos: u64) -> bool {
    let p = (bitpos / 8) as usize;
    let w = u64::from_le_bytes(inp[p..p + 8].try_into().unwrap()) >> (bitpos % 8);
    // 3 header bits: BFINAL=0 then BTYPE=2 (LSB first: bit1=0, bit2=1)
    if w & 0b111 != 0b100 {
        return false;
    }
    let hlit = (w >> 3) & 31;
    let hdist = (w >> 8) & 31;
    if hlit > 29 || hdist > 29 {
        return false;
    }
    let hclen = ((w >> 13) & 15) as usize + 4;
    // precode lengths: 3 bits each from bit 17; up to 19 * 3 = 57 bits -> need a second word
    let p2 = ((bitpos + 17) / 8) as usize;
    let w2 = u64::from_le_bytes(inp[p2..p2 + 8].try_into().unwrap()) >> ((bitpos + 17) % 8);
    let mut count = [0u32; 8];
    for i in 0..hclen {
        count[((w2 >> (3 * i)) & 7) as usize] += 1;
    }
    count[0] = 0;
    let mut left: i32 = 1;
    for c in &count[1..] {
        left = (left << 1) - *c as i32;
        if left < 0 {
            return false;
        }
    }
    left == 0
}

/// First bit position in [from, to) where a dynamic block header passes the prefilter and
/// a full header parse (complete codes, an end-of-block code).
pub fn find_block(dec: &mut Decoder, inp: &[u8], from: u64, to: u64) -> Option<u64> {
    let mut b = from;
    while b < to {
        if precheck(inp, b) && dec.header_ok(inp, b) {
            return Some(b);
        }
        b += 1;
    }
    None
}

pub struct Chunk {
    pub start: u64, // bit position of its first block
    pub end: u64,   // bit position where it stopped
    pub sym: Vec<u16>, // [0, WINDOW) markers then the symbols (chunks >= 1)
    pub bytes: Vec<u8>, // chunk 0's bytes, or the resolved bytes after pass 2
    pub len: usize,
    pub finished: bool,
}

pub struct Stats {
    pub chunks: usize,
    pub finder_misses: usize,
    pub marker_frac: f64,
}

/// Decode a single-member stream whose DEFLATE body starts at byte `body` of the padded
/// `inp` and ends before byte `body_end`. Returns chunk byte buffers in order.
pub fn decompress(inp: &[u8], body: usize, body_end: usize, threads: usize, chunk_bytes: usize, pool: &mut Vec<Chunk>) -> Result<Stats, Err> {
    let n = ((body_end - body) + chunk_bytes - 1) / chunk_bytes;
    while pool.len() < n {
        pool.push(Chunk { start: 0, end: 0, sym: Vec::new(), bytes: Vec::new(), len: 0, finished: false });
    }
    pool.truncate(n);
    let starts: Vec<Mutex<Option<u64>>> = (0..n).map(|_| Mutex::new(None)).collect();
    *starts[0].lock().unwrap() = Some(body as u64 * 8);

    // 1. finder, in parallel
    let next = AtomicUsize::new(1);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let mut d = Decoder::default();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= n {
                        break;
                    }
                    let from = (body + k * chunk_bytes) as u64 * 8;
                    let to = ((body + (k + 1) * chunk_bytes).min(body_end)) as u64 * 8;
                    *starts[k].lock().unwrap() = find_block(&mut d, inp, from, to);
                }
            });
        }
    });
    let mut s: Vec<u64> = starts.into_iter().filter_map(|m| m.into_inner().unwrap()).collect();
    s.push(u64::MAX); // the last chunk runs to the end
    let n = s.len() - 1;
    pool.truncate(n);

    // 2. pass 1, in parallel
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<&mut Chunk>> = pool.iter_mut().map(Mutex::new).collect();
    let res: Mutex<Result<(), Err>> = Mutex::new(Ok(()));
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let mut d = Decoder::default();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= n {
                        break;
                    }
                    let mut c = slots[k].lock().unwrap();
                    c.start = s[k];
                    let r = if k == 0 {
                        d.run::<u8>(inp, s[0], &mut c.bytes, 0, s[1])
                    } else {
                        if c.sym.len() < WINDOW {
                            c.sym.resize(WINDOW, 0);
                        }
                        for (i, v) in c.sym[..WINDOW].iter_mut().enumerate() {
                            *v = 256 + i as u16;
                        }
                        d.run::<u16>(inp, s[k], &mut c.sym, WINDOW, s[k + 1])
                    };
                    match r {
                        Ok(o) => {
                            c.end = o.end_bitpos;
                            c.len = if k == 0 { o.o } else { o.o - WINDOW };
                            c.finished = o.finished;
                        }
                        Err(e) => *res.lock().unwrap() = Err(e),
                    }
                }
            });
        }
    });
    res.into_inner().unwrap()?;
    drop(slots);
    // chaining: each chunk must stop exactly at the next chunk's start
    let mut misses = 0;
    for k in 0..n - 1 {
        if pool[k].end != s[k + 1] {
            misses += 1;
        }
    }
    if misses > 0 || !pool[n - 1].finished {
        return Err("chunk chaining failed (finder false positive): not handled yet");
    }

    // 3. windows, in order: resolve each chunk's last WINDOW symbols
    let mut win: Vec<u8> = {
        let c = &pool[0];
        if c.len < WINDOW {
            return Err("first chunk shorter than a window: not handled yet");
        }
        c.bytes[c.len - WINDOW..c.len].to_vec()
    };
    let mut windows: Vec<Vec<u8>> = vec![Vec::new(); n]; // windows[k] = the window before chunk k
    let mut markers = 0usize;
    let mut total = pool[0].len;
    for k in 1..n {
        let c = &pool[k];
        if c.len < WINDOW {
            return Err("chunk shorter than a window: not handled yet");
        }
        let tail = &c.sym[WINDOW + c.len - WINDOW..WINDOW + c.len];
        let nw: Vec<u8> = tail.iter().map(|&v| if v < 256 { v as u8 } else { win[(v - 256) as usize] }).collect();
        windows[k] = std::mem::replace(&mut win, nw);
        total += c.len;
    }

    // 4. pass 2, in parallel: resolve symbols to bytes
    let next = AtomicUsize::new(1);
    let slots: Vec<Mutex<&mut Chunk>> = pool.iter_mut().map(Mutex::new).collect();
    let mk = AtomicUsize::new(0);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                if k >= n {
                    break;
                }
                let mut c = slots[k].lock().unwrap();
                let c = &mut **c;
                let w = &windows[k];
                if c.bytes.len() < c.len {
                    c.bytes.resize(c.len, 0);
                }
                let mut m = 0usize;
                for (d, &v) in c.bytes[..c.len].iter_mut().zip(&c.sym[WINDOW..WINDOW + c.len]) {
                    // branch-free: markers index the window, bytes pass through
                    let is_m = v >= 256;
                    m += is_m as usize;
                    *d = if is_m { w[(v - 256) as usize] } else { v as u8 };
                }
                mk.fetch_add(m, Ordering::Relaxed);
            });
        }
    });
    markers += mk.into_inner();
    Ok(Stats { chunks: n, finder_misses: misses, marker_frac: markers as f64 / total as f64 })
}

// ---------------------------------------------------------------------------------------
// Streaming form: bounded memory. At most `cap` chunks are in flight; buffers are recycled.
// Workers: pass 1 on chunk k, publish; wait for chunk k's window; resolve to bytes + CRC;
// publish. The caller's thread: in order, hand each chunk's window to the next (resolving
// the chunk's last 32 KiB), then write the chunk and recycle its buffers.

use std::sync::Condvar;

#[derive(Default)]
struct Slot {
    pass1: Option<(usize, u64, bool)>, // (len, end bitpos, finished)
    sym: Option<Vec<u16>>,
    bytes: Option<Vec<u8>>,
    window: Option<std::sync::Arc<Vec<u8>>>,
    done: Option<crc32fast::Hasher>,
}

#[derive(Default)]
struct State {
    slots: Vec<Slot>,
    consumed: usize,
    free_sym: Vec<Vec<u16>>,
    free_bytes: Vec<Vec<u8>>,
    err: Option<Err>,
}

pub struct StreamStats {
    pub chunks: usize,
    pub bytes: usize,
    pub crc: u32,
    pub finder_s: f64,
}

pub fn stream<W: std::io::Write>(inp: &[u8], body: usize, body_end: usize, threads: usize, chunk_bytes: usize, cap: usize, mut w: W) -> Result<StreamStats, Err> {
    let t0 = std::time::Instant::now();
    // chunk starts (parallel finder)
    let n0 = ((body_end - body) + chunk_bytes - 1) / chunk_bytes;
    let starts: Vec<Mutex<Option<u64>>> = (0..n0).map(|_| Mutex::new(None)).collect();
    *starts[0].lock().unwrap() = Some(body as u64 * 8);
    let next = AtomicUsize::new(1);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let mut d = Decoder::default();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= n0 {
                        break;
                    }
                    let from = (body + k * chunk_bytes) as u64 * 8;
                    let to = ((body + (k + 1) * chunk_bytes).min(body_end)) as u64 * 8;
                    *starts[k].lock().unwrap() = find_block(&mut d, inp, from, to);
                }
            });
        }
    });
    let mut s: Vec<u64> = starts.into_iter().filter_map(|m| m.into_inner().unwrap()).collect();
    s.push(u64::MAX);
    let n = s.len() - 1;
    let finder_s = t0.elapsed().as_secs_f64();

    let st = Mutex::new(State { slots: (0..n).map(|_| Slot::default()).collect(), ..Default::default() });
    let cv = Condvar::new();
    let s = &s;
    let mut total = 0usize;
    let mut crc = crc32fast::Hasher::new();
    // shared scheduling: next pass-1 chunk, and chunks whose window is known (resolvable)
    let resolve: Mutex<std::collections::VecDeque<usize>> = Mutex::new(Default::default());
    let next_p1 = AtomicUsize::new(0);
    let result: Result<(), Err> = std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let mut d = Decoder::default();
                loop {
                    // pick work: a resolvable chunk first, else a new pass-1 chunk under the cap
                    enum Job {
                        Resolve(usize),
                        Pass1(usize),
                    }
                    let job = {
                        let mut g = st.lock().unwrap();
                        loop {
                            if g.err.is_some() {
                                return;
                            }
                            if let Some(k) = resolve.lock().unwrap().pop_front() {
                                break Job::Resolve(k);
                            }
                            let k = next_p1.load(Ordering::Relaxed);
                            if k < n && k < g.consumed + cap {
                                next_p1.store(k + 1, Ordering::Relaxed);
                                break Job::Pass1(k);
                            }
                            if k >= n && g.consumed >= n {
                                return;
                            }
                            g = cv.wait(g).unwrap();
                        }
                    };
                    match job {
                        Job::Pass1(k) => {
                            let (mut sym, mut bytes) = {
                                let mut g = st.lock().unwrap();
                                (g.free_sym.pop().unwrap_or_default(), g.free_bytes.pop().unwrap_or_default())
                            };
                            let r = if k == 0 {
                                d.run::<u8>(inp, s[0], &mut bytes, 0, s[1]).map(|o| (o.o, o.end_bitpos, o.finished))
                            } else {
                                if sym.len() < WINDOW {
                                    sym.resize(WINDOW, 0);
                                }
                                for (i, v) in sym[..WINDOW].iter_mut().enumerate() {
                                    *v = 256 + i as u16;
                                }
                                d.run::<u16>(inp, s[k], &mut sym, WINDOW, s[k + 1]).map(|o| (o.o - WINDOW, o.end_bitpos, o.finished))
                            };
                            let mut g = st.lock().unwrap();
                            match r {
                                Ok(x) => {
                                    g.slots[k].pass1 = Some(x);
                                    if k == 0 {
                                        let mut h = crc32fast::Hasher::new();
                                        h.update(&bytes[..x.0]);
                                        g.slots[k].done = Some(h);
                                        g.slots[k].bytes = Some(bytes);
                                        g.free_sym.push(sym);
                                    } else {
                                        g.slots[k].sym = Some(sym);
                                        g.free_bytes.push(bytes);
                                    }
                                }
                                Err(e) => g.err = Some(e),
                            }
                            cv.notify_all();
                        }
                        Job::Resolve(k) => {
                            let (win, sym, mut bytes, len) = {
                                let mut g = st.lock().unwrap();
                                let len = g.slots[k].pass1.unwrap().0;
                                let b = g.free_bytes.pop().unwrap_or_default();
                                (g.slots[k].window.clone().unwrap(), g.slots[k].sym.take().unwrap(), b, len)
                            };
                            if bytes.len() < len {
                                bytes.resize(len, 0);
                            }
                            for (dst, &v) in bytes[..len].iter_mut().zip(&sym[WINDOW..WINDOW + len]) {
                                *dst = if v < 256 { v as u8 } else { win[(v - 256) as usize] };
                            }
                            let mut h = crc32fast::Hasher::new();
                            h.update(&bytes[..len]);
                            let mut g = st.lock().unwrap();
                            g.slots[k].done = Some(h);
                            g.slots[k].bytes = Some(bytes);
                            g.free_sym.push(sym);
                            cv.notify_all();
                        }
                    }
                }
            });
        }
        // Two cursors: windows run ahead as far as pass 1 allows (each chunk's next window is
        // taken from its symbols before it is queued for resolution); output follows in order.
        let fail = |g: &mut std::sync::MutexGuard<State>, e: Err| {
            g.err = Some(e);
            cv.notify_all();
            Err(e)
        };
        let (mut wk, mut ok) = (0usize, 0usize); // next window to hand on, next chunk to write
        let mut g = st.lock().unwrap();
        while ok < n {
            if let Some(e) = g.err {
                return Err(e);
            }
            let mut progressed = false;
            // hand on windows
            while wk < n && g.slots[wk].pass1.is_some() && (wk == 0 || g.slots[wk].window.is_some()) {
                let k = wk;
                let (len, end, finished) = g.slots[k].pass1.unwrap();
                if (k + 1 < n && end != s[k + 1]) || (k + 1 == n && !finished) {
                    return fail(&mut g, "chunk chaining failed (finder false positive): not handled yet");
                }
                if len < WINDOW {
                    return fail(&mut g, "chunk shorter than a window: not handled yet");
                }
                if k + 1 < n {
                    let nw: Vec<u8> = if k == 0 {
                        g.slots[0].bytes.as_ref().unwrap()[len - WINDOW..len].to_vec()
                    } else {
                        let win = g.slots[k].window.clone().unwrap();
                        let sym = g.slots[k].sym.as_ref().unwrap();
                        sym[len..WINDOW + len].iter().map(|&v| if v < 256 { v as u8 } else { win[(v - 256) as usize] }).collect()
                    };
                    g.slots[k + 1].window = Some(std::sync::Arc::new(nw));
                }
                if k > 0 {
                    resolve.lock().unwrap().push_back(k);
                }
                wk += 1;
                progressed = true;
            }
            if progressed {
                cv.notify_all();
            }
            // write what is done, in order
            if g.slots[ok].done.is_some() {
                let len = g.slots[ok].pass1.unwrap().0;
                let h = g.slots[ok].done.take().unwrap();
                let bytes = g.slots[ok].bytes.take().unwrap();
                drop(g);
                w.write_all(&bytes[..len]).map_err(|_| "write failed")?;
                crc.combine(&h);
                total += len;
                g = st.lock().unwrap();
                g.free_bytes.push(bytes);
                ok += 1;
                g.consumed = ok;
                cv.notify_all();
                continue;
            }
            if !progressed {
                g = cv.wait(g).unwrap();
            }
        }
        drop(g);
        Ok(())
    });
    result?;
    Ok(StreamStats { chunks: n, bytes: total, crc: crc.finalize(), finder_s })
}
