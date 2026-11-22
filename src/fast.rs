//! A single-stream DEFLATE body decoder in the libdeflate style (the upigz kernel, v1).
//!
//! Safe Rust. Techniques, after libdeflate's `decompress_template.h`:
//! - one table entry says how many bits to consume (code plus extra bits) in its low byte,
//!   so a symbol is consumed with one shift; extra bits are read from the saved bit buffer;
//! - the next litlen entry is looked up before the current literal is stored or match copied,
//!   so the table load overlaps the copy;
//! - up to three literals per refill;
//! - a fast loop guarded by margins (input: the padded buffer; output: 320 bytes of room),
//!   with whole-word match copies that may write past the match end into that room; the
//!   exact tail is decoded by a checked slow loop over the same tables.

pub type Err = &'static str;

// Fast-loop memory access. Safe (bounds-checked) by default; with the research-only
// `unchecked` feature they skip the check. Safety argument for the unchecked forms, all in
// the fast loop: pos <= in_end = inp.len() - 8, so the 8-byte refill is in bounds;
// o <= out_end = limit - OUT_MARGIN at the top of an iteration, so up to 3 literal stores and a
// match (<= 258 bytes plus at most 39 bytes of word overshoot) stay below limit + 16 <=
// out.len(); dst <= o is checked before any copy, so sources start at >= 0 and stay below the
// write cursor; subtable indices are < table length because `build` checks every subtable
// fits and only `build` sets SUB.
#[cfg(not(feature = "unchecked"))]
macro_rules! get {
    ($s:expr, $i:expr) => {
        $s[$i]
    };
}
#[cfg(feature = "unchecked")]
macro_rules! get {
    ($s:expr, $i:expr) => {
        unsafe { *$s.get_unchecked($i) }
    };
}
#[cfg(not(feature = "unchecked"))]
macro_rules! put {
    ($s:expr, $i:expr, $v:expr) => {
        $s[$i] = $v
    };
}
#[cfg(feature = "unchecked")]
macro_rules! put {
    ($s:expr, $i:expr, $v:expr) => {
        unsafe { *$s.get_unchecked_mut($i) = $v }
    };
}
#[cfg(not(feature = "unchecked"))]
macro_rules! rd {
    ($s:expr, $i:expr, $n:literal) => {{
        let a: [u8; $n] = $s[$i..$i + $n].try_into().unwrap();
        a
    }};
}
#[cfg(feature = "unchecked")]
macro_rules! rd {
    ($s:expr, $i:expr, $n:literal) => {
        unsafe { ($s.as_ptr().add($i) as *const [u8; $n]).read_unaligned() }
    };
}
#[cfg(not(feature = "unchecked"))]
macro_rules! wr {
    ($s:expr, $i:expr, $v:expr, $n:literal) => {
        $s[$i..$i + $n].copy_from_slice(&$v)
    };
}
#[cfg(feature = "unchecked")]
macro_rules! wr {
    ($s:expr, $i:expr, $v:expr, $n:literal) => {
        unsafe { ($s.as_mut_ptr().add($i) as *mut [u8; $n]).write_unaligned($v) }
    };
}

pub const LB: u32 = 11;
pub const DB: u32 = 8;
const LIT_ENOUGH: usize = 2342;
const DIST_ENOUGH: usize = 402;

// Entry: bits 0..8 bits to consume; 8..12 code length (lengths/distances: where the extra
// bits start) or a subtable's index bits; bit 13 end of block; bit 14 subtable; bit 15
// exceptional (subtable, end of block or invalid); bits 16..31 value; bit 31 literal.
const LIT: u32 = 1 << 31;
const EXC: u32 = 1 << 15;
const SUB: u32 = 1 << 14;
const EOB: u32 = 1 << 13;
const INVALID: u32 = EXC;

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// `l`: the code length to consume from this (sub)table's index position.
fn lit_entry(sym: usize, l: u32) -> u32 {
    match sym {
        0..=255 => LIT | ((sym as u32) << 16) | (l << 8) | l,
        256 => EXC | EOB | (l << 8) | l,
        257..=285 => {
            let i = sym - 257;
            ((LEN_BASE[i] as u32) << 16) | (l << 8) | (l + LEN_EXTRA[i] as u32)
        }
        _ => INVALID | l,
    }
}
fn dist_entry(sym: usize, l: u32) -> u32 {
    if sym < 30 {
        ((DIST_BASE[sym] as u32) << 16) | (l << 8) | (l + DIST_EXTRA[sym] as u32)
    } else {
        INVALID | l
    }
}

