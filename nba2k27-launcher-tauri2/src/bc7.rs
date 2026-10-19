//! BC7 (DXGI 97/98/99) and BC1/BC3 block decoding to RGBA8 for the clothing thumbnail
//! cache. WPF converted the same DDS files with texconv.exe
//! (-ft png -f R8G8B8A8_UNORM); this is the Khronos/Microsoft BC7 decoding
//! process, verified pixel-for-pixel against that texconv output.
use anyhow::{Result, ensure};

const PARTITION2: [u16; 64] = [
    0xCCCC, 0x8888, 0xEEEE, 0xECC8, 0xC880, 0xFEEC, 0xFEC8, 0xEC80, 0xC800, 0xFFEC, 0xFE80, 0xE800, 0xFFE8, 0xFF00,
    0xFFF0, 0xF000, 0xF710, 0x008E, 0x7100, 0x08CE, 0x008C, 0x7310, 0x3100, 0x8CCE, 0x088C, 0x3110, 0x6666, 0x366C,
    0x17E8, 0x0FF0, 0x718E, 0x399C, 0xAAAA, 0xF0F0, 0x5A5A, 0x33CC, 0x3C3C, 0x55AA, 0x9696, 0xA55A, 0x73CE, 0x13C8,
    0x324C, 0x3BDC, 0x6996, 0xC33C, 0x9966, 0x0660, 0x0272, 0x04E4, 0x4E40, 0x2720, 0xC936, 0x936C, 0x39C6, 0x639C,
    0x9336, 0x9CC6, 0x817E, 0xE718, 0xCCF0, 0x0FCC, 0x7744, 0xEE22,
];
const PARTITION3: [[u8; 16]; 64] = [
    [0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 1, 2, 2, 2, 2],
    [0, 0, 0, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 2, 1],
    [0, 0, 0, 0, 2, 0, 0, 1, 2, 2, 1, 1, 2, 2, 1, 1],
    [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 1, 0, 1, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2],
    [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 2, 2],
    [0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1],
    [0, 0, 1, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2],
    [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
    [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2],
    [0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2],
    [0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2],
    [0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2, 1, 2, 2, 2],
    [0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0, 2, 2, 2, 0],
    [0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 2, 1, 1, 2, 2],
    [0, 1, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 2, 2, 0, 0],
    [0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2],
    [0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 1, 1, 1, 1],
    [0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2, 0, 2, 2, 2],
    [0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 2, 1, 2, 2, 2, 1],
    [0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2],
    [0, 0, 0, 0, 1, 1, 0, 0, 2, 2, 1, 0, 2, 2, 1, 0],
    [0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0],
    [0, 0, 1, 2, 0, 0, 1, 2, 1, 1, 2, 2, 2, 2, 2, 2],
    [0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0],
    [0, 0, 0, 0, 0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1],
    [0, 0, 2, 2, 1, 1, 0, 2, 1, 1, 0, 2, 0, 0, 2, 2],
    [0, 1, 1, 0, 0, 1, 1, 0, 2, 0, 0, 2, 2, 2, 2, 2],
    [0, 0, 1, 1, 0, 1, 2, 2, 0, 1, 2, 2, 0, 0, 1, 1],
    [0, 0, 0, 0, 2, 0, 0, 0, 2, 2, 1, 1, 2, 2, 2, 1],
    [0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 2, 2, 2],
    [0, 2, 2, 2, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1],
    [0, 0, 1, 1, 0, 0, 1, 2, 0, 0, 2, 2, 0, 2, 2, 2],
    [0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0],
    [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0],
    [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0],
    [0, 1, 2, 0, 2, 0, 1, 2, 1, 2, 0, 1, 0, 1, 2, 0],
    [0, 0, 1, 1, 2, 2, 0, 0, 1, 1, 2, 2, 0, 0, 1, 1],
    [0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 1, 1],
    [0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1],
    [0, 0, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2, 1, 1, 2, 2],
    [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 2, 2, 0, 0, 1, 1],
    [0, 2, 2, 0, 1, 2, 2, 1, 0, 2, 2, 0, 1, 2, 2, 1],
    [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 0, 1, 0, 1],
    [0, 0, 0, 0, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1],
    [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2],
    [0, 2, 2, 2, 0, 1, 1, 1, 0, 2, 2, 2, 0, 1, 1, 1],
    [0, 0, 0, 2, 1, 1, 1, 2, 0, 0, 0, 2, 1, 1, 1, 2],
    [0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2, 2, 1, 1, 2],
    [0, 2, 2, 2, 0, 1, 1, 1, 0, 1, 1, 1, 0, 2, 2, 2],
    [0, 0, 0, 2, 1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0, 2],
    [0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2],
    [0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2, 2, 1, 1, 2],
    [0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2],
    [0, 0, 2, 2, 1, 1, 2, 2, 1, 1, 2, 2, 0, 0, 2, 2],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 1, 2],
    [0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1],
    [0, 2, 2, 2, 1, 2, 2, 2, 0, 2, 2, 2, 1, 2, 2, 2],
    [0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 1, 1, 1, 2, 0, 1, 1, 2, 2, 0, 1, 2, 2, 2, 0],
];
const ANCHOR2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2, 2, 8, 8, 2, 2,
    15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2, 2, 15, 15, 15, 15, 15, 2, 2, 15,
];
const ANCHOR3A: [u8; 64] = [
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5, 15, 15, 8, 15, 3,
    5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5, 10, 8, 13, 15, 12, 3, 3,
];
const ANCHOR3B: [u8; 64] = [
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6, 10, 15, 15, 10, 8,
    15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
];
const WEIGHTS: [&[u16]; 3] = [
    &[0, 21, 43, 64],
    &[0, 9, 18, 27, 37, 46, 55, 64],
    &[0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64],
];
struct Mode {
    subsets: usize,
    partition_bits: u32,
    rotation_bits: u32,
    selector_bits: u32,
    color_bits: u32,
    alpha_bits: u32,
    endpoint_pbits: bool,
    shared_pbits: bool,
    index_bits: u32,
    index2_bits: u32,
}
const MODES: [Mode; 8] = [
    Mode { subsets: 3, partition_bits: 4, rotation_bits: 0, selector_bits: 0, color_bits: 4, alpha_bits: 0, endpoint_pbits: true, shared_pbits: false, index_bits: 3, index2_bits: 0 },
    Mode { subsets: 2, partition_bits: 6, rotation_bits: 0, selector_bits: 0, color_bits: 6, alpha_bits: 0, endpoint_pbits: false, shared_pbits: true, index_bits: 3, index2_bits: 0 },
    Mode { subsets: 3, partition_bits: 6, rotation_bits: 0, selector_bits: 0, color_bits: 5, alpha_bits: 0, endpoint_pbits: false, shared_pbits: false, index_bits: 2, index2_bits: 0 },
    Mode { subsets: 2, partition_bits: 6, rotation_bits: 0, selector_bits: 0, color_bits: 7, alpha_bits: 0, endpoint_pbits: true, shared_pbits: false, index_bits: 2, index2_bits: 0 },
    Mode { subsets: 1, partition_bits: 0, rotation_bits: 2, selector_bits: 1, color_bits: 5, alpha_bits: 6, endpoint_pbits: false, shared_pbits: false, index_bits: 2, index2_bits: 3 },
    Mode { subsets: 1, partition_bits: 0, rotation_bits: 2, selector_bits: 0, color_bits: 7, alpha_bits: 8, endpoint_pbits: false, shared_pbits: false, index_bits: 2, index2_bits: 2 },
    Mode { subsets: 1, partition_bits: 0, rotation_bits: 0, selector_bits: 0, color_bits: 7, alpha_bits: 7, endpoint_pbits: true, shared_pbits: false, index_bits: 4, index2_bits: 0 },
    Mode { subsets: 2, partition_bits: 6, rotation_bits: 0, selector_bits: 0, color_bits: 5, alpha_bits: 5, endpoint_pbits: true, shared_pbits: false, index_bits: 2, index2_bits: 0 },
];
struct Bits {
    value: u128,
    position: u32,
}
impl Bits {
    fn take(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }
        let out = ((self.value >> self.position) & ((1u128 << count) - 1)) as u32;
        self.position += count;
        out
    }
}
fn expand(value: u32, bits: u32) -> u32 {
    let value = value << (8 - bits);
    value | (value >> bits)
}
fn interpolate(a: u32, b: u32, weight: u16) -> u8 {
    (((64 - weight as u32) * a + weight as u32 * b + 32) >> 6) as u8
}
fn subset(mode: &Mode, partition: usize, pixel: usize) -> usize {
    match mode.subsets {
        2 => ((PARTITION2[partition] >> pixel) & 1) as usize,
        3 => PARTITION3[partition][pixel] as usize,
        _ => 0,
    }
}
fn anchor(mode: &Mode, partition: usize, pixel: usize) -> bool {
    pixel == 0
        || match mode.subsets {
            2 => pixel == ANCHOR2[partition] as usize,
            3 => pixel == ANCHOR3A[partition] as usize || pixel == ANCHOR3B[partition] as usize,
            _ => false,
        }
}
/// Decode one 16-byte BC7 block into 16 RGBA pixels (row-major).
pub fn block(data: &[u8; 16]) -> [[u8; 4]; 16] {
    let mut bits = Bits { value: u128::from_le_bytes(*data), position: 0 };
    let Some(mode_index) = (0..8).find(|m| data[0] & (1 << m) != 0) else {
        return [[0; 4]; 16]; // reserved mode: transparent black
    };
    bits.position = mode_index as u32 + 1;
    let mode = &MODES[mode_index];
    let partition = bits.take(mode.partition_bits) as usize;
    let rotation = bits.take(mode.rotation_bits);
    let selector = bits.take(mode.selector_bits);
    let mut endpoints = [[[0u32; 4]; 2]; 3];
    for channel in 0..3 {
        for s in 0..mode.subsets {
            for e in 0..2 {
                endpoints[s][e][channel] = bits.take(mode.color_bits);
            }
        }
    }
    if mode.alpha_bits > 0 {
        for s in 0..mode.subsets {
            for e in 0..2 {
                endpoints[s][e][3] = bits.take(mode.alpha_bits);
            }
        }
    }
    let mut color_bits = mode.color_bits;
    let mut alpha_bits = mode.alpha_bits;
    if mode.endpoint_pbits || mode.shared_pbits {
        for s in 0..mode.subsets {
            let shared = if mode.shared_pbits { bits.take(1) } else { 0 };
            for e in 0..2 {
                let p = if mode.endpoint_pbits { bits.take(1) } else { shared };
                for channel in 0..4 {
                    endpoints[s][e][channel] = (endpoints[s][e][channel] << 1) | p;
                }
            }
        }
        color_bits += 1;
        if alpha_bits > 0 {
            alpha_bits += 1;
        }
    }
    for s in 0..mode.subsets {
        for e in 0..2 {
            for channel in 0..3 {
                endpoints[s][e][channel] = expand(endpoints[s][e][channel], color_bits);
            }
            endpoints[s][e][3] = if alpha_bits > 0 { expand(endpoints[s][e][3], alpha_bits) } else { 255 };
        }
    }
    let mut index1 = [0u32; 16];
    for (pixel, slot) in index1.iter_mut().enumerate() {
        let count = mode.index_bits - anchor(mode, partition, pixel) as u32;
        *slot = bits.take(count);
    }
    let mut index2 = [0u32; 16];
    if mode.index2_bits > 0 {
        for (pixel, slot) in index2.iter_mut().enumerate() {
            *slot = bits.take(mode.index2_bits - (pixel == 0) as u32);
        }
    }
    let weights = |bits: u32| WEIGHTS[(bits - 2) as usize];
    let mut out = [[0u8; 4]; 16];
    for pixel in 0..16 {
        let s = subset(mode, partition, pixel);
        let [e0, e1] = endpoints[s];
        let (color_index, color_weights, alpha_index, alpha_weights) = if mode.index2_bits == 0 {
            (index1[pixel], weights(mode.index_bits), index1[pixel], weights(mode.index_bits))
        } else if selector == 0 {
            (index1[pixel], weights(mode.index_bits), index2[pixel], weights(mode.index2_bits))
        } else {
            (index2[pixel], weights(mode.index2_bits), index1[pixel], weights(mode.index_bits))
        };
        let mut rgba = [0u8; 4];
        for channel in 0..3 {
            rgba[channel] = interpolate(e0[channel], e1[channel], color_weights[color_index as usize]);
        }
        rgba[3] = interpolate(e0[3], e1[3], alpha_weights[alpha_index as usize]);
        match rotation {
            1 => rgba.swap(0, 3),
            2 => rgba.swap(1, 3),
            3 => rgba.swap(2, 3),
            _ => {}
        }
        out[pixel] = rgba;
    }
    out
}
/// Decode a BC7 top mip (blocks row-major) to an RGBA8 image.
pub fn image(data: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let blocks_x = width.div_ceil(4);
    let blocks_y = height.div_ceil(4);
    ensure!(width > 0 && height > 0 && data.len() >= blocks_x * blocks_y * 16, "Truncated BC7 texture");
    let mut out = vec![0u8; width * height * 4];
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let offset = (by * blocks_x + bx) * 16;
            let pixels = block(data[offset..offset + 16].try_into().unwrap());
            for (i, rgba) in pixels.iter().enumerate() {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                if x < width && y < height {
                    out[(y * width + x) * 4..(y * width + x) * 4 + 4].copy_from_slice(rgba);
                }
            }
        }
    }
    Ok(out)
}

