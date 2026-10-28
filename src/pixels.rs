use crate::{Result, protocol::Request};
use tiny_skia::*;
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}
pub fn rasterize(r: &Request) -> Result<Gray> {
    crate::protocol::validate(r)?;
    let (mut left, mut top, mut right, mut bottom) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for s in &r.strokes {
        for &[x, y] in &s.points {
            left = left.min(x - s.width / 2.);
            right = right.max(x + s.width / 2.);
            top = top.min(y - s.width / 2.);
            bottom = bottom.max(y + s.width / 2.);
        }
    }
    let w = ((right - left) * 6. + 24.).ceil() as usize;
    let h = ((bottom - top) * 6. + 24.).ceil() as usize;
    // Bound allocations even for valid coordinates spread over an enormous page.
    if w.checked_mul(h).is_none_or(|n| n > 64 * 1024 * 1024) {
        return Err("raster exceeds 64 megapixels".into());
    }
    let mut pix = Pixmap::new(w as u32, h as u32).ok_or("invalid raster dimensions")?;
    pix.fill(Color::WHITE);
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;
    for s in &r.strokes {
        let mut p = PathBuilder::new();
        for (i, &[x, y]) in s.points.iter().enumerate() {
            let (x, y) = (
                ((x - left) * 6. + 12.) as f32,
                ((y - top) * 6. + 12.) as f32,
            );
            if i == 0 {
                p.move_to(x, y);
            } else {
                p.line_to(x, y);
            }
        }
        if let Some(&[x, y]) = s.points.first()
            && (s.points.len() == 1 || s.points.iter().all(|p| *p == [x, y]))
        {
            let path = PathBuilder::from_circle(
                ((x - left) * 6. + 12.) as f32,
                ((y - top) * 6. + 12.) as f32,
                (s.width * 3.) as f32,
            )
            .ok_or("invalid circle")?;
            pix.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
            continue;
        }
        if let Some(p) = p.finish() {
            pix.stroke_path(
                &p,
                &paint,
                &tiny_skia::Stroke {
                    width: (s.width * 6.) as f32,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }
    Ok(Gray {
        width: w,
        height: h,
        data: pix.data().chunks_exact(4).map(|p| p[0]).collect(),
    })
}
fn cubic(x: f64) -> f64 {
    let x = x.abs();
    if x < 1. {
        ((1.5 * x - 2.5) * x) * x + 1.
    } else if x < 2. {
        ((-0.5 * x + 2.5) * x - 4.) * x + 2.
    } else {
        0.
    }
}
fn weights(input: usize, output: usize) -> (Vec<(usize, Vec<i64>)>, u32) {
    let scale = input as f64 / output as f64;
    let filter = scale.max(1.);
    let support = 2. * filter;
    let floating: Vec<(usize, Vec<f64>)> = (0..output)
        .map(|o| {
            let center = (o as f64 + 0.5) * scale;
            let start = ((center - support + 0.5) as isize).max(0) as usize;
            let end = ((center + support + 0.5) as usize).min(input);
            let mut weights: Vec<f64> = (start..end)
                .map(|i| cubic((i as f64 - center + 0.5) / filter))
                .collect();
            let sum: f64 = weights.iter().sum();
            for v in &mut weights {
                *v /= sum;
            }
            (start, weights)
        })
        .collect();
    // PyTorch uint8 antialiased resize uses signed 16-bit coefficients and
    // the greatest shared precision which keeps the maximum coefficient in range.
    let max = floating
        .iter()
        .flat_map(|(_, v)| v)
        .copied()
        .fold(0f64, f64::max);
    let mut bits = 0;
    while max * ((1u64 << (bits + 1)) as f64) < 32768. {
        bits += 1;
    }
    (
        floating
            .into_iter()
            .map(|(start, v)| {
                (
                    start,
                    v.into_iter()
                        .map(|w| (w * ((1u64 << bits) as f64)).round() as i64)
                        .collect(),
                )
            })
            .collect(),
        bits,
    )
}
pub fn preprocess(g: &Gray) -> Result<Vec<f32>> {
    let corners = [
        g.data[0],
        g.data[g.width - 1],
        g.data[(g.height - 1) * g.width],
        g.data[g.data.len() - 1],
    ];
    let mut bg = corners[0];
    let mut count = 0;
    for &c in &corners {
        let n = corners.iter().filter(|&&v| v == c).count();
        if n > count {
            count = n;
            bg = c;
        }
    }
    let (mut l, mut t, mut r, mut b) = (g.width, g.height, 0, 0);
    for y in 0..g.height {
        for x in 0..g.width {
            if g.data[y * g.width + x].abs_diff(bg) > 15 {
                l = l.min(x);
                r = r.max(x + 1);
                t = t.min(y);
                b = b.max(y + 1);
            }
        }
    }
    if r <= l || b <= t {
        return Err("image has no ink".into());
    }
    let (w, h) = (r - l, b - t);
    let (nw, nh) = if w <= h {
        let long = 447 * h / w;
        if long > 448 {
            ((448 * w / h).max(1), 448)
        } else {
            (447, long)
        }
    } else {
        let long = 447 * w / h;
        if long > 448 {
            (448, (448 * h / w).max(1))
        } else {
            (long, 447)
        }
    };
    let (wx, bx) = weights(w, nw);
    let (wy, by) = weights(h, nh);
    let mut horizontal = vec![0u8; nw * h];
    for y in 0..h {
        for (x, (start, ws)) in wx.iter().enumerate() {
            let sum: i64 = ws
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    v * (g.data[(t + y) * g.width + l + start + i] as f32 * 0.2989
                        + g.data[(t + y) * g.width + l + start + i] as f32 * 0.587
                        + g.data[(t + y) * g.width + l + start + i] as f32 * 0.114)
                        as u8 as i64
                })
                .sum();
            horizontal[y * nw + x] = ((sum + (1 << (bx - 1))) >> bx).clamp(0, 255) as u8;
        }
    }
    let mut result = vec![0.; 448 * 448];
    for (y, (start, ws)) in wy.iter().enumerate() {
        for x in 0..nw {
            let sum: i64 = ws
                .iter()
                .enumerate()
                .map(|(i, &v)| v * horizontal[(start + i) * nw + x] as i64)
                .sum();
            let v = ((sum + (1 << (by - 1))) >> by).clamp(0, 255) as u8;
            result[y * 448 + x] = (v as f32 * (1.0f32 / 255.0) - 0.9545467) / 0.15394445;
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_point_margins() {
        let r = serde_json::from_str(r#"{"id":0,"strokes":[{"widthPt":2,"points":[[42,13]]}]}"#)
            .unwrap();
        let g = rasterize(&r).unwrap();
        assert_eq!((g.width, g.height), (36, 36));
        assert_eq!(g.data[18 * 36 + 18], 0);
        assert!(g.data[..12 * 36].iter().all(|&v| v == 255));
        assert_eq!(preprocess(&g).unwrap().len(), 448 * 448);
    }
}
