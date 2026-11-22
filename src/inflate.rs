//! A minimal DEFLATE decoder whose hot loop advances N independent streams round-robin.
//!
//! The point is the A/B, not the decoder: `hot::<1>` run on each member in turn and
//! `hot::<N>` run on N members at once are the same code, so their ratio isolates what
//! interleaving independent streams buys inside one thread (instruction-level parallelism
//! across the streams' serial bit-position chains).
//!
//! Safe Rust only. Inputs are one shared buffer padded with zero bytes at its end, so the
//! branch-free 8-byte refill never needs a slow path; outputs are separate buffers with
//! `SLACK` spare bytes, so match copies may write whole 8-byte words past their end.

pub const SLACK: usize = 16;
pub const IN_PAD: usize = 16;

const LB: u32 = 11; // litlen main table bits
const DB: u32 = 8; // distance main table bits
const LIT_ENOUGH: usize = 2342; // libdeflate's bound for 11 main bits, 288 symbols, 15-bit codes
const DIST_ENOUGH: usize = 402; // and for 8 main bits, 32 symbols

// Table entry: bits 0..8 code length (the whole code's, for subtable entries), 8..12 extra
// bits (for a subtable pointer: the subtable's index bits), 12..14 kind, 16..32 value.
const K_LIT: u32 = 0;
const K_LEN: u32 = 1;
const K_END: u32 = 2; // value 0: end of block; value 1: invalid code
const K_SUB: u32 = 3;
const INVALID: u32 = (K_END << 12) | (1 << 16);

#[inline(always)]
const fn ent(len: u32, extra: u32, kind: u32, val: u32) -> u32 {
    len | (extra << 8) | (kind << 12) | (val << 16)
}
#[inline(always)]
fn e_len(e: u32) -> u32 {
    e & 0xff
}
#[inline(always)]
fn e_extra(e: u32) -> u32 {
    (e >> 8) & 0xf
}
#[inline(always)]
fn e_kind(e: u32) -> u32 {
    (e >> 12) & 3
}
#[inline(always)]
fn e_val(e: u32) -> u32 {
    e >> 16
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn lit_entry(sym: usize, len: u32) -> u32 {
    match sym {
        0..=255 => ent(len, 0, K_LIT, sym as u32),
        256 => ent(len, 0, K_END, 0),
        257..=285 => {
            let i = sym - 257;
            ent(len, LEN_EXTRA[i] as u32, K_LEN, LEN_BASE[i] as u32)
        }
        _ => INVALID | len,
    }
}
fn dist_entry(sym: usize, len: u32) -> u32 {
    if sym < 30 {
        ent(len, DIST_EXTRA[sym] as u32, K_LEN, DIST_BASE[sym] as u32)
    } else {
        INVALID | len
    }
}
fn pre_entry(sym: usize, len: u32) -> u32 {
    ent(len, 0, K_LIT, sym as u32)
}

pub type Err = &'static str;

#[cfg(feature = "census")]
pub mod census {
    use core::sync::atomic::{AtomicU64, Ordering::Relaxed};
    pub static LIT: AtomicU64 = AtomicU64::new(0);
    pub static MATCH: AtomicU64 = AtomicU64::new(0);
    pub static MATCH_BYTES: AtomicU64 = AtomicU64::new(0);
    pub static DIST_LT8: AtomicU64 = AtomicU64::new(0);
    pub static DIST1: AtomicU64 = AtomicU64::new(0);
    pub static LEN_HIST: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6]; // 3-4,5-8,9-16,17-32,33-64,65+
    pub fn add(c: &AtomicU64, n: u64) {
        c.fetch_add(n, Relaxed);
    }
    pub fn get(c: &AtomicU64) -> u64 {
        c.load(Relaxed)
    }
}