/// BC1 / BC3 (DXT1 / DXT5).
pub fn bc1_bc3(data: &[u8], w: usize, h: usize, bc3: bool) -> Result<Vec<u8>> {
    let size = if bc3 { 16 } else { 8 };
    let (bx, by) = (w.div_ceil(4), h.div_ceil(4));
    ensure!(data.len() >= bx * by * size, "Truncated BC1/BC3 texture");
    let mut out = vec![0u8; w * h * 4];
    // DirectXTex D3DXDecodeBC1 (texconv): float 565 endpoints, XMVectorLerp,
    // then XMStoreUByteN4 (saturate, x255, round half to even).
    let rgb = |c: u16| {
        [((c >> 11) & 31) as f32 * (1.0 / 31.0), ((c >> 5) & 63) as f32 * (1.0 / 63.0), (c & 31) as f32 * (1.0 / 31.0)]
    };
    let unorm = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round_ties_even() as u32;
    for y in 0..by {
        for x in 0..bx {
            let block = &data[(y * bx + x) * size..][..size];
            let (alpha, color) = if bc3 { (Some(&block[..8]), &block[8..]) } else { (None, block) };
            let c0 = u16::from_le_bytes([color[0], color[1]]);
            let c1 = u16::from_le_bytes([color[2], color[3]]);
            let (a, b) = (rgb(c0), rgb(c1));
            let lerp = |t: f32| [0, 1, 2].map(|i| unorm(a[i] + t * (b[i] - a[i])));
            let four = c0 > c1 || bc3;
            let mut palette = [[0u32; 4]; 4];
            let [r, g, bl] = [0, 1, 2].map(|i| unorm(a[i]));
            palette[0] = [r, g, bl, 255];
            let [r, g, bl] = [0, 1, 2].map(|i| unorm(b[i]));
            palette[1] = [r, g, bl, 255];
            let [r, g, bl] = lerp(if four { 1.0 / 3.0 } else { 0.5 });
            palette[2] = [r, g, bl, 255];
            let [r, g, bl] = if four { lerp(2.0 / 3.0) } else { [0, 0, 0] };
            palette[3] = [r, g, bl, if four { 255 } else { 0 }];
            let mut alphas = [255u32; 16];
            if let Some(alpha) = alpha {
                let (a0, a1) = (alpha[0] as u32, alpha[1] as u32);
                let mut table = [a0, a1, 0, 0, 0, 0, 0, 0];
                for i in 1..7u32 {
                    table[i as usize + 1] = if a0 > a1 {
                        ((7 - i) * a0 + i * a1 + 3) / 7
                    } else if i < 5 {
                        ((5 - i) * a0 + i * a1 + 2) / 5
                    } else {
                        0
                    };
                }
                if a0 <= a1 {
                    table[6] = 0;
                    table[7] = 255;
                }
                let bits = u64::from_le_bytes([alpha[2], alpha[3], alpha[4], alpha[5], alpha[6], alpha[7], 0, 0]);
                for (p, slot) in alphas.iter_mut().enumerate() {
                    *slot = table[((bits >> (3 * p)) & 7) as usize];
                }
            }
            let indices = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
            for p in 0..16 {
                let (px, py) = (x * 4 + p % 4, y * 4 + p / 4);
                if px >= w || py >= h {
                    continue;
                }
                let c = palette[((indices >> (2 * p)) & 3) as usize];
                let o = (py * w + px) * 4;
                out[o..o + 4].copy_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, if bc3 { alphas[p] as u8 } else { c[3] as u8 }]);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partition_and_anchor_tables_are_consistent() {
        for p in 0..64 {
            assert_eq!(PARTITION2[p] & 1, 0, "p2 {p}");
            assert_eq!((PARTITION2[p] >> ANCHOR2[p]) & 1, 1, "anchor2 {p}");
            assert_eq!(PARTITION3[p][0], 0, "p3 {p}");
            assert_eq!(PARTITION3[p][ANCHOR3A[p] as usize], 1, "anchor3a {p}");
            assert_eq!(PARTITION3[p][ANCHOR3B[p] as usize], 2, "anchor3b {p}");
        }
    }
    #[test]
    fn mode6_solid_block() {
        // Mode 6, all endpoints 0x7f with p-bit 1 -> 255; indices 0.
        let mut value: u128 = 1 << 6;
        let mut position = 7;
        for _ in 0..8 {
            value |= 0x7f << position;
            position += 7;
        }
        value |= 0b11 << position;
        let pixels = block(&value.to_le_bytes());
        assert!(pixels.iter().all(|p| *p == [255, 255, 255, 255]));
    }
}