/// Canonical Huffman table with `bits` main bits. Subtable entries consume only the bits past
/// the main index (the pointer entry consumes the main bits first).
fn build(table: &mut [u32], lens: &[u8], bits: u32, entry: fn(usize, u32) -> u32) -> Result<(), Err> {
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
    let mut rev = [0u32; 288];
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
            table[p] = EXC | SUB | ((end as u32) << 16) | (sb << 8) | bits;
            end += 1 << sb;
        }
    }
    for (s, &l) in lens.iter().enumerate() {
        if l == 0 {
            continue;
        }
        let (l, r) = (l as u32, rev[s]);
        if l <= bits {
            let e = entry(s, l);
            let mut i = r as usize;
            while i < main {
                table[i] = e;
                i += 1 << l;
            }
        } else {
            let e = entry(s, l - bits);
            let p = table[(r as usize) & (main - 1)];
            let (off, sb) = ((p >> 16) as usize & 0x7fff, (p >> 8) & 0xf);
            let mut i = (r >> bits) as usize;
            while i < (1 << sb) {
                table[off + i] = e;
                i += 1 << (l - bits);
            }
        }
    }
    Ok(())
}

pub struct Decoder {
    lit: Box<[u32; LIT_ENOUGH]>,
    dist: Box<[u32; DIST_ENOUGH]>,
}

impl Default for Decoder {
    fn default() -> Self {
        Decoder { lit: Box::new([INVALID; LIT_ENOUGH]), dist: Box::new([INVALID; DIST_ENOUGH]) }
    }
}

struct Bits<'a> {
    inp: &'a [u8], // padded with at least 8 readable bytes past any valid stream
    pos: usize,
    bb: u64,
    bc: u32,
}

impl Bits<'_> {
    #[inline(always)]
    fn refill(&mut self) -> Result<(), Err> {
        let w = self.inp.get(self.pos..self.pos + 8).ok_or("input overrun")?;
        self.bb |= u64::from_le_bytes(w.try_into().unwrap()) << self.bc;
        self.pos += (7 - ((self.bc >> 3) & 7)) as usize;
        self.bc |= 56;
        Ok(())
    }
    fn take(&mut self, n: u32) -> Result<u32, Err> {
        if self.bc < n {
            self.refill()?;
        }
        let v = (self.bb & ((1u64 << n) - 1)) as u32;
        self.bb >>= n;
        self.bc -= n;
        Ok(v)
    }
    fn align(&mut self) {
        let d = self.bc & 7;
        self.bb >>= d;
        self.bc -= d;
    }
    fn byte_pos(&self) -> usize {
        self.pos - (self.bc as usize >> 3)
    }
}

/// The extra bits after a length/distance code, for non-exceptional entries (flag bits
/// 13..15 clear, so `e >> 8` is exactly the code length). `bzhi` takes its index from the
/// low 8 bits and `shrx` its count from the low 6, so no masking instructions are needed.
#[inline(always)]
fn extra_fast(saved: u64, e: u32) -> usize {
    // exactly bzhi's semantics (index = low byte; >= 64 leaves the value whole), so LLVM can
    // emit a single bzhi with no masking
    let n = e & 0xff;
    let low = if n < 64 { saved & ((1u64 << n) - 1) } else { saved };
    low.wrapping_shr(e >> 8) as usize
}

#[inline(always)]
fn extra(saved: u64, e: u32) -> usize {
    // (saved mod 2^consume) >> codelen: the extra bits that follow the code
    ((saved & !(u64::MAX << (e & 0x3f))) >> ((e >> 8) & 0xf)) as usize
}

const OUT_MARGIN: usize = 258 + 64;

impl Decoder {
    /// Decode the DEFLATE stream starting at `inp[start]` into `out[..limit]` (`out` has at
    /// least 16 spare bytes past `limit`, unused). Returns (bytes written, end byte position).
    pub fn decode(&mut self, inp: &[u8], start: usize, out: &mut [u8], limit: usize) -> Result<(usize, usize), Err> {
        let mut b = Bits { inp, pos: start, bb: 0, bc: 0 };
        let mut o = 0usize;
        loop {
            let last = b.take(1)? == 1;
            match b.take(2)? {
                0 => {
                    b.align();
                    let p = b.byte_pos();
                    let h = inp.get(p..p + 4).ok_or("input overrun")?;
                    let len = u16::from_le_bytes([h[0], h[1]]) as usize;
                    if len != !u16::from_le_bytes([h[2], h[3]]) as usize {
                        return Err("stored length mismatch");
                    }
                    let src = inp.get(p + 4..p + 4 + len).ok_or("input overrun")?;
                    if o + len > limit {
                        return Err("output overrun");
                    }
                    out[o..o + len].copy_from_slice(src);
                    o += len;
                    b = Bits { inp, pos: p + 4 + len, bb: 0, bc: 0 };
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
                    o = self.body(&mut b, out, o, limit)?;
                }
                2 => {
                    self.dynamic_header(&mut b)?;
                    o = self.body(&mut b, out, o, limit)?;
                }
                _ => return Err("invalid block type"),
            }
            if last {
                b.align();
                return Ok((o, b.byte_pos()));
            }
        }
    }