/// Build a canonical-Huffman decode table with `bits` main bits and subtables.
/// Unused slots (an incomplete code) hold INVALID. Returns an error if over-subscribed.
fn build(
    table: &mut [u32],
    lens: &[u8],
    bits: u32,
    entry: fn(usize, u32) -> u32,
) -> Result<(), Err> {
    let mut count = [0u16; 16];
    for &l in lens {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut left: i32 = 1;
    for c in &count[1..] {
        left = (left << 1) - *c as i32;
        if left < 0 {
            return Err("over-subscribed code");
        }
    }
    let mut next = [0u32; 16];
    let mut code = 0u32;
    for l in 1..16 {
        code = (code + count[l - 1] as u32) << 1;
        next[l] = code;
    }
    let main = 1usize << bits;
    table[..main].fill(INVALID);
    // Reversed codes per symbol; the max length per main-table prefix sizes the subtables.
    let mut rev = [0u32; 320];
    let mut prefix_max = [0u8; 1 << LB];
    for (s, &l) in lens.iter().enumerate() {
        if l == 0 {
            continue;
        }
        let c = next[l as usize];
        next[l as usize] += 1;
        let r = c.reverse_bits() >> (32 - l as u32);
        rev[s] = r;
        if l as u32 > bits {
            let p = (r as usize) & (main - 1);
            prefix_max[p] = prefix_max[p].max(l);
        }
    }
    let mut end = main;
    for p in 0..main {
        if prefix_max[p] != 0 {
            let sb = prefix_max[p] as u32 - bits;
            if end + (1 << sb) > table.len() {
                return Err("table overflow");
            }
            table[end..end + (1 << sb)].fill(INVALID);
            table[p] = ent(bits, sb, K_SUB, end as u32);
            end += 1 << sb;
        }
    }
    for (s, &l) in lens.iter().enumerate() {
        if l == 0 {
            continue;
        }
        let (l, r) = (l as u32, rev[s]);
        let e = entry(s, l);
        if l <= bits {
            let mut i = r as usize;
            while i < main {
                table[i] = e;
                i += 1 << l;
            }
        } else {
            let p = table[(r as usize) & (main - 1)];
            let (off, sb) = (e_val(p) as usize, e_extra(p));
            let mut i = (r >> bits) as usize;
            while i < (1 << sb) {
                table[off + i] = e;
                i += 1 << (l - bits);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Header,
    Body,
    Done,
}

/// One member's decode: its bit reader, output cursor and current tables.
pub struct Stream<'a> {
    pub inp: &'a [u8], // the whole padded input; pos indexes into it
    pub start: usize,
    pub pos: usize,
    pub bb: u64,
    pub bc: u32,
    pub out: &'a mut [u8], // len = limit + SLACK
    pub o: usize,
    pub limit: usize,
    pub phase: Phase,
    pub last: bool,
    lit: Box<[u32; LIT_ENOUGH]>,
    dist: Box<[u32; DIST_ENOUGH]>,
}

impl<'a> Stream<'a> {
    pub fn new(inp: &'a [u8], start: usize, out: &'a mut [u8]) -> Self {
        let limit = out.len() - SLACK;
        Stream {
            inp,
            start,
            pos: start,
            bb: 0,
            bc: 0,
            out,
            o: 0,
            limit,
            phase: Phase::Header,
            last: false,
            lit: Box::new([INVALID; LIT_ENOUGH]),
            dist: Box::new([INVALID; DIST_ENOUGH]),
        }
    }

    /// Reuse the stream's tables for a new member.
    pub fn reset(&mut self, inp: &'a [u8], start: usize, out: &'a mut [u8]) {
        self.inp = inp;
        self.start = start;
        self.pos = start;
        self.bb = 0;
        self.bc = 0;
        self.limit = out.len() - SLACK;
        self.out = out;
        self.o = 0;
        self.phase = Phase::Header;
        self.last = false;
    }

    #[inline(always)]
    fn refill(&mut self) -> Result<(), Err> {
        let w = self
            .inp
            .get(self.pos..self.pos + 8)
            .ok_or("input overrun")?;
        self.bb |= u64::from_le_bytes(w.try_into().unwrap()) << self.bc;
        self.pos += (7 - (self.bc >> 3)) as usize;
        self.bc |= 56;
        Ok(())
    }
    fn bits(&mut self, n: u32) -> Result<u32, Err> {
        if self.bc < n {
            self.refill()?;
        }
        let v = (self.bb & ((1u64 << n) - 1)) as u32;
        self.bb >>= n;
        self.bc -= n;
        Ok(v)
    }
    /// Byte position of the next unread bit (rounded up to a byte boundary).
    pub fn byte_pos(&self) -> usize {
        self.pos - (self.bc as usize >> 3)
    }

    /// Decode one block header (or finish the member). Stored blocks are copied here whole.
    pub fn header(&mut self) -> Result<(), Err> {
        if self.last {
            // Align to a byte; the trailer follows (not checked here).
            let drop = self.bc & 7;
            self.bb >>= drop;
            self.bc -= drop;
            self.phase = Phase::Done;
            return Ok(());
        }
        self.last = self.bits(1)? == 1;
        match self.bits(2)? {
            0 => {
                let drop = self.bc & 7;
                self.bb >>= drop;
                self.bc -= drop;
                let p = self.byte_pos();
                self.pos = p;
                self.bb = 0;
                self.bc = 0;
                let h = self.inp.get(p..p + 4).ok_or("input overrun")?;
                let len = u16::from_le_bytes([h[0], h[1]]) as usize;
                if len != !u16::from_le_bytes([h[2], h[3]]) as usize {
                    return Err("stored length mismatch");
                }
                let src = self.inp.get(p + 4..p + 4 + len).ok_or("input overrun")?;
                if self.o + len > self.limit {
                    return Err("output overrun");
                }
                self.out[self.o..self.o + len].copy_from_slice(src);
                self.o += len;
                self.pos = p + 4 + len;
                self.phase = Phase::Header;
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
                self.phase = Phase::Body;
            }
            2 => {
                let hlit = self.bits(5)? as usize + 257;
                let hdist = self.bits(5)? as usize + 1;
                let hclen = self.bits(4)? as usize + 4;
                const ORD: [usize; 19] = [
                    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
                ];
                let mut pl = [0u8; 19];
                for &i in &ORD[..hclen] {
                    pl[i] = self.bits(3)? as u8;
                }
                let mut pt = [INVALID; 128];
                build(&mut pt, &pl, 7, pre_entry)?;
                let mut l = [0u8; 320];
                let mut i = 0;
                while i < hlit + hdist {
                    if self.bc < 16 {
                        self.refill()?;
                    }
                    let e = pt[(self.bb & 127) as usize];
                    if e_kind(e) != K_LIT {
                        return Err("invalid precode");
                    }
                    self.bb >>= e_len(e);
                    self.bc -= e_len(e);
                    let s = e_val(e) as usize;
                    let (v, n) = match s {
                        0..=15 => (s as u8, 1),
                        16 => {
                            if i == 0 {
                                return Err("repeat with no previous length");
                            }
                            (l[i - 1], 3 + self.bits(2)? as usize)
                        }
                        17 => (0, 3 + self.bits(3)? as usize),
                        _ => (0, 11 + self.bits(7)? as usize),
                    };
                    if i + n > hlit + hdist {
                        return Err("code lengths overflow");
                    }
                    l[i..i + n].fill(v);
                    i += n;
                }
                if l[256] == 0 {
                    return Err("no end-of-block code");
                }
                build(&mut self.lit[..], &l[..hlit], LB, lit_entry)?;
                build(&mut self.dist[..], &l[hlit..hlit + hdist], DB, dist_entry)?;
                self.phase = Phase::Body;
            }
            _ => return Err("invalid block type"),
        }
        Ok(())
    }
}

/// Decode one symbol of a stream in its block body. Returns Ok(true) at end of block.
/// All state is passed as locals so that, inlined into `hot::<N>` with constant indices,
/// LLVM can hold every stream's bit buffer and cursors in registers.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn step(
    inp: &[u8],
    lit: &[u32; LIT_ENOUGH],
    dist: &[u32; DIST_ENOUGH],
    out: &mut [u8],
    limit: usize,
    pos: &mut usize,
    bb: &mut u64,
    bc: &mut u32,
    o: &mut usize,
) -> Result<bool, Err> {
    // Refill: at least 56 valid bits, enough for any length code plus its distance code.
    let w = inp.get(*pos..*pos + 8).ok_or("input overrun")?;
    *bb |= u64::from_le_bytes(w.try_into().unwrap()) << *bc;
    *pos += (7 - (*bc >> 3)) as usize;
    *bc |= 56;

    let mut e = lit[(*bb & ((1 << LB) - 1)) as usize];
    if e_kind(e) == K_SUB {
        e = lit[(e_val(e) as usize + ((*bb >> LB) & ((1 << e_extra(e)) - 1)) as usize)
            .min(LIT_ENOUGH - 1)];
    }
    let cl = e_len(e);
    match e_kind(e) {
        K_LIT => {
            if *o >= limit {
                return Err("output overrun");
            }
            out[*o] = e_val(e) as u8;
            *o += 1;
            #[cfg(feature = "census")]
            census::add(&census::LIT, 1);
            *bb >>= cl;
            *bc -= cl;
            Ok(false)
        }
        K_LEN => {
            let ex = e_extra(e);
            let len = e_val(e) as usize + ((*bb >> cl) & ((1u64 << ex) - 1)) as usize;
            *bb >>= cl + ex;
            *bc -= cl + ex;
            let mut d = dist[(*bb & ((1 << DB) - 1)) as usize];
            if e_kind(d) == K_SUB {
                d = dist[(e_val(d) as usize + ((*bb >> DB) & ((1 << e_extra(d)) - 1)) as usize)
                    .min(DIST_ENOUGH - 1)];
            }
            if e_kind(d) != K_LEN {
                return Err("invalid distance code");
            }
            let (dl, dx) = (e_len(d), e_extra(d));
            let dst = e_val(d) as usize + ((*bb >> dl) & ((1u64 << dx) - 1)) as usize;
            *bb >>= dl + dx;
            *bc -= dl + dx;
            if dst > *o {
                return Err("distance too far back");
            }
            if *o + len > limit {
                return Err("output overrun");
            }
            #[cfg(feature = "census")]
            {
                use census::*;
                add(&MATCH, 1);
                add(&MATCH_BYTES, len as u64);
                add(&DIST_LT8, (dst < 8) as u64);
                add(&DIST1, (dst == 1) as u64);
                let b = match len {
                    0..=4 => 0,
                    5..=8 => 1,
                    9..=16 => 2,
                    17..=32 => 3,
                    33..=64 => 4,
                    _ => 5,
                };
                add(&LEN_HIST[b], 1);
            }
            let mut s = *o - dst;
            let end = *o + len;
            let mut t = *o;
            if dst >= 8 {
                while t < end {
                    let v: [u8; 8] = out[s..s + 8].try_into().unwrap();
                    out[t..t + 8].copy_from_slice(&v);
                    s += 8;
                    t += 8;
                }
            } else if dst == 1 {
                let b = out[s];
                out[t..end].fill(b);
            } else {
                while t < end {
                    out[t] = out[s];
                    s += 1;
                    t += 1;
                }
            }
            *o = end;
            Ok(false)
        }
        _ => {
            if e_val(e) != 0 {
                return Err("invalid literal/length code");
            }
            *bb >>= cl;
            *bc -= cl;
            Ok(true)
        }
    }
}

/// Advance N streams, all in a block body, round-robin `B` symbols at a time, until any
/// one reaches the end of its block. The per-stream body is unrolled by macro with literal
/// indices, so every stream's state is a scalar that LLVM can keep in a register.
macro_rules! hot_unrolled {
    ($name:ident, $n:literal, [$($i:literal),+]) => {
        #[inline(never)]
        fn $name<const B: usize>(ss: &mut [Stream<'_>]) -> Result<(), Err> {
            let ss: &mut [Stream<'_>; $n] = ss.try_into().unwrap();
            let inp = ss[0].inp;
            let mut pos = [0usize; $n];
            let mut bb = [0u64; $n];
            let mut bc = [0u32; $n];
            let mut o = [0usize; $n];
            let mut limit = [0usize; $n];
            let mut outs: [&mut [u8]; $n] = core::array::from_fn(|_| <&mut [u8]>::default());
            for i in 0..$n {
                debug_assert!(core::ptr::eq(ss[i].inp, inp));
                pos[i] = ss[i].pos;
                bb[i] = ss[i].bb;
                bc[i] = ss[i].bc;
                o[i] = ss[i].o;
                limit[i] = ss[i].limit;
                outs[i] = core::mem::take(&mut ss[i].out);
            }
            let mut ended = [false; $n];
            let r: Result<(), Err> = {
                let lits: [&[u32; LIT_ENOUGH]; $n] = core::array::from_fn(|i| &*ss[i].lit);
                let dists: [&[u32; DIST_ENOUGH]; $n] = core::array::from_fn(|i| &*ss[i].dist);
                'outer: loop {
                    $(
                        for _ in 0..B {
                            match step(inp, lits[$i], dists[$i], outs[$i], limit[$i], &mut pos[$i], &mut bb[$i], &mut bc[$i], &mut o[$i]) {
                                Ok(false) => {}
                                Ok(true) => {
                                    ended[$i] = true;
                                    break;
                                }
                                Err(e) => break 'outer Err(e),
                            }
                        }
                    )+
                    if false $(|| ended[$i])+ {
                        break Ok(());
                    }
                }
            };
            for (i, out) in outs.into_iter().enumerate() {
                ss[i].out = out;
                ss[i].pos = pos[i];
                ss[i].bb = bb[i];
                ss[i].bc = bc[i];
                ss[i].o = o[i];
                if ended[i] {
                    ss[i].phase = Phase::Header;
                }
            }
            r
        }
    };
}
hot_unrolled!(hot1, 1, [0]);
hot_unrolled!(hot2, 2, [0, 1]);
hot_unrolled!(hot3, 3, [0, 1, 2]);
hot_unrolled!(hot4, 4, [0, 1, 2, 3]);
hot_unrolled!(hot5, 5, [0, 1, 2, 3, 4]);
hot_unrolled!(hot6, 6, [0, 1, 2, 3, 4, 5]);
hot_unrolled!(hot7, 7, [0, 1, 2, 3, 4, 5, 6]);
hot_unrolled!(hot8, 8, [0, 1, 2, 3, 4, 5, 6, 7]);

fn hot_dyn<const B: usize>(ss: &mut [Stream<'_>]) -> Result<(), Err> {
    match ss.len() {
        1 => hot1::<B>(ss),
        2 => hot2::<B>(ss),
        3 => hot3::<B>(ss),
        4 => hot4::<B>(ss),
        5 => hot5::<B>(ss),
        6 => hot6::<B>(ss),
        7 => hot7::<B>(ss),
        8 => hot8::<B>(ss),
        _ => unreachable!(),
    }
}

/// The rolled form (kept as an arm): a runtime loop over streams, so state stays in
/// stack arrays indexed at run time.
///
/// Advance N streams, all in a block body, round-robin `B` symbols at a time, until any
/// one reaches the end of its block. States are copied into local arrays for the loop.
#[inline(never)]
fn hot_rolled<const N: usize, const B: usize>(ss: &mut [Stream<'_>]) -> Result<(), Err> {
    let ss: &mut [Stream<'_>; N] = ss.try_into().unwrap();
    let inp = ss[0].inp;
    let mut pos = [0usize; N];
    let mut bb = [0u64; N];
    let mut bc = [0u32; N];
    let mut o = [0usize; N];
    let mut limit = [0usize; N];
    let mut outs: [&mut [u8]; N] = core::array::from_fn(|_| <&mut [u8]>::default());
    for i in 0..N {
        debug_assert!(core::ptr::eq(ss[i].inp, inp));
        pos[i] = ss[i].pos;
        bb[i] = ss[i].bb;
        bc[i] = ss[i].bc;
        o[i] = ss[i].o;
        limit[i] = ss[i].limit;
        outs[i] = core::mem::take(&mut ss[i].out);
    }
    let mut ended = [false; N];
    let r = {
        let lits: [&[u32; LIT_ENOUGH]; N] = core::array::from_fn(|i| &*ss[i].lit);
        let dists: [&[u32; DIST_ENOUGH]; N] = core::array::from_fn(|i| &*ss[i].dist);
        'outer: loop {
            for i in 0..N {
                for _ in 0..B {
                    match step(
                        inp,
                        lits[i],
                        dists[i],
                        outs[i],
                        limit[i],
                        &mut pos[i],
                        &mut bb[i],
                        &mut bc[i],
                        &mut o[i],
                    ) {
                        Ok(false) => {}
                        Ok(true) => {
                            ended[i] = true;
                            break;
                        }
                        Err(e) => break 'outer Err(e),
                    }
                }
            }
            if ended.iter().any(|&x| x) {
                break Ok(());
            }
        }
    };
    for (i, out) in outs.into_iter().enumerate() {
        ss[i].out = out;
        ss[i].pos = pos[i];
        ss[i].bb = bb[i];
        ss[i].bc = bc[i];
        ss[i].o = o[i];
        if ended[i] {
            ss[i].phase = Phase::Header;
        }
    }
    r
}

fn hot_rolled_dyn<const B: usize>(ss: &mut [Stream<'_>]) -> Result<(), Err> {
    match ss.len() {
        1 => hot_rolled::<1, B>(ss),
        2 => hot_rolled::<2, B>(ss),
        3 => hot_rolled::<3, B>(ss),
        4 => hot_rolled::<4, B>(ss),
        5 => hot_rolled::<5, B>(ss),
        6 => hot_rolled::<6, B>(ss),
        7 => hot_rolled::<7, B>(ss),
        8 => hot_rolled::<8, B>(ss),
        _ => unreachable!(),
    }
}

/// Decode a group of streams (at most 8) to completion, interleaved. As streams finish,
/// the group narrows (N drops), so a group of one is exactly the sequential decoder.
pub fn decode_group<const B: usize>(ss: &mut [Stream<'_>], rolled: bool) -> Result<(), Err> {
    let mut idx: [usize; 8] = core::array::from_fn(|i| i);
    let mut n = ss.len();
    assert!(n <= 8);
    loop {
        // Headers (rare) one stream at a time; streams that finished drop out of the group.
        let mut k = 0;
        while k < n {
            let s = &mut ss[idx[k]];
            while s.phase == Phase::Header {
                s.header()?;
            }
            if s.phase == Phase::Done {
                idx.swap(k, n - 1);
                n -= 1;
            } else {
                k += 1;
            }
        }
        if n == 0 {
            return Ok(());
        }
        // Gather the live streams to the front so hot::<n> sees a contiguous slice.
        idx[..n].sort_unstable();
        for (j, &i) in idx[..n].iter().enumerate() {
            ss.swap(j, i);
        }
        for (j, v) in idx[..n].iter_mut().enumerate() {
            *v = j;
        }
        if rolled {
            hot_rolled_dyn::<B>(&mut ss[..n])?;
        } else {
            hot_dyn::<B>(&mut ss[..n])?;
        }
    }
}
