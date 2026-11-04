//! zlib streams (RFC 1950) with DEFLATE (RFC 1951): a bounded decoder for untrusted input and
//! an LZ77 + Huffman encoder.

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
/// Transmission order of the code length code lengths.
const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
const FAST_BITS: u32 = 10;

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    count: u32,
}
impl Bits<'_> {
    fn refill(&mut self) {
        while self.count <= 56 {
            let Some(&byte) = self.data.get(self.pos) else {
                break;
            };
            self.buf |= u64::from(byte) << self.count;
            self.count += 8;
            self.pos += 1;
        }
    }
    fn take(&mut self, n: u32) -> Result<u32, String> {
        if self.count < n {
            self.refill();
            if self.count < n {
                return Err("truncated deflate stream".into());
            }
        }
        let v = (self.buf & ((1u64 << n) - 1)) as u32;
        self.buf >>= n;
        self.count -= n;
        Ok(v)
    }
    /// Drop the partial byte and return buffered whole bytes to the input.
    fn align(&mut self) {
        self.pos -= (self.count / 8) as usize;
        self.buf = 0;
        self.count = 0;
    }
}

/// Canonical Huffman decoding: a direct table for short codes, then a bit-serial walk.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
    fast: Vec<u32>,
}
impl Huffman {
    /// Returns the code and the unused code space (0 for a complete code).
    fn new(lengths: &[u8]) -> Result<(Self, i32), String> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        let mut left = 1i32;
        for &c in &counts[1..] {
            left = (left << 1) - i32::from(c);
            if left < 0 {
                return Err("over-subscribed Huffman code".into());
            }
        }
        let mut offsets = [0u16; 16];
        for len in 1..15 {
            offsets[len + 1] = offsets[len] + counts[len];
        }
        let mut symbols = vec![0u16; lengths.len()];
        let mut next = [0u32; 16];
        let mut code = 0u32;
        for len in 1..16 {
            let shorter = if len > 1 { counts[len - 1] } else { 0 };
            code = (code + u32::from(shorter)) << 1;
            next[len] = code;
        }
        let mut fast = vec![0u32; 1 << FAST_BITS];
        for (symbol, &l) in lengths.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let len = l as usize;
            symbols[offsets[len] as usize] = symbol as u16;
            offsets[len] += 1;
            let c = next[len];
            next[len] += 1;
            if u32::from(l) <= FAST_BITS {
                let mut r = c.reverse_bits() >> (32 - u32::from(l));
                while r < 1 << FAST_BITS {
                    fast[r as usize] = (u32::from(l) << 16) | symbol as u32;
                    r += 1 << l;
                }
            }
        }
        Ok((
            Self {
                counts,
                symbols,
                fast,
            },
            left,
        ))
    }
    fn decode(&self, bits: &mut Bits) -> Result<u16, String> {
        if bits.count < 15 {
            bits.refill();
        }
        let entry = self.fast[(bits.buf & ((1 << FAST_BITS) - 1)) as usize];
        if entry != 0 {
            let len = entry >> 16;
            if len > bits.count {
                return Err("truncated deflate stream".into());
            }
            bits.buf >>= len;
            bits.count -= len;
            return Ok(entry as u16);
        }
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for &count in &self.counts[1..] {
            code |= bits.take(1)? as i32;
            let count = i32::from(count);
            if code - count < first {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("invalid Huffman code".into())
    }
}

/// A code may only be incomplete when it is a single one-bit code (or, for distances, empty).
fn usable(lengths: &[u8], left: i32) -> bool {
    left == 0 || lengths.iter().filter(|&&l| l > 1).count() == 0
}

fn fixed() -> Result<(Huffman, Huffman), String> {
    let mut lit = [8u8; 288];
    lit[144..256].fill(9);
    lit[256..280].fill(7);
    Ok((Huffman::new(&lit)?.0, Huffman::new(&[5u8; 30])?.0))
}

fn dynamic(bits: &mut Bits) -> Result<(Huffman, Huffman), String> {
    let nlit = bits.take(5)? as usize + 257;
    let ndist = bits.take(5)? as usize + 1;
    let ncode = bits.take(4)? as usize + 4;
    if nlit > 286 || ndist > 30 {
        return Err("invalid dynamic block code counts".into());
    }
    let mut cl = [0u8; 19];
    for &i in &ORDER[..ncode] {
        cl[i] = bits.take(3)? as u8;
    }
    let (clcode, left) = Huffman::new(&cl)?;
    if left != 0 {
        return Err("incomplete code length code".into());
    }
    let mut lengths = [0u8; 316];
    let mut i = 0;
    while i < nlit + ndist {
        let symbol = clcode.decode(bits)?;
        if symbol < 16 {
            lengths[i] = symbol as u8;
            i += 1;
            continue;
        }
        let (value, repeat) = match symbol {
            16 if i > 0 => (lengths[i - 1], 3 + bits.take(2)?),
            16 => return Err("repeated code length without a predecessor".into()),
            17 => (0, 3 + bits.take(3)?),
            _ => (0, 11 + bits.take(7)?),
        };
        let end = i + repeat as usize;
        if end > nlit + ndist {
            return Err("code lengths overrun the dynamic block header".into());
        }
        lengths[i..end].fill(value);
        i = end;
    }
    if lengths[256] == 0 {
        return Err("dynamic block has no end-of-block code".into());
    }
    let (lit, left) = Huffman::new(&lengths[..nlit])?;
    if !usable(&lengths[..nlit], left) {
        return Err("incomplete literal/length code".into());
    }
    let (dist, left) = Huffman::new(&lengths[nlit..nlit + ndist])?;
    if !usable(&lengths[nlit..nlit + ndist], left) {
        return Err("incomplete distance code".into());
    }
    Ok((lit, dist))
}

fn inflate_block(
    bits: &mut Bits,
    out: &mut Vec<u8>,
    limit: usize,
    (lit, dist): &(Huffman, Huffman),
) -> Result<(), String> {
    let overflow = || "decompressed data exceeds its expected size".to_string();
    loop {
        let symbol = lit.decode(bits)?;
        if symbol < 256 {
            if out.len() >= limit {
                return Err(overflow());
            }
            out.push(symbol as u8);
            continue;
        }
        if symbol == 256 {
            return Ok(());
        }
        let s = usize::from(symbol - 257);
        if s >= 29 {
            return Err("invalid length symbol".into());
        }
        let len = usize::from(LEN_BASE[s]) + bits.take(u32::from(LEN_EXTRA[s]))? as usize;
        let d = usize::from(dist.decode(bits)?);
        if d >= 30 {
            return Err("invalid distance symbol".into());
        }
        let distance = usize::from(DIST_BASE[d]) + bits.take(u32::from(DIST_EXTRA[d]))? as usize;
        if distance > out.len() {
            return Err("distance reaches before the start of the stream".into());
        }
        if out.len() + len > limit {
            return Err(overflow());
        }
        let start = out.len() - distance;
        if distance >= len {
            out.extend_from_within(start..start + len);
        } else {
            for k in 0..len {
                out.push(out[start + k]);
            }
        }
    }
}

/// Decompress a complete zlib stream that inflates to at most `limit` bytes. Allocation is
/// bounded by both `limit` and the input size, and the Adler-32 checksum must match.
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let [cmf, flg, ..] = *data else {
        return Err("truncated zlib header".into());
    };
    if cmf & 0x0f != 8 || cmf >> 4 > 7 || (u16::from(cmf) << 8 | u16::from(flg)) % 31 != 0 {
        return Err("invalid zlib header".into());
    }
    if flg & 0x20 != 0 {
        return Err("zlib preset dictionaries are unsupported".into());
    }
    let mut out = Vec::with_capacity(limit.min(data.len().saturating_mul(4)));
    let mut bits = Bits {
        data: &data[2..],
        pos: 0,
        buf: 0,
        count: 0,
    };
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => {
                bits.align();
                let header = bits
                    .data
                    .get(bits.pos..bits.pos + 4)
                    .ok_or("truncated stored block")?;
                let len = usize::from(u16::from_le_bytes([header[0], header[1]]));
                if u16::from_le_bytes([header[2], header[3]]) != !(len as u16) {
                    return Err("stored block length check failed".into());
                }
                bits.pos += 4;
                let raw = bits
                    .data
                    .get(bits.pos..bits.pos + len)
                    .ok_or("truncated stored block")?;
                if out.len() + len > limit {
                    return Err("decompressed data exceeds its expected size".into());
                }
                out.extend_from_slice(raw);
                bits.pos += len;
            }
            1 => inflate_block(&mut bits, &mut out, limit, &fixed()?)?,
            2 => {
                let codes = dynamic(&mut bits)?;
                inflate_block(&mut bits, &mut out, limit, &codes)?
            }
            _ => return Err("invalid deflate block type".into()),
        }
        if last {
            break;
        }
    }
    bits.align();
    match bits.data.get(bits.pos..) {
        Some(trailer) if trailer.len() == 4 => {
            if u32::from_be_bytes(trailer.try_into().expect("four bytes")) != adler32(&out) {
                return Err("Adler-32 checksum mismatch".into());
            }
            Ok(out)
        }
        Some(trailer) if trailer.len() > 4 => Err("data after the zlib stream".into()),
        _ => Err("missing Adler-32 checksum".into()),
    }
}

