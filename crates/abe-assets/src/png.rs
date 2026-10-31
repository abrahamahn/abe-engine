//! PNG decoding to RGBA8 for untrusted input, and lossless RGBA8 encoding. Decoding accepts
//! non-interlaced greyscale, RGB, paletted, grey-alpha and RGBA images at their standard bit
//! depths (16-bit samples are rounded to 8). Every CRC and the zlib Adler-32 are checked.
use crate::zlib;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
pub const MAX_DIMENSION: u32 = 16_384;
/// 128 MiB of RGBA8 output.
pub const MAX_PIXELS: u64 = 1 << 25;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};
pub fn crc32(parts: &[&[u8]]) -> u32 {
    let mut c = !0u32;
    for part in parts {
        for &b in *part {
            c = CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
        }
    }
    !c
}

#[derive(Clone, Copy)]
struct Header {
    width: u32,
    height: u32,
    depth: u8,
    color: u8,
}
impl Header {
    fn parse(b: &[u8]) -> Result<Self, String> {
        let &[
            w0,
            w1,
            w2,
            w3,
            h0,
            h1,
            h2,
            h3,
            depth,
            color,
            compression,
            filter,
            interlace,
        ] = b
        else {
            return Err("invalid IHDR length".into());
        };
        let (width, height) = (
            u32::from_be_bytes([w0, w1, w2, w3]),
            u32::from_be_bytes([h0, h1, h2, h3]),
        );
        if compression != 0 || filter != 0 {
            return Err("unsupported PNG compression or filter method".into());
        }
        match interlace {
            0 => {}
            1 => {
                return Err(
                    "interlaced (Adam7) PNGs are unsupported; save without interlacing".into(),
                );
            }
            _ => return Err("invalid PNG interlace method".into()),
        }
        if width == 0
            || height == 0
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
            || u64::from(width) * u64::from(height) > MAX_PIXELS
        {
            return Err(format!("PNG size {width}x{height} outside limits"));
        }
        let valid = match color {
            0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
            3 => matches!(depth, 1 | 2 | 4 | 8),
            2 | 4 | 6 => matches!(depth, 8 | 16),
            _ => false,
        };
        if !valid {
            return Err(format!(
                "invalid PNG colour type {color} at bit depth {depth}"
            ));
        }
        Ok(Self {
            width,
            height,
            depth,
            color,
        })
    }
    fn channels(self) -> usize {
        match self.color {
            2 => 3,
            4 => 2,
            6 => 4,
            _ => 1,
        }
    }
    fn stride(self) -> usize {
        (self.width as usize * self.channels() * usize::from(self.depth)).div_ceil(8)
    }
    /// Bytes per complete pixel for filtering, at least one.
    fn bpp(self) -> usize {
        (self.channels() * usize::from(self.depth)).div_ceil(8)
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Reverse the per-row filters in place; rows are a filter byte followed by `stride` bytes.
fn unfilter(raw: &mut [u8], stride: usize, bpp: usize) -> Result<(), String> {
    let row = stride + 1;
    for y in 0..raw.len() / row {
        let (done, rest) = raw.split_at_mut(y * row);
        let prev = (y > 0).then(|| &done[done.len() - stride..]);
        let (filter, line) = rest[..row].split_first_mut().expect("nonempty row");
        let up = |i: usize| prev.map_or(0, |p| p[i]);
        match *filter {
            0 => {}
            1 => {
                for i in bpp..stride {
                    line[i] = line[i].wrapping_add(line[i - bpp]);
                }
            }
            2 => {
                for (i, v) in line.iter_mut().enumerate() {
                    *v = v.wrapping_add(up(i));
                }
            }
            3 => {
                for i in 0..stride {
                    let left = if i >= bpp { line[i - bpp] } else { 0 };
                    line[i] =
                        line[i].wrapping_add(((u16::from(left) + u16::from(up(i))) / 2) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let (left, corner) = if i >= bpp {
                        (line[i - bpp], up(i - bpp))
                    } else {
                        (0, 0)
                    };
                    line[i] = line[i].wrapping_add(paeth(left, up(i), corner));
                }
            }
            f => return Err(format!("invalid PNG filter type {f}")),
        }
    }
    Ok(())
}

fn sample(line: &[u8], k: usize, depth: u8) -> u16 {
    match depth {
        8 => u16::from(line[k]),
        16 => u16::from_be_bytes([line[2 * k], line[2 * k + 1]]),
        d => {
            let bit = k * usize::from(d);
            let shift = 8 - usize::from(d) - bit % 8;
            u16::from((line[bit / 8] >> shift) & ((1u8 << d) - 1))
        }
    }
}
fn to8(v: u16, depth: u8) -> u8 {
    match depth {
        16 => ((u32::from(v) + 128) / 257) as u8,
        8 => v as u8,
        d => (u32::from(v) * 255 / ((1 << d) - 1)) as u8,
    }
}

fn expand(h: Header, raw: &[u8], palette: &[u8], trns: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let (w, stride, d) = (h.width as usize, h.stride(), h.depth);
    let mut out = Vec::with_capacity(w * h.height as usize * 4);
    let key = |i: usize| trns.map(|t| u16::from_be_bytes([t[2 * i], t[2 * i + 1]]));
    for line in raw.chunks_exact(stride + 1).map(|r| &r[1..]) {
        match h.color {
            0 => {
                let k = key(0);
                for x in 0..w {
                    let g = sample(line, x, d);
                    let v = to8(g, d);
                    out.extend_from_slice(&[v, v, v, if k == Some(g) { 0 } else { 255 }]);
                }
            }
            2 if d == 8 && trns.is_none() => {
                for p in line.chunks_exact(3) {
                    out.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            }
            2 => {
                let k = trns.map(|_| [key(0), key(1), key(2)]);
                for x in 0..w {
                    let s = [0, 1, 2].map(|c| sample(line, 3 * x + c, d));
                    let a = if k == Some(s.map(Some)) { 0 } else { 255 };
                    out.extend_from_slice(&[to8(s[0], d), to8(s[1], d), to8(s[2], d), a]);
                }
            }
            3 => {
                for x in 0..w {
                    let i = usize::from(sample(line, x, d));
                    let rgb = palette
                        .get(3 * i..3 * i + 3)
                        .ok_or("PNG palette index out of range")?;
                    let a = trns.and_then(|t| t.get(i).copied()).unwrap_or(255);
                    out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
                }
            }
            4 => {
                for x in 0..w {
                    let v = to8(sample(line, 2 * x, d), d);
                    out.extend_from_slice(&[v, v, v, to8(sample(line, 2 * x + 1, d), d)]);
                }
            }
            _ if d == 8 => out.extend_from_slice(&line[..4 * w]),
            _ => {
                for x in 0..w {
                    let s = [0, 1, 2, 3].map(|c| to8(sample(line, 4 * x + c, d), d));
                    out.extend_from_slice(&s);
                }
            }
        }
    }
    Ok(out)
}

pub fn decode(data: &[u8]) -> Result<Image, String> {
    let mut rest = data.strip_prefix(&SIGNATURE).ok_or("not a PNG file")?;
    let mut header: Option<Header> = None;
    let mut palette: &[u8] = &[];
    let mut trns: Option<&[u8]> = None;
    let mut idat = Vec::new();
    // 0: before image data, 1: inside the IDAT run, 2: after it.
    let mut stage = 0;
    loop {
        if rest.len() < 12 {
            return Err("truncated PNG chunk".into());
        }
        let len = u32::from_be_bytes(rest[..4].try_into().expect("four bytes")) as usize;
        let kind: [u8; 4] = rest[4..8].try_into().expect("four bytes");
        if len > 0x7fff_ffff || rest.len() - 12 < len {
            return Err("truncated PNG chunk".into());
        }
        let body = &rest[8..8 + len];
        let crc = u32::from_be_bytes(rest[8 + len..12 + len].try_into().expect("four bytes"));
        rest = &rest[12 + len..];
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err("invalid PNG chunk type".into());
        }
        let name = String::from_utf8_lossy(&kind).into_owned();
        if crc32(&[&kind, body]) != crc {
            return Err(format!("CRC mismatch in PNG {name} chunk"));
        }
        if stage == 1 && &kind != b"IDAT" {
            stage = 2;
        }
        let Some(h) = header else {
            if &kind != b"IHDR" {
                return Err("first PNG chunk must be IHDR".into());
            }
            header = Some(Header::parse(body)?);
            continue;
        };
        match &kind {
            b"IHDR" => return Err("duplicate IHDR chunk".into()),
            b"PLTE" => {
                if !palette.is_empty() || trns.is_some() || stage != 0 {
                    return Err("misplaced or repeated PLTE chunk".into());
                }
                if h.color == 0 || h.color == 4 {
                    return Err("PLTE chunk in a greyscale PNG".into());
                }
                if len == 0 || len % 3 != 0 || len > 768 {
                    return Err("invalid PLTE length".into());
                }
                palette = body;
            }
            b"tRNS" => {
                let valid = match h.color {
                    0 => len == 2,
                    2 => len == 6,
                    3 => !palette.is_empty() && len <= palette.len() / 3,
                    _ => return Err("tRNS chunk in a PNG with an alpha channel".into()),
                };
                if !valid || trns.is_some() || stage != 0 {
                    return Err("invalid tRNS chunk".into());
                }
                trns = Some(body);
            }
            b"IDAT" => {
                if stage == 2 {
                    return Err("PNG IDAT chunks are not consecutive".into());
                }
                if h.color == 3 && palette.is_empty() {
                    return Err("paletted PNG without a PLTE chunk".into());
                }
                stage = 1;
                idat.extend_from_slice(body);
            }
            b"IEND" => {
                if len != 0 || !rest.is_empty() {
                    return Err("invalid IEND or data after it".into());
                }
                break;
            }
            _ if kind[0].is_ascii_uppercase() => {
                return Err(format!("unsupported critical PNG chunk {name}"));
            }
            _ => {}
        }
    }
    let h = header.ok_or("missing IHDR")?;
    if stage == 0 {
        return Err("PNG has no image data".into());
    }
    let expected = (h.stride() + 1) * h.height as usize;
    let mut raw = zlib::decompress(&idat, expected)?;
    if raw.len() != expected {
        return Err("PNG image data is shorter than its size".into());
    }
    unfilter(&mut raw, h.stride(), h.bpp())?;
    Ok(Image {
        width: h.width,
        height: h.height,
        rgba: expand(h, &raw, palette, trns)?,
    })
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&crc32(&[kind, body]).to_be_bytes());
}

/// Choose the row filter with the smallest sum of absolute signed bytes.
fn filter_rows(rows: &[u8], stride: usize, bpp: usize, adaptive: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(rows.len() + rows.len() / stride.max(1));
    let zero = vec![0u8; stride];
    let mut candidate = vec![0u8; stride];
    let mut best = vec![0u8; stride];
    for (y, line) in rows.chunks_exact(stride).enumerate() {
        let up = if y == 0 {
            &zero[..]
        } else {
            &rows[(y - 1) * stride..y * stride]
        };
        let mut chosen = (u64::MAX, 0u8);
        for f in 0..if adaptive { 5 } else { 1 } {
            for i in 0..stride {
                let left = if i >= bpp { line[i - bpp] } else { 0 };
                let corner = if i >= bpp { up[i - bpp] } else { 0 };
                let predicted = match f {
                    0 => 0,
                    1 => left,
                    2 => up[i],
                    3 => ((u16::from(left) + u16::from(up[i])) / 2) as u8,
                    _ => paeth(left, up[i], corner),
                };
                candidate[i] = line[i].wrapping_sub(predicted);
            }
            let score = candidate
                .iter()
                .map(|&v| u64::from((v as i8).unsigned_abs()))
                .sum::<u64>();
            if score < chosen.0 {
                chosen = (score, f);
                best.copy_from_slice(&candidate);
            }
        }
        out.push(chosen.1);
        out.extend_from_slice(&best);
    }
    out
}

/// Encode RGBA8 losslessly, choosing greyscale, paletted (1, 2, 4 or 8 bits), grey-alpha,
/// RGB or RGBA, whichever represents the pixels exactly in the fewest bytes.
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(format!("PNG size {width}x{height} outside limits"));
    }
    let (w, h) = (width as usize, height as usize);
    if rgba.len() != w * h * 4 {
        return Err("RGBA buffer does not match the image size".into());
    }
    let pixels = || rgba.chunks_exact(4);
    let grey = pixels().all(|p| p[0] == p[1] && p[1] == p[2]);
    let opaque = pixels().all(|p| p[3] == 255);
    // Up to 256 distinct colours in first-use order; translucent entries lead so tRNS is short.
    let mut seen = std::collections::HashSet::new();
    let mut colours: Vec<[u8; 4]> = Vec::new();
    for p in pixels() {
        let c: [u8; 4] = p.try_into().expect("four channels");
        if seen.insert(c) {
            colours.push(c);
            if colours.len() > 256 {
                break;
            }
        }
    }
    let paletted = !(grey && opaque) && colours.len() <= 256;
    let (color, depth, channels) = if grey && opaque {
        (0u8, 8u8, 1)
    } else if paletted {
        let depth = match colours.len() {
            0..=2 => 1,
            3..=4 => 2,
            5..=16 => 4,
            _ => 8,
        };
        (3, depth, 1)
    } else if grey {
        (4, 8, 2)
    } else if opaque {
        (2, 8, 3)
    } else {
        (6, 8, 4)
    };
    let mut out = SIGNATURE.to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[depth, color, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    let stride = (w * channels * usize::from(depth)).div_ceil(8);
    let mut rows = Vec::with_capacity(stride * h);
    if paletted {
        colours.sort_by_key(|c| c[3] == 255);
        let index: std::collections::HashMap<[u8; 4], u8> = colours
            .iter()
            .enumerate()
            .map(|(i, c)| (*c, i as u8))
            .collect();
        let plte: Vec<u8> = colours.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
        chunk(&mut out, b"PLTE", &plte);
        let translucent = colours.iter().take_while(|c| c[3] != 255).count();
        if translucent > 0 {
            let alpha: Vec<u8> = colours[..translucent].iter().map(|c| c[3]).collect();
            chunk(&mut out, b"tRNS", &alpha);
        }
        let per_byte = 8 / usize::from(depth);
        for line in rgba.chunks_exact(w * 4) {
            let mut packed = vec![0u8; stride];
            for (x, p) in line.chunks_exact(4).enumerate() {
                let i = index[<&[u8; 4]>::try_from(p).expect("four channels")];
                let shift = 8 - usize::from(depth) * (x % per_byte + 1);
                packed[x / per_byte] |= i << shift;
            }
            rows.extend_from_slice(&packed);
        }
    } else {
        for p in pixels() {
            match color {
                0 => rows.push(p[0]),
                4 => rows.extend_from_slice(&[p[0], p[3]]),
                2 => rows.extend_from_slice(&p[..3]),
                _ => rows.extend_from_slice(p),
            }
        }
    }
    let filtered = filter_rows(&rows, stride, channels.max(1), !paletted);
    chunk(&mut out, b"IDAT", &zlib::compress(&filtered));
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}
