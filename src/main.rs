//! Paired, interleaved A/B over DEFLATE decoders, one thread.
//!
//! usage: inflate-ilp <file.gz> <reference-raw-file> <rounds> <arm,arm,...>
//!
//! Members come from `<file.gz>.idx` ("comp_off comp_len raw_len" per line) when present,
//! otherwise the file is one member. Every arm decodes every member's raw DEFLATE body into
//! a preallocated buffer; no arm checks CRCs. Each round runs every arm once, in a shuffled
//! order, and prints one line per arm: `round arm ns bytes`.

mod inflate;

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

// ---- arms ------------------------------------------------------------------------------

enum Arm {
    Ours { k: usize, b: usize },
    Libdeflate(libdeflater::Decompressor),
    ZlibNg(Zng),
    ZlibRs(Zrs),
    Miniz(Box<miniz_oxide::inflate::core::DecompressorOxide>),
}

fn arm(name: &str) -> Arm {
    // ours-k<K>[-b<B>], e.g. ours-k1 (sequential), ours-k4, ours-k4-b4; "-aa" suffix = duplicate
    let base = name.trim_end_matches("-aa");
    if let Some(r) = base.strip_prefix("ours-k") {
        let mut it = r.split("-b");
        let k = it.next().unwrap().parse().unwrap();
        let b = it.next().map_or(1, |x| x.parse().unwrap());
        return Arm::Ours { k, b };
    }
    match base {
        "libdeflate" => Arm::Libdeflate(libdeflater::Decompressor::new()),
        "zlib-ng" => Arm::ZlibNg(Zng::new()),
        "zlib-rs" => Arm::ZlibRs(Zrs::new()),
        "miniz" => Arm::Miniz(Box::default()),
        _ => panic!("unknown arm {name}"),
    }
}

fn ours<const B: usize>(k: usize, inp: &[u8], ms: &[Member], outs: &mut [Vec<u8>]) {
    for (g, og) in ms.chunks(k).zip(outs.chunks_mut(k)) {
        let mut ss: Vec<Stream<'_>> = g
            .iter()
            .zip(og.iter_mut())
            .map(|(m, o)| Stream::new(inp, m.body, &mut o[..]))
            .collect();
        decode_group::<B>(&mut ss).unwrap();
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
        Arm::Ours { k, b } => match *b {
            1 => ours::<1>(*k, inp, ms, outs),
            2 => ours::<2>(*k, inp, ms, outs),
            4 => ours::<4>(*k, inp, ms, outs),
            8 => ours::<8>(*k, inp, ms, outs),
            _ => panic!("batch must be 1, 2, 4 or 8"),
        },
        Arm::Libdeflate(d) => {
            for (m, o) in ms.iter().zip(outs) {
                let n = d.deflate_decompress(&inp[m.body..m.end - 8], &mut o[..m.raw]).unwrap();
                assert_eq!(n, m.raw);
            }
        }
        Arm::ZlibNg(z) => {
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

fn main() {
    let a: Vec<String> = std::env::args().collect();
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