// Compression.

const WINDOW: usize = 1 << 15;
const HASH_BITS: u32 = 15;
const NONE: u32 = u32::MAX;
const MAX_CHAIN: usize = 96;
const NICE_LENGTH: usize = 128;
const LAZY_LENGTH: usize = 32;
const BLOCK_TOKENS: usize = 1 << 15;

#[derive(Clone, Copy)]
struct Token {
    /// Literal byte when `dist` is 0, otherwise match length.
    len: u16,
    dist: u16,
}

struct Writer {
    out: Vec<u8>,
    buf: u64,
    count: u32,
}
impl Writer {
    fn put(&mut self, value: u32, n: u32) {
        self.buf |= u64::from(value) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.count -= 8;
        }
    }
    /// Huffman codes are sent most significant bit first.
    fn code(&mut self, (code, len): (u16, u8)) {
        self.put(
            u32::from(code).reverse_bits() >> (32 - u32::from(len)),
            u32::from(len),
        );
    }
    fn align(&mut self) {
        if self.count > 0 {
            self.out.push(self.buf as u8);
        }
        self.buf = 0;
        self.count = 0;
    }
}

fn hash(d: &[u8], i: usize) -> usize {
    let v = u32::from(d[i]) << 16 | u32::from(d[i + 1]) << 8 | u32::from(d[i + 2]);
    (v.wrapping_mul(2_654_435_761) >> (32 - HASH_BITS)) as usize
}

struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
}
impl Matcher<'_> {
    fn insert(&mut self, i: usize) {
        if i + 3 <= self.data.len() {
            let h = hash(self.data, i);
            self.prev[i % WINDOW] = self.head[h];
            self.head[h] = i as u32;
        }
    }
    /// Longest earlier match at `i` within the window, searched before `i` is inserted.
    fn find(&self, i: usize) -> (usize, usize) {
        let d = self.data;
        let max = (d.len() - i).min(258);
        if max < 3 {
            return (0, 0);
        }
        let (mut best, mut dist) = (2, 0);
        let mut c = self.head[hash(d, i)];
        for _ in 0..MAX_CHAIN {
            if c == NONE || i - c as usize >= WINDOW {
                break;
            }
            let c0 = c as usize;
            if d[c0 + best] == d[i + best] && d[c0] == d[i] && d[c0 + 1] == d[i + 1] {
                let len = (0..max).find(|&k| d[c0 + k] != d[i + k]).unwrap_or(max);
                if len > best {
                    (best, dist) = (len, i - c0);
                    if len >= NICE_LENGTH.min(max) {
                        break;
                    }
                }
            }
            let next = self.prev[c0 % WINDOW];
            if next == NONE || next >= c {
                break;
            }
            c = next;
        }
        if best >= 3 { (best, dist) } else { (0, 0) }
    }
}

fn tokens(data: &[u8]) -> Vec<Token> {
    let mut m = Matcher {
        data,
        head: vec![NONE; 1 << HASH_BITS],
        prev: vec![NONE; WINDOW],
    };
    let mut out = Vec::with_capacity(data.len() / 2);
    let literal = |b: u8| Token {
        len: u16::from(b),
        dist: 0,
    };
    // One-step lazy evaluation: a match is deferred when the next position has a longer one.
    let (mut pending, mut prev) = (false, (0, 0));
    let mut i = 0;
    while i < data.len() {
        let cur = if prev.0 >= LAZY_LENGTH {
            (0, 0)
        } else {
            m.find(i)
        };
        m.insert(i);
        if prev.0 >= 3 && cur.0 <= prev.0 {
            out.push(Token {
                len: prev.0 as u16,
                dist: prev.1 as u16,
            });
            let end = i - 1 + prev.0;
            for j in i + 1..end {
                m.insert(j);
            }
            i = end;
            pending = false;
            prev = (0, 0);
        } else {
            if pending {
                out.push(literal(data[i - 1]));
            }
            pending = true;
            prev = cur;
            i += 1;
        }
    }
    if pending {
        out.push(literal(data[data.len() - 1]));
    }
    out
}

