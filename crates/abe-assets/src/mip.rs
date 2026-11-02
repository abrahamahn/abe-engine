//! Complete mip chains for RGBA8 textures. Each level halves the previous one (rounding down)
//! with a box filter; for an odd source size the last texel of a row or column also takes the
//! leftover source texel, so nothing is dropped. sRGB colour is averaged in linear light and
//! normal maps are renormalized. Alpha is always averaged linearly.
use abe_core::world::{MAX_TEXTURE_SIZE, Texture};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    /// sRGB-encoded colour (base colour, emissive).
    Srgb,
    /// Linear data.
    Linear,
    /// Tangent-space normals encoded as `n * 0.5 + 0.5`.
    Normal,
}

pub fn srgb_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
pub fn linear_to_srgb(l: f32) -> u8 {
    let c = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1. / 2.4) - 0.055
    };
    unorm(c)
}
fn unorm(v: f32) -> u8 {
    (v.clamp(0., 1.) * 255. + 0.5) as u8
}

/// Source texels covered by destination texel `d` along one axis.
fn taps(d: usize, src: usize, dst: usize) -> std::ops::Range<usize> {
    2 * d..if d + 1 == dst { src } else { 2 * d + 2 }
}

fn downsample(
    src: &[u8],
    (sw, sh): (usize, usize),
    (dw, dh): (usize, usize),
    filter: Filter,
    lut: &[f32; 256],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(dw * dh * 4);
    for y in 0..dh {
        let ys = taps(y, sh, dh);
        for x in 0..dw {
            let xs = taps(x, sw, dw);
            let mut acc = [0f32; 4];
            for sy in ys.clone() {
                for p in src[(sy * sw + xs.start) * 4..(sy * sw + xs.end) * 4].chunks_exact(4) {
                    for c in 0..3 {
                        acc[c] += match filter {
                            Filter::Srgb => lut[usize::from(p[c])],
                            Filter::Linear => f32::from(p[c]) / 255.,
                            Filter::Normal => f32::from(p[c]) / 127.5 - 1.,
                        };
                    }
                    acc[3] += f32::from(p[3]) / 255.;
                }
            }
            let n = (ys.len() * xs.len()) as f32;
            let [r, g, b, a] = acc.map(|v| v / n);
            let rgb = match filter {
                Filter::Srgb => [r, g, b].map(linear_to_srgb),
                Filter::Linear => [r, g, b].map(unorm),
                Filter::Normal => {
                    let len = (r * r + g * g + b * b).sqrt();
                    let v = if len > 1e-6 {
                        [r / len, g / len, b / len]
                    } else {
                        [0., 0., 1.]
                    };
                    v.map(|c| unorm(c * 0.5 + 0.5))
                }
            };
            out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], unorm(a)]);
        }
    }
    out
}

/// Bytes in a complete chain, or `None` past the size limits.
pub fn chain_bytes(width: u32, height: u32) -> Option<u64> {
    let limit = 1..=MAX_TEXTURE_SIZE;
    if !limit.contains(&width) || !limit.contains(&height) {
        return None;
    }
    Some(
        (0..Texture::level_count(width, height))
            .map(|i| {
                let (w, h) = Texture::level_extent(width, height, i);
                u64::from(w) * u64::from(h) * 4
            })
            .sum(),
    )
}

/// Build a texture with a complete chain from its top level. sRGB filtering marks it sRGB.
pub fn texture(width: u32, height: u32, rgba: Vec<u8>, filter: Filter) -> Result<Texture, String> {
    if chain_bytes(width, height).is_none()
        || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4
    {
        return Err(format!("invalid {width}x{height} RGBA8 texture"));
    }
    let lut: [f32; 256] = std::array::from_fn(|v| srgb_to_linear(v as u8));
    let count = Texture::level_count(width, height);
    let mut levels = Vec::with_capacity(count);
    levels.push(rgba);
    for i in 1..count {
        let (sw, sh) = Texture::level_extent(width, height, i - 1);
        let (dw, dh) = Texture::level_extent(width, height, i);
        let next = downsample(
            &levels[i - 1],
            (sw as usize, sh as usize),
            (dw as usize, dh as usize),
            filter,
            &lut,
        );
        levels.push(next);
    }
    Ok(Texture {
        width,
        height,
        srgb: filter == Filter::Srgb,
        levels,
        encoded: None,
    })
}
