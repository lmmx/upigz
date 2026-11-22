//! Speculative decoding from a block start with an unknown window (rapidgzip's idea),
//! as a measuring instrument: a simple checked decoder over 16-bit symbols.
//!
//! Output symbols: 0..=255 are bytes; 256 + i is a marker for byte i of the unknown 32 KiB
//! window before the start (i = 0 is 32768 bytes back). The output vector is seeded with the
//! 32768 window markers, so back-references into the window copy markers, and markers
//! propagate through later matches exactly as the unknown bytes would.

use crate::fast::{
    build, dist_entry, lit_entry, Err, DB, DIST_ENOUGH, EOB, EXC, INVALID, LB, LIT, LIT_ENOUGH, SUB,
};

pub const WINDOW: usize = 32768;

struct Bits<'a> {
    inp: &'a [u8],
    pos: usize,
    bb: u64,
    bc: u32,
}

impl Bits<'_> {
    fn at(inp: &[u8], bitpos: u64) -> Result<Bits<'_>, Err> {
        let mut b = Bits {
            inp,
            pos: (bitpos / 8) as usize,
            bb: 0,
            bc: 0,
        };
        let skip = (bitpos % 8) as u32;
        b.take(skip)?;
        Ok(b)
    }
    fn refill(&mut self) -> Result<(), Err> {
        let w = self
            .inp
            .get(self.pos..self.pos + 8)
            .ok_or("input overrun")?;
        self.bb |= u64::from_le_bytes(w.try_into().unwrap()) << self.bc;
        self.pos += (7 - ((self.bc >> 3) & 7)) as usize;
        self.bc |= 56;
        Ok(())
    }
    fn take(&mut self, n: u32) -> Result<u32, Err> {
        if n == 0 {
            return Ok(0);
        }
        if self.bc < n {
            self.refill()?;
        }
        let v = (self.bb & ((1u64 << n) - 1)) as u32;
        self.bb >>= n;
        self.bc -= n;
        Ok(v)
    }
    fn bitpos(&self) -> u64 {
        self.pos as u64 * 8 - self.bc as u64
    }
}

pub struct Spec {
    lit: Box<[u32; LIT_ENOUGH]>,
    dist: Box<[u32; DIST_ENOUGH]>,
    m: core::cell::Cell<usize>,
    mm: core::cell::Cell<usize>,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            lit: Box::new([INVALID; LIT_ENOUGH]),
            dist: Box::new([INVALID; DIST_ENOUGH]),
            m: Default::default(),
            mm: Default::default(),
        }
    }
}

pub struct Run {
    pub out: Vec<u16>, // window markers [0, WINDOW), then the decoded symbols
    pub blocks: Vec<(u64, usize)>, // (bit position, output offset past the window) per block
    pub end_bitpos: u64,
    pub finished: bool, // reached the final block's end
    pub matches: usize,
    pub marker_matches: usize, // matches whose source held at least one marker
}

impl Spec {
    /// Decode from `bitpos` (a block start) until the final block ends, `max_out` symbols are
    /// out, or a block starts at or past `stop_bitpos`.
    pub fn run(
        &mut self,
        inp: &[u8],
        bitpos: u64,
        max_out: usize,
        stop_bitpos: u64,
    ) -> Result<Run, Err> {
        self.m.set(0);
        self.mm.set(0);
        let mut b = Bits::at(inp, bitpos)?;
        let mut out: Vec<u16> = (0..WINDOW as u16).map(|i| 256 + i).collect();
        out.reserve(max_out.min(1 << 28));
        let mut blocks = Vec::new();
        loop {
            let bp = b.bitpos();
            if bp >= stop_bitpos || out.len() - WINDOW >= max_out {
                return Ok(Run {
                    out,
                    blocks,
                    end_bitpos: bp,
                    finished: false,
                    matches: self.m.get(),
                    marker_matches: self.mm.get(),
                });
            }
            blocks.push((bp, out.len() - WINDOW));
            let last = b.take(1)? == 1;
            match b.take(2)? {
                0 => {
                    let d = b.bc & 7;
                    b.take(d)?;
                    let p = b.pos - (b.bc as usize >> 3);
                    let h = inp.get(p..p + 4).ok_or("input overrun")?;
                    let len = u16::from_le_bytes([h[0], h[1]]) as usize;
                    if len != !u16::from_le_bytes([h[2], h[3]]) as usize {
                        return Err("stored length mismatch");
                    }
                    let src = inp.get(p + 4..p + 4 + len).ok_or("input overrun")?;
                    out.extend(src.iter().map(|&x| x as u16));
                    b = Bits {
                        inp,
                        pos: p + 4 + len,
                        bb: 0,
                        bc: 0,
                    };
                }
                1 => {
                    let mut l = [0u8; 320];
                    l[..144].fill(8);
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    l[280..288].fill(8);
                    l[288..320].fill(5);
                    build(&mut self.lit[..], &l[..288], LB, lit_entry)?;
                    build(&mut self.dist[..], &l[288..320], DB, dist_entry)?;
                    self.body(&mut b, &mut out)?;
                }
                2 => {
                    self.dynamic(&mut b)?;
                    self.body(&mut b, &mut out)?;
                }
                _ => return Err("invalid block type"),
            }
            if last {
                let d = b.bc & 7;
                b.take(d)?;
                let bp = b.bitpos();
                return Ok(Run {
                    out,
                    blocks,
                    end_bitpos: bp,
                    finished: true,
                    matches: self.m.get(),
                    marker_matches: self.mm.get(),
                });
            }
        }
    }