/// Huffman code lengths no longer than `max`: frequencies are flattened until the tree fits.
fn code_lengths(freqs: &[u32], max: u8) -> Vec<u8> {
    let mut f: Vec<u64> = freqs.iter().map(|&x| u64::from(x)).collect();
    loop {
        let used: Vec<usize> = (0..f.len()).filter(|&s| f[s] > 0).collect();
        let mut lengths = vec![0u8; f.len()];
        if used.len() == 1 {
            lengths[used[0]] = 1;
            return lengths;
        }
        // Nodes: leaves first, then internal nodes in creation order.
        let mut parent = vec![usize::MAX; 2 * used.len()];
        let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(u64, usize)>> = used
            .iter()
            .enumerate()
            .map(|(k, &s)| std::cmp::Reverse((f[s], k)))
            .collect();
        let mut next = used.len();
        while heap.len() > 1 {
            let std::cmp::Reverse((wa, a)) = heap.pop().expect("two nodes");
            let std::cmp::Reverse((wb, b)) = heap.pop().expect("two nodes");
            parent[a] = next;
            parent[b] = next;
            heap.push(std::cmp::Reverse((wa + wb, next)));
            next += 1;
        }
        let mut depth = vec![0u8; next];
        for node in (0..next.saturating_sub(1)).rev() {
            depth[node] = depth[parent[node]].saturating_add(1);
        }
        for (k, &s) in used.iter().enumerate() {
            lengths[s] = depth[k];
        }
        if lengths.iter().all(|&l| l <= max) {
            return lengths;
        }
        for x in &mut f {
            if *x > 0 {
                *x = (*x >> 1).max(1);
            }
        }
    }
}

fn canonical(lengths: &[u8]) -> Vec<(u16, u8)> {
    let mut counts = [0u16; 16];
    for &l in lengths {
        counts[l as usize] += 1;
    }
    counts[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for len in 1..16 {
        code = (code + counts[len - 1]) << 1;
        next[len] = code;
    }
    lengths
        .iter()
        .map(|&l| {
            let c = next[l as usize];
            next[l as usize] += 1;
            (c, l)
        })
        .collect()
}

/// Run-length coded code lengths: (symbol, extra bits value, extra bit count).
fn rle(lengths: &[u8]) -> Vec<(u8, u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lengths.len() {
        let v = lengths[i];
        let mut run = lengths[i..].iter().take_while(|&&x| x == v).count();
        i += run;
        if v == 0 {
            while run >= 11 {
                let r = run.min(138);
                out.push((18, (r - 11) as u8, 7));
                run -= r;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u8, 3));
                run = 0;
            }
        } else {
            out.push((v, 0, 0));
            run -= 1;
            while run >= 3 {
                let r = run.min(6);
                out.push((16, (r - 3) as u8, 2));
                run -= r;
            }
        }
        out.extend(std::iter::repeat_n((v, 0, 0), run));
    }
    out
}

/// At least two used symbols make every Huffman code complete.
fn ensure_two(freqs: &mut [u32]) {
    let mut used = freqs.iter().filter(|&&x| x > 0).count();
    for f in freqs.iter_mut() {
        if used >= 2 {
            break;
        }
        if *f == 0 {
            *f = 1;
            used += 1;
        }
    }
}

fn len_symbol(len: u16) -> usize {
    LEN_BASE.partition_point(|&b| b <= len) - 1
}
fn dist_symbol(dist: u16) -> usize {
    DIST_BASE.partition_point(|&b| b <= dist) - 1
}

fn write_tokens(w: &mut Writer, tokens: &[Token], lit: &[(u16, u8)], dist: &[(u16, u8)]) {
    for t in tokens {
        if t.dist == 0 {
            w.code(lit[usize::from(t.len)]);
            continue;
        }
        let s = len_symbol(t.len);
        w.code(lit[257 + s]);
        w.put(u32::from(t.len - LEN_BASE[s]), u32::from(LEN_EXTRA[s]));
        let d = dist_symbol(t.dist);
        w.code(dist[d]);
        w.put(u32::from(t.dist - DIST_BASE[d]), u32::from(DIST_EXTRA[d]));
    }
    w.code(lit[256]);
}