    fn dynamic_header(&mut self, b: &mut Bits<'_>) -> Result<(), Err> {
        let hlit = b.take(5)? as usize + 257;
        let hdist = b.take(5)? as usize + 1;
        let hclen = b.take(4)? as usize + 4;
        const ORD: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
        let mut pl = [0u8; 19];
        for &i in &ORD[..hclen] {
            pl[i] = b.take(3)? as u8;
        }
        // precode: 7-bit direct table, entry = len | sym << 16
        let mut pt = [INVALID; 128];
        build(&mut pt, &pl, 7, |s, l| ((s as u32) << 16) | l)?;
        let mut l = [0u8; 320];
        let mut i = 0;
        while i < hlit + hdist {
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
        build(&mut self.dist[..], &l[hlit..hlit + hdist], DB, dist_entry)
    }

    /// One block body: the fast loop while margins hold, then the checked loop.
    #[inline(never)]
    fn body(&self, b: &mut Bits<'_>, out: &mut [u8], mut o: usize, limit: usize) -> Result<usize, Err> {
        let lit: &[u32; LIT_ENOUGH] = &self.lit;
        let dist: &[u32; DIST_ENOUGH] = &self.dist;
        let inp = b.inp;
        let (mut pos, mut bb, mut bc) = (b.pos, b.bb, b.bc);
        let in_end = inp.len().saturating_sub(8);
        let out_end = limit.saturating_sub(OUT_MARGIN);
        const LM: u64 = (1 << LB) - 1;
        const DM: u64 = (1 << DB) - 1;

        // ---- fast loop (libdeflate's control flow) ----
        // Invariant at the top of an iteration: >= 56 bits claimed (`bc as u8`), and
        // e = lit[bb & LM]. Inside the loop `bc` carries garbage above bit 7 (full entries
        // are subtracted); its low byte is the true count, its low 6 bits drive the refill.
        macro_rules! refill {
            () => {{
                let w: [u8; 8] = rd!(inp, pos, 8);
                bb |= u64::from_le_bytes(w).wrapping_shl(bc);
                pos += (7 - ((bc >> 3) & 7)) as usize;
                bc |= 56;
            }};
        }
        macro_rules! consume {
            ($x:expr) => {{
                bb = bb.wrapping_shr($x);
                bc = bc.wrapping_sub($x);
            }};
        }
        if pos > in_end || o > out_end {
            // too close to either end for the fast loop
        } else {
            if (bc & 0xff) < 56 {
                refill!();
            }
        }
        let mut e = lit[(bb & LM) as usize];
        let ended = 'fast: loop {
            if pos > in_end || o > out_end {
                break false;
            }
            let mut saved = bb;
            consume!(e);
            if e & LIT != 0 {
                // a main-table literal (<= 11 bits); two extra literals (<= 22 bits) and
                // then possibly a length (<= 20) still fit in the 56 claimed bits
                let l = (e >> 16) as u8;
                e = lit[(bb & LM) as usize];
                saved = bb;
                consume!(e);
                put!(out, o, l);
                o += 1;
                if e & LIT != 0 {
                    let l = (e >> 16) as u8;
                    e = lit[(bb & LM) as usize];
                    saved = bb;
                    consume!(e);
                    put!(out, o, l);
                    o += 1;
                    if e & LIT != 0 {
                        // a third literal, in lieu of the primary item
                        let l = (e >> 16) as u8;
                        e = lit[(bb & LM) as usize];
                        refill!();
                        put!(out, o, l);
                        o += 1;
                        continue;
                    }
                }
                // e (already consumed) is a length, a subtable pointer or end of block:
                // fall through to decode it in this iteration
            }
            if e & EXC != 0 {
                if e & SUB == 0 {
                    if e & EOB != 0 {
                        break true;
                    }
                    return Err("invalid literal/length code");
                }
                // main bits were consumed with the pointer; now the subtable entry
                e = get!(lit, ((e >> 16) & 0x7fff) as usize + (bb & ((1 << ((e >> 8) & 0xf)) - 1)) as usize);
                saved = bb;
                consume!(e);
                if e & LIT != 0 {
                    let l = (e >> 16) as u8;
                    e = lit[(bb & LM) as usize];
                    refill!();
                    put!(out, o, l);
                    o += 1;
                    continue;
                }
                if e & EXC != 0 {
                    if e & EOB != 0 {
                        break true;
                    }
                    return Err("invalid literal/length code");
                }
            }
            // a length: its code and extra bits were consumed with the entry
            let len = (e >> 16) as usize + extra_fast(saved, e);
            // at most 42 bits consumed so far: >= 14 claimed, >= 22 valid for the preload
            let mut d = dist[(bb & DM) as usize];
            if d & EXC != 0 {
                if d & SUB == 0 {
                    return Err("invalid distance code");
                }
                if (bc & 0xff) < 28 + 11 {
                    refill!();
                }
                consume!(DB);
                d = get!(dist, ((d >> 16) & 0x7fff) as usize + (bb & ((1 << ((d >> 8) & 0xf)) - 1)) as usize);
                if d & EXC != 0 {
                    return Err("invalid distance code");
                }
            } else if (bc & 0xff) < 28 + 11 {
                refill!();
            }
            let saved = bb;
            consume!(d);
            let dst = (d >> 16) as usize + extra_fast(saved, d);
            if dst > o {
                return Err("distance too far back");
            }
            let end = o + len;
            // preload the next litlen entry, then refill, before the copy (>= 11 claimed bits
            // remain after <= 28 consumed from >= 39)
            e = lit[(bb & LM) as usize];
            refill!();
            let mut s = o - dst;
            let mut t = o;
            if dst >= 8 {
                // libdeflate's shape: five 8-byte words unconditionally (len <= 40), then
                // five-word strides; may write up to 39 bytes past `end`, inside the margin
                macro_rules! w5 {
                    () => {
                        let v: [u8; 8] = rd!(out, s, 8);
                        wr!(out, t, v, 8);
                        let v: [u8; 8] = rd!(out, s + 8, 8);
                        wr!(out, t + 8, v, 8);
                        let v: [u8; 8] = rd!(out, s + 16, 8);
                        wr!(out, t + 16, v, 8);
                        let v: [u8; 8] = rd!(out, s + 24, 8);
                        wr!(out, t + 24, v, 8);
                        let v: [u8; 8] = rd!(out, s + 32, 8);
                        wr!(out, t + 32, v, 8);
                        s += 40;
                        t += 40;
                    };
                }
                w5!();
                while t < end {
                    w5!();
                }
            } else if dst == 1 {
                let v = [get!(out, s); 16];
                wr!(out, t, v, 16);
                wr!(out, t + 16, v, 16);
                t += 32;
                while t < end {
                    wr!(out, t, v, 16);
                    t += 16;
                }
            } else {
                while t < end {
                    put!(out, t, get!(out, s));
                    s += 1;
                    t += 1;
                }
            }
            o = end;
            continue 'fast;
        };
        bc &= 0xff;
        if ended {
            b.pos = pos;
            b.bb = bb;
            b.bc = bc;
            return Ok(o);
        }

        // ---- checked loop: one symbol at a time, exact bounds ----
        loop {
            if bc < 56 {
                let w = inp.get(pos..pos + 8).ok_or("input overrun")?;
                bb |= u64::from_le_bytes(w.try_into().unwrap()) << bc;
                pos += (7 - ((bc >> 3) & 7)) as usize;
                bc |= 56;
            }
            let mut e = lit[(bb & LM) as usize];
            if e & SUB != 0 {
                bb >>= e & 0x3f;
                bc -= e & 0xff;
                e = lit[((e >> 16) & 0x7fff) as usize + (bb & ((1 << ((e >> 8) & 0xf)) - 1)) as usize];
            }
            if e & LIT != 0 {
                if o >= limit {
                    return Err("output overrun");
                }
                bb >>= e & 0x3f;
                bc -= e & 0xff;
                out[o] = (e >> 16) as u8;
                o += 1;
                continue;
            }
            if e & EXC != 0 {
                if e & EOB != 0 {
                    bb >>= e & 0x3f;
                    bc -= e & 0xff;
                    break;
                }
                return Err("invalid literal/length code");
            }
            let saved = bb;
            bb >>= e & 0x3f;
            bc -= e & 0xff;
            let len = (e >> 16) as usize + extra(saved, e);
            let mut d = dist[(bb & DM) as usize];
            if d & SUB != 0 {
                bb >>= d & 0x3f;
                bc -= d & 0xff;
                d = dist[((d >> 16) & 0x7fff) as usize + (bb & ((1 << ((d >> 8) & 0xf)) - 1)) as usize];
            }
            if d & EXC != 0 {
                return Err("invalid distance code");
            }
            let saved = bb;
            bb >>= d & 0x3f;
            bc -= d & 0xff;
            let dst = (d >> 16) as usize + extra(saved, d);
            if dst > o {
                return Err("distance too far back");
            }
            if o + len > limit {
                return Err("output overrun");
            }
            for t in o..o + len {
                out[t] = out[t - dst];
            }
            o += len;
        }
        b.pos = pos;
        b.bb = bb;
        b.bc = bc;
        Ok(o)
    }
}
