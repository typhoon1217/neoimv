pub fn rotate_rgba_cw(pixels: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, u32, u32)> {
    rotate_rgba(pixels, width, height, true)
}

pub fn rotate_rgba_ccw(pixels: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, u32, u32)> {
    rotate_rgba(pixels, width, height, false)
}

fn rotate_rgba(
    pixels: &[u8],
    width: u32,
    height: u32,
    clockwise: bool,
) -> Option<(Vec<u8>, u32, u32)> {
    let src_len = width.checked_mul(height)?.checked_mul(4)? as usize;
    if pixels.len() != src_len {
        return None;
    }

    let new_width = height;
    let new_height = width;
    let mut out = vec![0; src_len];

    for y in 0..height {
        for x in 0..width {
            let (nx, ny) = if clockwise {
                (height - 1 - y, x)
            } else {
                (y, width - 1 - x)
            };
            let src = ((y * width + x) * 4) as usize;
            let dst = ((ny * new_width + nx) * 4) as usize;
            out[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
        }
    }

    Some((out, new_width, new_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_2x3_clockwise() {
        let pixels = rgba_ids(&[1, 2, 3, 4, 5, 6]);
        let (rotated, width, height) = rotate_rgba_cw(&pixels, 2, 3).unwrap();
        assert_eq!((width, height), (3, 2));
        assert_eq!(rgba_ids_from(&rotated), vec![5, 3, 1, 6, 4, 2]);
    }

    #[test]
    fn rotates_2x3_counter_clockwise() {
        let pixels = rgba_ids(&[1, 2, 3, 4, 5, 6]);
        let (rotated, width, height) = rotate_rgba_ccw(&pixels, 2, 3).unwrap();
        assert_eq!((width, height), (3, 2));
        assert_eq!(rgba_ids_from(&rotated), vec![2, 4, 6, 1, 3, 5]);
    }

    fn rgba_ids(ids: &[u8]) -> Vec<u8> {
        ids.iter().flat_map(|id| [*id, 0, 0, 255]).collect()
    }

    fn rgba_ids_from(pixels: &[u8]) -> Vec<u8> {
        pixels.chunks_exact(4).map(|px| px[0]).collect()
    }
}
