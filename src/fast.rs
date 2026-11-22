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
pub(crate) const LIT_ENOUGH: usize = 2342;
pub(crate) const DIST_ENOUGH: usize = 402;

// Entry: bits 0..8 bits to consume; 8..12 code length (lengths/distances: where the extra
// bits start) or a subtable's index bits; bit 13 end of block; bit 14 subtable; bit 15
// exceptional (subtable, end of block or invalid); bits 16..31 value; bit 31 literal.
pub(crate) const LIT: u32 = 1 << 31;
pub(crate) const EXC: u32 = 1 << 15;
pub(crate) const SUB: u32 = 1 << 14;
pub(crate) const EOB: u32 = 1 << 13;
pub(crate) const INVALID: u32 = EXC;

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
pub(crate) fn lit_entry(sym: usize, l: u32) -> u32 {
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
pub(crate) fn dist_entry(sym: usize, l: u32) -> u32 {
    if sym < 30 {
        ((DIST_BASE[sym] as u32) << 16) | (l << 8) | (l + DIST_EXTRA[sym] as u32)
    } else {
        INVALID | l
    }
}

/// Canonical Huffman table with `bits` main bits. Subtable entries consume only the bits past
/// the main index (the pointer entry consumes the main bits first).
fn build_slow<F: Fn(usize, u32) -> u32>(table: &mut [u32], lens: &[u8], bits: u32, entry: F) -> Result<(), Err> {
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

/// Build a decode table: libdeflate's method for complete codes (codewords of each length
/// written into the first 2^len slots in bit-reversed canonical order, then the table doubled
/// by contiguous copies; no clearing, since a complete code covers every slot), and the
/// clearing builder for incomplete codes.
#[inline(always)]
pub(crate) fn build<F: Fn(usize, u32) -> u32 + Copy>(table: &mut [u32], lens: &[u8], bits: u32, entry: F) -> Result<(), Err> {
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
    if left != 0 {
        return build_slow(table, lens, bits, entry);
    }
    // canonical order: by length, then symbol
    let mut offs = [0u16; 16];
    for l in 1..15 {
        offs[l + 1] = offs[l] + count[l];
    }
    let mut sorted = [0u16; 288];
    for (sym, &l) in lens.iter().enumerate() {
        if l != 0 {
            sorted[offs[l as usize] as usize] = sym as u16;
            offs[l as usize] += 1;
        }
    }
    let main = 1usize << bits;
    let mut len = 1usize;
    while count[len] == 0 {
        len += 1;
    }
    if len as u32 > bits {
        return build_slow(table, lens, bits, entry);
    }
    let mut cur_end = 1usize << len;
    let mut cw = 0usize; // bit-reversed codeword, `len` bits
    let mut i = 0usize;
    loop {
        for _ in 0..count[len] {
            table[cw] = entry(sorted[i] as usize, len as u32);
            i += 1;
            if cw == cur_end - 1 {
                // the last codeword: the code is complete at this length
                while cur_end < main {
                    table.copy_within(0..cur_end, cur_end);
                    cur_end <<= 1;
                }
                return Ok(());
            }
            let bit = 1usize << (usize::BITS - 1 - (cw ^ (cur_end - 1)).leading_zeros());
            cw &= bit - 1;
            cw |= bit;
        }
        len += 1;
        if len as u32 > bits {
            break;
        }
        table.copy_within(0..cur_end, cur_end);
        cur_end <<= 1;
    }
    // codes longer than `bits`: subtables, in canonical order (codes sharing a main-table
    // prefix are contiguous)
    let mut cnt = count;
    let mut cur_end_l = 1usize << len; // codeword space at the current length
    let mut prefix = usize::MAX;
    let mut sub_start = 0usize;
    let mut sub_bits = 0u32;
    let mut next_sub = main;
    loop {
        while cnt[len] == 0 {
            len += 1;
            cur_end_l <<= 1;
            if len > 15 {
                return Err("bad code");
            }
        }
        let p = cw & (main - 1);
        if p != prefix {
            prefix = p;
            sub_start = next_sub;
            sub_bits = len as u32 - bits;
            let mut used = cnt[len] as u32;
            while used < (1 << sub_bits) {
                sub_bits += 1;
                if bits + sub_bits > 15 {
                    return Err("bad code");
                }
                used = (used << 1) + cnt[(bits + sub_bits) as usize] as u32;
            }
            next_sub = sub_start + (1 << sub_bits);
            if next_sub > table.len() {
                return Err("table overflow");
            }
            table[p] = EXC | SUB | ((sub_start as u32) << 16) | (sub_bits << 8) | bits;
        }
        let e = entry(sorted[i] as usize, len as u32 - bits);
        i += 1;
        cnt[len] -= 1;
        let stride = 1usize << (len as u32 - bits);
        let mut j = cw >> bits;
        while j < (1 << sub_bits) {
            table[sub_start + j] = e;
            j += stride;
        }
        if cw == cur_end_l - 1 {
            return Ok(());
        }
        let bit = 1usize << (usize::BITS - 1 - (cw ^ (cur_end_l - 1)).leading_zeros());
        cw &= bit - 1;
        cw |= bit;
    }
}

#[cfg(feature = "census")]
pub mod prof {
    use core::sync::atomic::{AtomicU64, Ordering::Relaxed};
    pub static BLOCKS: AtomicU64 = AtomicU64::new(0);
    pub static BUILD_NS: AtomicU64 = AtomicU64::new(0);
    pub static HEADER_NS: AtomicU64 = AtomicU64::new(0);
    pub fn add(c: &AtomicU64, n: u64) {
        c.fetch_add(n, Relaxed);
    }
    pub fn get(c: &AtomicU64) -> u64 {
        c.load(Relaxed)
    }
}

/// An output symbol: a byte (the real decoder) or a 16-bit symbol (speculative decoding,
/// where values >= 256 are markers for unknown window bytes). Copies move 8-byte words.
pub trait Sym: Copy + Default + Send + Sync + 'static {
    const PW: usize; // symbols per 8-byte word
    type W: Copy;
    fn lit(b: u8) -> Self;
    fn rdw(s: &[Self], i: usize) -> Self::W;
    fn wrw(s: &mut [Self], i: usize, w: Self::W);
    fn splat(v: Self) -> Self::W;
}

impl Sym for u8 {
    const PW: usize = 8;
    type W = [u8; 8];
    #[inline(always)]
    fn lit(b: u8) -> u8 {
        b
    }
    #[inline(always)]
    fn rdw(s: &[u8], i: usize) -> [u8; 8] {
        rd!(s, i, 8)
    }
    #[inline(always)]
    fn wrw(s: &mut [u8], i: usize, w: [u8; 8]) {
        wr!(s, i, w, 8)
    }
    #[inline(always)]
    fn splat(v: u8) -> [u8; 8] {
        [v; 8]
    }
}

impl Sym for u16 {
    const PW: usize = 4;
    type W = [u16; 4];
    #[inline(always)]
    fn lit(b: u8) -> u16 {
        b as u16
    }
    #[inline(always)]
    fn rdw(s: &[u16], i: usize) -> [u16; 4] {
        #[cfg(feature = "unchecked")]
        unsafe {
            (s.as_ptr().add(i) as *const [u16; 4]).read_unaligned()
        }
        #[cfg(not(feature = "unchecked"))]
        s[i..i + 4].try_into().unwrap()
    }
    #[inline(always)]
    fn wrw(s: &mut [u16], i: usize, w: [u16; 4]) {
        #[cfg(feature = "unchecked")]
        unsafe {
            (s.as_mut_ptr().add(i) as *mut [u16; 4]).write_unaligned(w)
        }
        #[cfg(not(feature = "unchecked"))]
        s[i..i + 4].copy_from_slice(&w)
    }
    #[inline(always)]
    fn splat(v: u16) -> [u16; 4] {
        [v; 4]
    }
}

/// Kraft equality: the lengths form a complete prefix code.
fn complete(lens: &[u8]) -> bool {
    let mut count = [0u16; 16];
    for &l in lens {
        count[l as usize] += 1;
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
    fn at(inp: &[u8], bitpos: u64) -> Result<Bits<'_>, Err> {
        let mut b = Bits { inp, pos: (bitpos / 8) as usize, bb: 0, bc: 0 };
        let skip = (bitpos % 8) as u32;
        if skip != 0 {
            b.take(skip)?;
        }
        Ok(b)
    }
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
    fn bitpos(&self) -> u64 {
        self.pos as u64 * 8 - self.bc as u64
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
pub const SLACK: usize = 16;

/// How a block body ended.
enum Body {
    End(usize),  // end of block; output cursor
    Full(usize), // (growable output only) stopped at a symbol boundary for more room
}

/// Where a run stopped.
pub struct Out {
    pub o: usize,           // output cursor
    pub end_bitpos: u64,    // bit position of the next unread bit
    pub finished: bool,     // the final block ended (else: stopped at a block start >= stop)
    pub blocks: usize,      // blocks decoded
}

impl Decoder {
    /// Decode the DEFLATE stream starting at `inp[start]` into `out[..limit]` (`out` has at
    /// least 16 spare bytes past `limit`, unused). Returns (bytes written, end byte position).
    pub fn decode(&mut self, inp: &[u8], start: usize, out: &mut [u8], limit: usize) -> Result<(usize, usize), Err> {
        let mut b = Bits { inp, pos: start, bb: 0, bc: 0 };
        let mut o = 0usize;
        loop {
            let last = b.take(1)? == 1;
            match b.take(2)? {
                0 => o = self.stored(&mut b, out, o, limit)?,
                t => {
                    self.tables(&mut b, t)?;
                    match self.body::<u8>(&mut b, out, o, limit, false)? {
                        Body::End(n) => o = n,
                        Body::Full(_) => unreachable!(),
                    }
                }
            }
            if last {
                b.align();
                return Ok((o, b.byte_pos()));
            }
        }
    }

    /// Decode from bit position `bitpos` (a block start) into a growable buffer, appending at
    /// `o0` (`out[..o0]` is history: a known window, or window markers). Stops when the final
    /// block ends or when a block would start at or past `stop_bitpos`.
    pub fn run<T: Sym>(&mut self, inp: &[u8], bitpos: u64, out: &mut Vec<T>, o0: usize, stop_bitpos: u64) -> Result<Out, Err> {
        let mut b = Bits::at(inp, bitpos)?;
        let mut o = o0;
        let mut blocks = 0;
        loop {
            let bp = b.bitpos();
            if bp >= stop_bitpos {
                return Ok(Out { o, end_bitpos: bp, finished: false, blocks });
            }
            blocks += 1;
            let last = b.take(1)? == 1;
            let t = b.take(2)?;
            if t == 0 {
                if out.len() < o + 65536 + SLACK {
                    out.resize((out.len() * 2).max(o + 65536 + SLACK), T::default());
                }
                let lim = out.len() - SLACK;
                o = self.stored(&mut b, out, o, lim)?;
            } else {
                self.tables(&mut b, t)?;
                loop {
                    if out.len() < o + (1 << 20) + SLACK {
                        out.resize((out.len() * 2).max(o + (1 << 20) + SLACK), T::default());
                    }
                    let lim = out.len() - SLACK;
                    match self.body::<T>(&mut b, out, o, lim, true)? {
                        Body::End(n) => {
                            o = n;
                            break;
                        }
                        Body::Full(n) => o = n,
                    }
                }
            }
            if last {
                b.align();
                return Ok(Out { o, end_bitpos: b.bitpos(), finished: true, blocks });
            }
        }
    }

    /// Whether a dynamic block header parses at `bitpos` with complete codes (the block
    /// finder's full check). Builds this decoder's tables as a side effect.
    pub fn header_ok(&mut self, inp: &[u8], bitpos: u64) -> bool {
        let Ok(mut b) = Bits::at(inp, bitpos) else { return false };
        let (Ok(0), Ok(2)) = (b.take(1), b.take(2)) else { return false };
        self.dynamic_header_strict(&mut b).is_ok()
    }

    fn stored<T: Sym>(&mut self, b: &mut Bits<'_>, out: &mut [T], o: usize, limit: usize) -> Result<usize, Err> {
        b.align();
        let inp = b.inp;
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
        for (d, &x) in out[o..o + len].iter_mut().zip(src) {
            *d = T::lit(x);
        }
        *b = Bits { inp, pos: p + 4 + len, bb: 0, bc: 0 };
        Ok(o + len)
    }

    fn tables(&mut self, b: &mut Bits<'_>, t: u32) -> Result<(), Err> {
        match t {
            1 => {
                let mut l = [0u8; 320];
                l[..144].fill(8);
                l[144..256].fill(9);
                l[256..280].fill(7);
                l[280..288].fill(8);
                l[288..320].fill(5);
                build(&mut self.lit[..], &l[..288], LB, lit_entry)?;
                build(&mut self.dist[..], &l[288..320], DB, dist_entry)
            }
            2 => {
                #[cfg(feature = "census")]
                let t0 = std::time::Instant::now();
                let r = self.dynamic_header(b);
                #[cfg(feature = "census")]
                {
                    prof::add(&prof::HEADER_NS, t0.elapsed().as_nanos() as u64);
                    prof::add(&prof::BLOCKS, 1);
                }
                r
            }
            _ => Err("invalid block type"),
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
        // Code lengths: one refill serves 4 precode symbols (<= 7 + 7 bits each); repeats are
        // written as fixed-width chunks into slack past the end, then bounds are checked.
        let total = hlit + hdist;
        let mut l = [0u8; 320 + 160];
        let mut i = 0;
        'lens: while i < total {
            if b.bc < 56 {
                b.refill()?;
            }
            for _ in 0..4 {
                let e = pt[(b.bb & 127) as usize];
                if e & EXC != 0 {
                    return Err("invalid precode");
                }
                let sl = e & 0xff;
                b.bb >>= sl;
                b.bc -= sl;
                let sym = e >> 16;
                if sym < 16 {
                    l[i] = sym as u8;
                    i += 1;
                } else if sym == 16 {
                    if i == 0 {
                        return Err("repeat with no previous length");
                    }
                    let n = 3 + (b.bb & 3) as usize;
                    b.bb >>= 2;
                    b.bc -= 2;
                    let v = l[i - 1];
                    l[i..i + 8].copy_from_slice(&[v; 8]);
                    i += n;
                } else if sym == 17 {
                    let n = 3 + (b.bb & 7) as usize;
                    b.bb >>= 3;
                    b.bc -= 3;
                    l[i..i + 16].copy_from_slice(&[0; 16]);
                    i += n;
                } else {
                    let n = 11 + (b.bb & 127) as usize;
                    b.bb >>= 7;
                    b.bc -= 7;
                    l[i..i + 138].fill(0);
                    i += n;
                }
                if i >= total {
                    break 'lens;
                }
            }
        }
        if i > total {
            return Err("code lengths overflow");
        }
        if l[256] == 0 {
            return Err("no end-of-block code");
        }
        #[cfg(feature = "census")]
        let t0 = std::time::Instant::now();
        build(&mut self.lit[..], &l[..hlit], LB, lit_entry)?;
        build(&mut self.dist[..], &l[hlit..total], DB, dist_entry)?;
        #[cfg(feature = "census")]
        prof::add(&prof::BUILD_NS, t0.elapsed().as_nanos() as u64);
        Ok(())
    }

    fn dynamic_header_strict(&mut self, b: &mut Bits<'_>) -> Result<(), Err> {
        let hlit = b.take(5)? as usize + 257;
        let hdist = b.take(5)? as usize + 1;
        let hclen = b.take(4)? as usize + 4;
        if hlit > 286 || hdist > 30 {
            return Err("too many codes");
        }
        const ORD: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
        let mut pl = [0u8; 19];
        for &i in &ORD[..hclen] {
            pl[i] = b.take(3)? as u8;
        }
        // precode: 7-bit direct table, entry = len | sym << 16
        let mut pt = [INVALID; 128];
        build(&mut pt, &pl, 7, |s, l| ((s as u32) << 16) | l)?;
        // Code lengths: one refill serves 4 precode symbols (<= 7 + 7 bits each); repeats are
        // written as fixed-width chunks into slack past the end, then bounds are checked.
        let total = hlit + hdist;
        let mut l = [0u8; 320 + 160];
        let mut i = 0;
        'lens: while i < total {
            if b.bc < 56 {
                b.refill()?;
            }
            for _ in 0..4 {
                let e = pt[(b.bb & 127) as usize];
                if e & EXC != 0 {
                    return Err("invalid precode");
                }
                let sl = e & 0xff;
                b.bb >>= sl;
                b.bc -= sl;
                let sym = e >> 16;
                if sym < 16 {
                    l[i] = sym as u8;
                    i += 1;
                } else if sym == 16 {
                    if i == 0 {
                        return Err("repeat with no previous length");
                    }
                    let n = 3 + (b.bb & 3) as usize;
                    b.bb >>= 2;
                    b.bc -= 2;
                    let v = l[i - 1];
                    l[i..i + 8].copy_from_slice(&[v; 8]);
                    i += n;
                } else if sym == 17 {
                    let n = 3 + (b.bb & 7) as usize;
                    b.bb >>= 3;
                    b.bc -= 3;
                    l[i..i + 16].copy_from_slice(&[0; 16]);
                    i += n;
                } else {
                    let n = 11 + (b.bb & 127) as usize;
                    b.bb >>= 7;
                    b.bc -= 7;
                    l[i..i + 138].fill(0);
                    i += n;
                }
                if i >= total {
                    break 'lens;
                }
            }
        }
        if i > total {
            return Err("code lengths overflow");
        }
        if l[256] == 0 {
            return Err("no end-of-block code");
        }
        #[cfg(feature = "census")]
        let t0 = std::time::Instant::now();
        if !complete(&l[..hlit]) || !complete(&l[hlit..total]) {
            return Err("incomplete code");
        }
        build(&mut self.lit[..], &l[..hlit], LB, lit_entry)?;
        build(&mut self.dist[..], &l[hlit..total], DB, dist_entry)?;
        #[cfg(feature = "census")]
        prof::add(&prof::BUILD_NS, t0.elapsed().as_nanos() as u64);
        Ok(())
    }

    /// One block body: the fast loop while margins hold, then the checked loop.
    #[inline(never)]
    fn body<T: Sym>(&self, b: &mut Bits<'_>, out: &mut [T], mut o: usize, limit: usize, grow: bool) -> Result<Body, Err> {
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
                put!(out, o, T::lit(l));
                o += 1;
                if e & LIT != 0 {
                    let l = (e >> 16) as u8;
                    e = lit[(bb & LM) as usize];
                    saved = bb;
                    consume!(e);
                    put!(out, o, T::lit(l));
                    o += 1;
                    if e & LIT != 0 {
                        // a third literal, in lieu of the primary item
                        let l = (e >> 16) as u8;
                        e = lit[(bb & LM) as usize];
                        refill!();
                        put!(out, o, T::lit(l));
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
                    put!(out, o, T::lit(l));
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
            if dst >= T::PW {
                // libdeflate's shape: five words unconditionally, then five-word strides;
                // may write up to five words past `end`, inside the margin
                macro_rules! w5 {
                    () => {
                        let v = T::rdw(out, s);
                        T::wrw(out, t, v);
                        let v = T::rdw(out, s + T::PW);
                        T::wrw(out, t + T::PW, v);
                        let v = T::rdw(out, s + 2 * T::PW);
                        T::wrw(out, t + 2 * T::PW, v);
                        let v = T::rdw(out, s + 3 * T::PW);
                        T::wrw(out, t + 3 * T::PW, v);
                        let v = T::rdw(out, s + 4 * T::PW);
                        T::wrw(out, t + 4 * T::PW, v);
                        s += 5 * T::PW;
                        t += 5 * T::PW;
                    };
                }
                w5!();
                while t < end {
                    w5!();
                }
            } else if dst == 1 {
                let v = T::splat(get!(out, s));
                T::wrw(out, t, v);
                T::wrw(out, t + T::PW, v);
                T::wrw(out, t + 2 * T::PW, v);
                T::wrw(out, t + 3 * T::PW, v);
                t += 4 * T::PW;
                while t < end {
                    T::wrw(out, t, v);
                    t += T::PW;
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
            return Ok(Body::End(o));
        }

        // ---- checked loop: one symbol at a time, exact bounds ----
        loop {
            if grow && o + 258 > limit {
                // a growable buffer: stop at this symbol boundary and ask for room
                b.pos = pos;
                b.bb = bb;
                b.bc = bc;
                return Ok(Body::Full(o));
            }
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
                out[o] = T::lit((e >> 16) as u8);
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
        Ok(Body::End(o))
    }
}