/// Emit one block as stored, fixed or dynamic Huffman, whichever is smallest.
fn write_block(w: &mut Writer, tokens: &[Token], raw: &[u8], last: bool) {
    let mut lf = [0u32; 286];
    let mut df = [0u32; 30];
    let mut extra = 0u64;
    for t in tokens {
        if t.dist == 0 {
            lf[usize::from(t.len)] += 1;
        } else {
            let (s, d) = (len_symbol(t.len), dist_symbol(t.dist));
            lf[257 + s] += 1;
            df[d] += 1;
            extra += u64::from(LEN_EXTRA[s] + DIST_EXTRA[d]);
        }
    }
    lf[256] = 1;
    let cost = |f: &[u32], l: &[u8]| -> u64 {
        f.iter()
            .zip(l)
            .map(|(&f, &l)| u64::from(f) * u64::from(l))
            .sum()
    };
    let mut fixed_lit = [8u8; 288];
    fixed_lit[144..256].fill(9);
    fixed_lit[256..280].fill(7);
    let fixed_cost = 3 + cost(&lf, &fixed_lit) + cost(&df, &[5; 30]) + extra;

    let (mut lf2, mut df2) = (lf, df);
    ensure_two(&mut lf2);
    ensure_two(&mut df2);
    let ll = code_lengths(&lf2, 15);
    let dl = code_lengths(&df2, 15);
    let nlit = ll
        .iter()
        .rposition(|&l| l != 0)
        .map_or(257, |p| (p + 1).max(257));
    let ndist = dl.iter().rposition(|&l| l != 0).map_or(1, |p| p + 1);
    let runs = rle(&[&ll[..nlit], &dl[..ndist]].concat());
    let mut cf = [0u32; 19];
    for &(sym, _, _) in &runs {
        cf[usize::from(sym)] += 1;
    }
    ensure_two(&mut cf);
    let cl = code_lengths(&cf, 7);
    let ncl = ORDER
        .iter()
        .rposition(|&i| cl[i] != 0)
        .map_or(4, |p| (p + 1).max(4));
    let dynamic_cost = 17
        + 3 * ncl as u64
        + cost(&cf, &cl)
        + runs.iter().map(|r| u64::from(r.2)).sum::<u64>()
        + cost(&lf, &ll)
        + cost(&df, &dl)
        + extra;
    let stored_cost = (raw.len() as u64 + 5 * (raw.len() as u64 / 65535 + 1)) * 8 + 10;

    if stored_cost <= fixed_cost.min(dynamic_cost) {
        let mut chunks = raw.chunks(65535).peekable();
        if chunks.peek().is_none() {
            w.put(u32::from(last), 3);
            w.align();
            w.out.extend_from_slice(&[0, 0, 0xff, 0xff]);
        }
        while let Some(chunk) = chunks.next() {
            w.put(u32::from(last && chunks.peek().is_none()), 3);
            w.align();
            let n = chunk.len() as u16;
            w.out.extend_from_slice(&n.to_le_bytes());
            w.out.extend_from_slice(&(!n).to_le_bytes());
            w.out.extend_from_slice(chunk);
        }
    } else if fixed_cost <= dynamic_cost {
        w.put(u32::from(last) | 2, 3);
        write_tokens(w, tokens, &canonical(&fixed_lit), &canonical(&[5; 30]));
    } else {
        w.put(u32::from(last) | 4, 3);
        w.put(nlit as u32 - 257, 5);
        w.put(ndist as u32 - 1, 5);
        w.put(ncl as u32 - 4, 4);
        for &i in &ORDER[..ncl] {
            w.put(u32::from(cl[i]), 3);
        }
        let cc = canonical(&cl);
        for &(sym, value, bits) in &runs {
            w.code(cc[usize::from(sym)]);
            w.put(u32::from(value), u32::from(bits));
        }
        write_tokens(w, tokens, &canonical(&ll), &canonical(&dl));
    }
}

/// Compress `data` into a zlib stream with a 32 KiB window.
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut w = Writer {
        out: vec![0x78, 0x9c],
        buf: 0,
        count: 0,
    };
    let all = tokens(data);
    let blocks = all.len().div_ceil(BLOCK_TOKENS).max(1);
    let mut pos = 0;
    for k in 0..blocks {
        let chunk =
            &all[(k * BLOCK_TOKENS).min(all.len())..((k + 1) * BLOCK_TOKENS).min(all.len())];
        let span: usize = chunk
            .iter()
            .map(|t| if t.dist == 0 { 1 } else { usize::from(t.len) })
            .sum();
        write_block(&mut w, chunk, &data[pos..pos + span], k + 1 == blocks);
        pos += span;
    }
    w.align();
    w.out.extend_from_slice(&adler32(data).to_be_bytes());
    w.out
}
