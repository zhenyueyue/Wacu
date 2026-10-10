//! Smooth, cached-by-service halo pixels; no blur work runs during animation.
use std::f64::consts::{FRAC_PI_2, TAU};

const RADIUS: f64 = 18.0;
const SIGMA: f64 = 2.8;

fn alpha(dx: f64, dy: f64, percent: i32) -> f64 {
    let radial_distance = (dx.hypot(dy) - RADIUS).abs();
    let gaussian = |distance: f64| (-distance * distance / (2.0 * SIGMA * SIGMA)).exp();
    let base = 0.15 * gaussian(radial_distance);
    let progress = percent.clamp(0, 100) as f64 / 100.0;
    if progress == 0.0 {
        return base;
    }
    let sweep = progress * TAU;
    let angle = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU);
    let arc_distance = if progress == 1.0 || angle <= sweep {
        radial_distance
    } else {
        let end_angle = sweep - FRAC_PI_2;
        let start_distance = dx.hypot(dy + RADIUS);
        let end_distance = (dx - RADIUS * end_angle.cos()).hypot(dy - RADIUS * end_angle.sin());
        start_distance.min(end_distance)
    };
    base + 0.30 * gaussian(arc_distance)
}

pub fn make(percent: i32, accent: slint::Color) -> slint::Image {
    // 50px reference circle + 8px padding per side, rasterized at 2x.
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(132, 132);
    for (index, pixel) in buffer.make_mut_bytes().chunks_exact_mut(4).enumerate() {
        let dx = (index % 132) as f64 / 2.0 + 0.25 - 33.0;
        let dy = (index / 132) as f64 / 2.0 + 0.25 - 33.0;
        pixel.copy_from_slice(&[
            accent.red(),
            accent.green(),
            accent.blue(),
            (alpha(dx, dy, percent) * 255.0).round() as u8,
        ]);
    }
    slint::Image::from_rgba8(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halo_fades_continuously_outward() {
        let samples = [0.0, 1.0, 3.0, 6.0, 8.0].map(|offset| alpha(0.0, -RADIUS - offset, 24));
        assert!(samples.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(samples[4] < 0.01);
    }

    #[test]
    fn arc_endpoints_have_no_hard_cut() {
        for angle in [-FRAC_PI_2, TAU * 0.24 - FRAC_PI_2] {
            let sample = |angle: f64| alpha(RADIUS * angle.cos(), RADIUS * angle.sin(), 24);
            assert!((sample(angle - 0.005) - sample(angle + 0.005)).abs() < 0.001);
        }
    }

    #[test]
    fn zero_and_full_progress_are_symmetric() {
        for percent in [0, 100] {
            assert!((alpha(RADIUS, 0.0, percent) - alpha(0.0, -RADIUS, percent)).abs() < 1e-8);
        }
        assert!(alpha(RADIUS, 0.0, 100) > alpha(RADIUS, 0.0, 0));
    }
}