    fn dynamic(&mut self, b: &mut Bits<'_>) -> Result<(), Err> {
        let hlit = b.take(5)? as usize + 257;
        let hdist = b.take(5)? as usize + 1;
        let hclen = b.take(4)? as usize + 4;
        if hlit > 286 || hdist > 30 {
            return Err("too many codes");
        }
        const ORD: [usize; 19] = [
            16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
        ];
        let mut pl = [0u8; 19];
        for &i in &ORD[..hclen] {
            pl[i] = b.take(3)? as u8;
        }
        let mut pt = [INVALID; 128];
        build(&mut pt, &pl, 7, |s, l| ((s as u32) << 16) | l)?;
        let total = hlit + hdist;
        let mut l = [0u8; 320];
        let mut i = 0;
        while i < total {
            if b.bc < 16 {
                b.refill()?;
            }
            let e = pt[(b.bb & 127) as usize];
            if e & EXC != 0 {
                return Err("invalid precode");
            }
            b.bb >>= e & 0xff;
            b.bc -= e & 0xff;
            let s = (e >> 16) as usize;
            let (v, n) = match s {
                0..=15 => (s as u8, 1),
                16 => {
                    if i == 0 {
                        return Err("repeat with no previous length");
                    }
                    (l[i - 1], 3 + b.take(2)? as usize)
                }
                17 => (0, 3 + b.take(3)? as usize),
                _ => (0, 11 + b.take(7)? as usize),
            };
            if i + n > total {
                return Err("code lengths overflow");
            }
            l[i..i + n].fill(v);
            i += n;
        }
        if l[256] == 0 {
            return Err("no end-of-block code");
        }
        build(&mut self.lit[..], &l[..hlit], LB, lit_entry)?;
        build(&mut self.dist[..], &l[hlit..total], DB, dist_entry)
    }

    fn body(&self, b: &mut Bits<'_>, out: &mut Vec<u16>) -> Result<(), Err> {
        loop {
            if b.bc < 48 {
                b.refill()?;
            }
            let mut e = self.lit[(b.bb & ((1 << LB) - 1)) as usize];
            if e & SUB != 0 {
                b.bb >>= e & 0x3f;
                b.bc -= e & 0xff;
                e = self.lit[((e >> 16) & 0x7fff) as usize
                    + (b.bb & ((1 << ((e >> 8) & 0xf)) - 1)) as usize];
            }
            let saved = b.bb;
            b.bb >>= e & 0x3f;
            b.bc -= e & 0xff;
            if e & LIT != 0 {
                out.push(((e >> 16) & 0xff) as u16);
                continue;
            }
            if e & EXC != 0 {
                if e & EOB != 0 {
                    return Ok(());
                }
                return Err("invalid literal/length code");
            }
            let len = (e >> 16) as usize
                + ((saved & !(u64::MAX << (e & 0x3f))) >> ((e >> 8) & 0xf)) as usize;
            let mut d = self.dist[(b.bb & ((1 << DB) - 1)) as usize];
            if d & SUB != 0 {
                b.bb >>= d & 0x3f;
                b.bc -= d & 0xff;
                d = self.dist[((d >> 16) & 0x7fff) as usize
                    + (b.bb & ((1 << ((d >> 8) & 0xf)) - 1)) as usize];
            }
            if d & EXC != 0 {
                return Err("invalid distance code");
            }
            let saved = b.bb;
            b.bb >>= d & 0x3f;
            b.bc -= d & 0xff;
            let dst = (d >> 16) as usize
                + ((saved & !(u64::MAX << (d & 0x3f))) >> ((d >> 8) & 0xf)) as usize;
            if dst > out.len() {
                return Err("distance too far back");
            }
            let s = out.len() - dst;
            let mut any = false;
            for k in 0..len {
                let v = out[s + k];
                any |= v >= 256;
                out.push(v);
            }
            self.m.set(self.m.get() + 1);
            self.mm.set(self.mm.get() + any as usize);
        }
    }
}

/// The marker census of one speculative run: where markers stop, and how many there are.
pub struct Census {
    pub decoded: usize,
    pub markers: usize,
    pub last_marker: Option<usize>, // output offset (past the window) of the last marker
    pub window_refs: usize,         // distinct window bytes ever referenced
    pub runs: usize, // maximal runs of consecutive markers naming consecutive window bytes
}

pub fn census(run: &Run) -> Census {
    let dec = &run.out[WINDOW..];
    let mut markers = 0;
    let mut last = None;
    let mut seen = vec![false; WINDOW];
    let mut runs = 0;
    let mut prev: Option<u16> = None;
    for (i, &v) in dec.iter().enumerate() {
        if v >= 256 && prev.map_or(true, |p| p < 256 || p + 1 != v) {
            runs += 1;
        }
        prev = Some(v);
        if v >= 256 {
            markers += 1;
            last = Some(i);
            seen[(v - 256) as usize] = true;
        }
    }
    Census {
        decoded: dec.len(),
        markers,
        last_marker: last,
        window_refs: seen.iter().filter(|&&x| x).count(),
        runs,
    }
}
