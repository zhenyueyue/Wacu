//! Physical desktop geometry for the approved ring -> line -> Logo handoff.
use std::f64::consts::PI;

pub const OPEN_MS: u64 = 1080;
pub const CLOSE_MS: u64 = 480;
pub const LOCAL_OPEN_MS: u64 = 630;
pub const LOCAL_CLOSE_MS: u64 = 200;
pub const MINI_SPIN_MS: u64 = 420;

pub fn spin_turn(progress: f64) -> f64 {
    let s = progress.clamp(0.0, 1.0);
    s.powi(3) * (10.0 - 15.0 * s + 6.0 * s * s)
}

fn mini_sector(count: usize, index: usize) -> (f64, f64) {
    let sector = PI * 2.0 / count.max(1) as f64;
    let seam = if count == 1 { 0.0 } else { 0.10 };
    (
        -PI / 2.0 + index as f64 * sector + seam / 2.0,
        sector - seam,
    )
}

fn quota_fraction(percent: f64) -> f64 {
    if percent.is_finite() {
        percent.clamp(0.0, 100.0) / 100.0
    } else {
        0.0
    }
}

pub fn mini_path(
    count: usize,
    index: usize,
    percent: f64,
    turn: f64,
    tip: bool,
    center: Vector,
) -> String {
    if count == 0 || quota_fraction(percent) == 0.0 {
        return String::new();
    }
    let (start, span) = mini_sector(count, index);
    let start = start + turn * PI * 2.0;
    let span = span * quota_fraction(percent);
    let span = if tip { span.min(0.24) } else { span };
    let point = |angle: f64| (center.x + 29.0 * angle.cos(), center.y + 29.0 * angle.sin());
    let (x, y) = point(start);
    if span >= PI * 2.0 {
        let (mx, my) = point(start + PI);
        return format!(
            "M {x:.6} {y:.6} A 29 29 0 1 1 {mx:.6} {my:.6} A 29 29 0 1 1 {x:.6} {y:.6} Z"
        );
    }
    let (ex, ey) = point(start + span);
    format!(
        "M {x:.6} {y:.6} A 29 29 0 {} 1 {ex:.6} {ey:.6}",
        if span > PI { 1 } else { 0 }
    )
}

// Keep the established desktop travel rate. Only the assembly after all
// three lines have reached the Logo gets 65% more time, with a smooth ramp.
pub fn time_position(spatial: bool, progress: f64) -> f64 {
    let split = if spatial { 0.52 } else { 0.10 };
    let ramp = 0.08;
    let after = (progress.clamp(0.0, 1.0) - split).max(0.0);
    let extra = if after < ramp {
        let t = after / ramp;
        ramp * (t.powi(3) - t.powi(4) / 2.0)
    } else {
        after - ramp / 2.0
    };
    progress.clamp(0.0, 1.0) + 0.65 * extra
}

pub fn progress_at(spatial: bool, time: f64) -> f64 {
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if time_position(spatial, middle) < time {
            low = middle;
        } else {
            high = middle;
        }
    }
    ((low + high) / 2.0).clamp(0.0, 1.0)
}

pub fn base_ms(spatial: bool, open: bool) -> u64 {
    match (spatial, open) {
        (true, true) => OPEN_MS,
        (true, false) => CLOSE_MS,
        (false, true) => LOCAL_OPEN_MS,
        (false, false) => LOCAL_CLOSE_MS,
    }
}

pub fn motion_time_ms(spatial: bool, open: bool, progress: f64) -> f64 {
    if progress < 0.0 {
        progress * MINI_SPIN_MS as f64
    } else {
        time_position(spatial, progress) * base_ms(spatial, open) as f64
    }
}

pub fn motion_progress(spatial: bool, open: bool, time_ms: f64) -> f64 {
    if time_ms < 0.0 {
        (time_ms / MINI_SPIN_MS as f64).max(-1.0)
    } else {
        progress_at(spatial, time_ms / base_ms(spatial, open) as f64)
    }
}

#[cfg(debug_assertions)]
pub fn duration_ms(spatial: bool, open: bool) -> u64 {
    (motion_time_ms(spatial, open, 1.0)
        - motion_time_ms(spatial, open, if open { 0.0 } else { -1.0 }))
    .ceil() as u64
}

#[derive(Clone, Copy, Debug)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub source: Vector,
    pub logo: Vector,
    pub source_scale: f64,
    pub target_scale: f64,
    pub quota_offset: f64,
    pub quota_step: f64,
    pub quota_diameter: f64,
    pub quota_width: f64,
    pub capsule_width: f64,
    pub bottom: f64,
}

pub fn smooth(value: f64) -> f64 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn interpolate(a: Vector, b: Vector, t: f64) -> Vector {
    Vector {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

fn cubic(route: [Vector; 4], t: f64) -> Vector {
    let u = 1.0 - t;
    let [a, b, c, d] = route;
    Vector {
        x: u.powi(3) * a.x + 3.0 * u * u * t * b.x + 3.0 * u * t * t * c.x + t.powi(3) * d.x,
        y: u.powi(3) * a.y + 3.0 * u * u * t * b.y + 3.0 * u * t * t * c.y + t.powi(3) * d.y,
    }
}

impl Geometry {
    pub fn spatial(self) -> bool {
        (self.logo.x - self.source.x).hypot(self.logo.y - self.source.y) >= 24.0
    }

    pub fn bounds(self) -> (i32, i32, u32, u32) {
        let margin = 140.0 * self.source_scale.max(self.target_scale);
        let x = (self.source.x.min(self.logo.x) - margin).floor();
        let y = (self.source.y.min(self.logo.y) - margin).floor();
        (
            x as i32,
            y as i32,
            (self.source.x.max(self.logo.x) + margin - x).ceil() as u32,
            (self.source.y.max(self.bottom) + margin - y).ceil() as u32,
        )
    }

    pub fn rail(self, progress: f64) -> f64 {
        if self.spatial() {
            ((progress * 720.0 - 290.0) / 420.0).clamp(0.0, 1.0)
        } else {
            progress.clamp(0.0, 1.0)
        }
    }

    pub fn visibility(self, progress: f64) -> f64 {
        if self.spatial() {
            smooth((progress * 720.0 - 205.0) / 80.0)
        } else {
            (progress * 10.0).clamp(0.0, 1.0)
        }
    }

    pub fn mini_reveal(self, progress: f64) -> f64 {
        if self.spatial() {
            1.0 - smooth(progress * 720.0 / 95.0)
        } else {
            1.0 - smooth(progress / 0.22)
        }
    }

    pub fn opacity(self, index: usize, progress: f64, percent: f64) -> f32 {
        if quota_fraction(percent) > 0.0 {
            return 1.0;
        }
        // Empty/unknown Mini sectors have no colored endpoint. Grow their
        // transfer line smoothly before travel, rather than flashing a dot.
        smooth(if self.spatial() {
            (progress * 720.0 - index as f64 * 18.0) / 33.0
        } else {
            progress / 0.025
        }) as f32
    }

    pub fn coil(self, count: usize, index: usize, progress: f64) -> f64 {
        let stagger = 0.12 * index as f64 / count.saturating_sub(1).max(1) as f64;
        smooth((self.rail(progress) - 0.52 - stagger) / 0.28)
    }

    pub fn width(self, count: usize, index: usize, progress: f64) -> f64 {
        let initial = 2.8
            * (self.source_scale
                + (self.source_scale.max(self.target_scale) - self.source_scale)
                    * smooth(progress / 0.12));
        initial + (self.quota_width - initial) * self.coil(count, index, progress)
    }

    pub fn terminal_path(self, index: usize, percent: f64, origin: Vector, scale: f64) -> String {
        if percent <= 0.0 {
            return String::new();
        }
        let x = (self.logo.x - origin.x) / scale;
        let y =
            (self.logo.y + self.quota_offset + index as f64 * self.quota_step - origin.y) / scale;
        let ratio =
            (self.quota_diameter - 4.0 * self.target_scale - self.quota_width) / 56.0 / scale;
        let radius = 24.0 * ratio;
        if percent >= 100.0 {
            return format!(
                "M {x:.6} {:.6} A {radius:.6} {radius:.6} 0 1 1 {x:.6} {:.6} A {radius:.6} {radius:.6} 0 1 1 {x:.6} {:.6} Z",
                y - radius,
                y + radius,
                y - radius
            );
        }
        let angle = -PI / 2.0 + percent * PI * 2.0 / 100.0;
        // Preserve the static meter's two-decimal SVG endpoint before scaling.
        let end_x = ((28.0 + 24.0 * angle.cos()) * 100.0).round() / 100.0;
        let end_y = ((28.0 + 24.0 * angle.sin()) * 100.0).round() / 100.0;
        format!(
            "M {x:.6} {:.6} A {radius:.6} {radius:.6} 0 {} 1 {:.6} {:.6}",
            y - radius,
            if percent > 50.0 { 1 } else { 0 },
            x + (end_x - 28.0) * ratio,
            y + (end_y - 28.0) * ratio
        )
    }

    pub fn points(self, count: usize, index: usize, progress: f64, percent: f64) -> Vec<Vector> {
        let q = ((progress * 720.0 - index as f64 * 18.0) / 330.0).clamp(0.0, 1.0);
        if self.spatial() && q < 1.0 {
            return self.phase_points(count, index, q, percent);
        }
        let logo_arc = self.phase_points(count, index, 1.0, percent);
        if !self.spatial() && progress < 0.10 {
            let source_arc = self.phase_points(count, index, 0.0, percent);
            return source_arc
                .into_iter()
                .zip(logo_arc)
                .map(|(a, b)| interpolate(a, b, smooth(progress / 0.10)))
                .collect();
        }
        let r = self.rail(progress);
        let stagger = 0.12 * index as f64 / count.saturating_sub(1).max(1) as f64;
        let unwrap = smooth((r - 0.10 - stagger) / 0.16);
        let rotate = smooth((r - 0.10 - stagger) / 0.075);
        let down = smooth((r - 0.24 - stagger) / 0.34);
        let coil = self.coil(count, index, progress);
        let phase = smooth((r - 0.50 - stagger) / 0.28);
        let side = if index % 2 == 0 { -1.0 } else { 1.0 };
        let start = -PI / 2.0 + index as f64 * PI * 2.0 / count.max(1) as f64;
        let span = PI * 1.6 / count.max(1) as f64;
        let outgoing = if side < 0.0 { -PI } else { 0.0 };
        let turn = (outgoing - start - span / 2.0 + PI).rem_euclid(PI * 2.0) - PI;
        let motion = smooth(r / 0.43);
        let shell_width =
            56.0 * self.target_scale + (self.capsule_width - 80.0 * self.target_scale) * motion;
        let edge = shell_width / 2.0 - 3.0 * self.target_scale;
        let center = Vector {
            x: self.logo.x + side * 10.0 * self.target_scale * (1.0 - phase),
            y: self.logo.y + self.quota_offset + index as f64 * self.quota_step,
        };
        // Slint's static Path fits the viewbox into its stroke-inset bounds.
        let radius = (self.quota_diameter * (0.86 + 0.14 * phase)
            - 4.0 * self.target_scale
            - self.quota_width)
            * 24.0
            / 56.0;
        let destination_y = center.y + radius;
        (0..=64)
            .map(|j| {
                let s = j as f64 / 64.0;
                let angle = start + span * s + turn * rotate;
                let arc = Vector {
                    x: self.logo.x + 29.0 * self.target_scale * angle.cos(),
                    y: self.logo.y + 29.0 * self.target_scale * angle.sin(),
                };
                let line = Vector {
                    x: self.logo.x + side * edge,
                    y: self.logo.y
                        + (destination_y - self.logo.y) * down
                        + (s - 0.5) * 24.0 * self.target_scale,
                };
                let angle = -PI / 2.0 + PI * 2.0 * percent.clamp(0.0, 100.0) / 100.0 * s;
                let quota = Vector {
                    x: center.x + radius * angle.cos(),
                    y: center.y + radius * angle.sin(),
                };
                if coil == 0.0 {
                    return interpolate(arc, line, unwrap);
                }
                // First round the lower corner, then orbit from underneath
                // into the quota arc. Both stages stay outside the avatar.
                let line_center = Vector {
                    x: line.x,
                    y: self.logo.y + (destination_y - self.logo.y) * down,
                };
                let span = PI * 2.0 * percent.clamp(0.0, 100.0) / 100.0;
                let final_angle = -PI / 2.0 + span / 2.0;
                let initial_angle = (line_center.y - center.y).atan2(line_center.x - center.x);
                let bend = smooth(coil / 0.32);
                let orbit = smooth((coil - 0.32) / 0.68);
                let lower_angle = initial_angle + (PI / 2.0 - initial_angle) * bend;
                let mid_angle = lower_angle + (final_angle - PI / 2.0) * orbit;
                let initial_radius = (line_center.x - center.x).hypot(line_center.y - center.y);
                let mid_radius = initial_radius + (radius - initial_radius) * bend;
                let mid = Vector {
                    x: center.x + mid_radius * mid_angle.cos(),
                    y: center.y + mid_radius * mid_angle.sin(),
                };
                let length = 24.0 * self.target_scale * (1.0 - coil) + radius * span * coil;
                let sweep = span * coil;
                let theta = PI / 2.0 * bend + (final_angle - PI / 2.0) * orbit;
                if sweep <= 0.000001 {
                    return Vector {
                        x: mid.x - theta.sin() * length * (s - 0.5),
                        y: mid.y + theta.cos() * length * (s - 0.5),
                    };
                }
                let a = sweep * (s - 0.5);
                let factor = 2.0 * (a / 2.0).sin() * length / sweep;
                let curled = Vector {
                    x: mid.x - (theta + a / 2.0).sin() * factor,
                    y: mid.y + (theta + a / 2.0).cos() * factor,
                };
                if coil == 1.0 { quota } else { curled }
            })
            .collect()
    }

    fn phase_points(self, count: usize, index: usize, q: f64, percent: f64) -> Vec<Vector> {
        let count = count.max(1);
        let start = -PI / 2.0 + index as f64 * PI * 2.0 / count as f64;
        let span = PI * 1.6 / count as f64;
        let (mini_start, mini_span) = mini_sector(count, index);
        let mini_span = mini_span * quota_fraction(percent);
        let middle = start + span / 2.0;
        let target_radius = 29.0 * self.target_scale;
        let end = Vector {
            x: self.logo.x + target_radius * middle.cos(),
            y: self.logo.y + target_radius * middle.sin(),
        };
        let dx = self.logo.x - self.source.x;
        let dy = self.logo.y - self.source.y;
        let distance = dx.hypot(dy).max(1.0);
        let direction = Vector {
            x: dx / distance,
            y: dy / distance,
        };
        let mut normal = Vector {
            x: dy / distance,
            y: -dx / distance,
        };
        if normal.y > 0.00001 {
            normal.x = -normal.x;
            normal.y = -normal.y;
        }
        let lane = (count - 1) as f64 / 2.0 - index as f64;
        let origin = Vector {
            x: self.source.x
                + direction.x * 27.0 * self.source_scale
                + normal.x * lane * 8.0 * self.source_scale,
            y: self.source.y
                + direction.y * 27.0 * self.source_scale
                + normal.y * lane * 8.0 * self.source_scale,
        };
        let scale = self.source_scale.max(self.target_scale);
        let gap = (distance * 0.18).clamp(12.0 * scale, 50.0 * scale);
        let bend = ((distance * 0.035).clamp(10.0 * scale, 18.0 * scale) + lane * gap) * 4.0 / 3.0;
        let a = interpolate(origin, end, 1.0 / 3.0);
        let b = interpolate(origin, end, 2.0 / 3.0);
        let route = [
            origin,
            Vector {
                x: a.x + normal.x * bend,
                y: a.y + normal.y * bend,
            },
            Vector {
                x: b.x + normal.x * bend,
                y: b.y + normal.y * bend,
            },
            end,
        ];
        let portion = (95.0 * scale / (end.x - origin.x).hypot(end.y - origin.y).max(1.0)).min(0.9);
        (0..=64)
            .map(|j| {
                let s = j as f64 / 64.0;
                let angle = start + span * s;
                let ribbon = |t| cubic(route, t + (s - 0.5) * portion);
                if q < 0.20 {
                    let source_angle = mini_start + mini_span * s;
                    let from = Vector {
                        x: self.source.x + 29.0 * self.source_scale * source_angle.cos(),
                        y: self.source.y + 29.0 * self.source_scale * source_angle.sin(),
                    };
                    interpolate(from, ribbon(0.0), smooth(q / 0.20))
                } else if q < 0.80 {
                    ribbon(smooth((q - 0.20) / 0.60))
                } else {
                    let to = Vector {
                        x: self.logo.x + target_radius * angle.cos(),
                        y: self.logo.y + target_radius * angle.sin(),
                    };
                    interpolate(ribbon(1.0), to, smooth((q - 0.80) / 0.20))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mini_sectors_divide_the_perimeter_equally() {
        let g = fixture();
        for count in 1..=3 {
            for i in 0..count {
                let (start, span) = mini_sector(count, i);
                let seam = if count == 1 { 0.0 } else { 0.10 };
                assert!((span + seam - PI * 2.0 / count as f64).abs() < 1e-10);
                for (j, p) in g.phase_points(count, i, 0.0, 100.0).iter().enumerate() {
                    let angle = start + span * j as f64 / 64.0;
                    assert!((p.x - g.source.x - 29.0 * g.source_scale * angle.cos()).abs() < 1e-8);
                    assert!((p.y - g.source.y - 29.0 * g.source_scale * angle.sin()).abs() < 1e-8);
                }
            }
        }
    }

    #[test]
    fn mini_rim_encodes_quota_and_handoff_starts_at_the_same_arc() {
        let center = Vector { x: 33.0, y: 33.0 };
        assert!(mini_path(1, 0, 100.0, 0.0, false, center).ends_with('Z'));
        assert!(mini_path(1, 0, 50.0, 0.0, false, center).ends_with("33.000000 62.000000"));
        assert!(mini_path(1, 0, 25.0, 0.0, false, center).ends_with("62.000000 33.000000"));
        for percent in [0.0, -1.0, f64::NAN] {
            assert!(mini_path(1, 0, percent, 0.0, false, center).is_empty());
            assert!(mini_path(1, 0, percent, 0.5, true, center).is_empty());
        }
        for count in 1..=3 {
            for index in 0..count {
                for percent in [0.0, 25.0, 50.0, 100.0] {
                    let (start, span) = mini_sector(count, index);
                    for g in [
                        fixture(),
                        Geometry {
                            logo: fixture().source,
                            ..fixture()
                        },
                    ] {
                        for (j, p) in g.points(count, index, 0.0, percent).iter().enumerate() {
                            let angle = start + span * percent / 100.0 * j as f64 / 64.0;
                            assert!(
                                (p.x - g.source.x - 29.0 * g.source_scale * angle.cos()).abs()
                                    < 1e-8
                            );
                            assert!(
                                (p.y - g.source.y - 29.0 * g.source_scale * angle.sin()).abs()
                                    < 1e-8
                            );
                        }
                        assert_eq!(g.opacity(index, 0.0, 0.0), 0.0);
                        assert_eq!(g.opacity(index, 0.20, 0.0), 1.0);
                    }
                }
            }
        }
    }

    #[test]
    fn return_finishes_one_turn_and_remains_reversible() {
        assert_eq!(spin_turn(0.0), 0.0);
        assert_eq!(spin_turn(1.0), 1.0);
        assert!(1.0 - spin_turn(0.999) < 1e-7);
        assert_eq!(duration_ms(true, true), 1389);
        assert!(duration_ms(true, false) < 1196);
        assert!(duration_ms(false, false) < 749);
        for spatial in [true, false] {
            for open in [true, false] {
                for step in -100..=100 {
                    let p = step as f64 / 100.0;
                    assert!(
                        (motion_progress(spatial, open, motion_time_ms(spatial, open, p)) - p)
                            .abs()
                            < 1e-10
                    );
                }
            }
        }
    }

    #[test]
    fn desktop_travel_keeps_its_rate_while_assembly_slows() {
        for step in 0..=52 {
            let progress = step as f64 / 100.0;
            assert_eq!(time_position(true, progress), progress);
        }
        for spatial in [true, false] {
            let dt = time_position(spatial, 0.81) - time_position(spatial, 0.80);
            assert!((dt / 0.01 - 1.65).abs() < 0.000001);
            for step in 0..=100 {
                let p = step as f64 / 100.0;
                assert!((progress_at(spatial, time_position(spatial, p)) - p).abs() < 1e-10);
            }
        }
    }

    fn fixture() -> Geometry {
        Geometry {
            source: Vector {
                x: -700.0,
                y: 500.0,
            },
            logo: Vector {
                x: -100.0,
                y: 180.0,
            },
            source_scale: 1.0,
            target_scale: 1.5,
            quota_offset: 102.0,
            quota_step: 115.5,
            quota_diameter: 84.0,
            quota_width: 6.75,
            capsule_width: 144.0,
            bottom: 700.0,
        }
    }
    #[test]
    fn rings_land_on_logo_not_quota_positions() {
        let g = fixture();
        for count in 1..=3 {
            for i in 0..count {
                for p in g.phase_points(count, i, 1.0, 100.0) {
                    assert!(
                        ((p.x - g.logo.x).hypot(p.y - g.logo.y) - 29.0 * g.target_scale).abs()
                            < 0.001
                    );
                }
            }
        }
    }
    #[test]
    fn independent_routes_separate_in_middle() {
        let g = fixture();
        let centers: Vec<_> = (0..3)
            .map(|i| g.phase_points(3, i, 0.5, 100.0)[32])
            .collect();
        for pair in centers.windows(2) {
            assert!((pair[0].x - pair[1].x).hypot(pair[0].y - pair[1].y) > 40.0);
        }
    }
    #[test]
    fn negative_coordinates_and_dpi_stay_inside_fixed_surface() {
        for logo in [
            Vector {
                x: -100.0,
                y: 180.0,
            },
            Vector {
                x: -1300.0,
                y: -180.0,
            },
            Vector {
                x: -695.0,
                y: 160.0,
            },
        ] {
            let g = Geometry { logo, ..fixture() };
            let (x, y, w, h) = g.bounds();
            for step in 0..=100 {
                for i in 0..3 {
                    for p in g.points(3, i, step as f64 / 100.0, 24.0) {
                        assert!(p.x.is_finite() && p.y.is_finite());
                        assert!(p.x >= x as f64 && p.x <= x as f64 + w as f64);
                        assert!(p.y >= y as f64 && p.y <= y as f64 + h as f64);
                    }
                }
            }
        }
    }
    #[test]
    fn trajectory_is_continuous_across_shape_changes() {
        let g = fixture();
        for boundary in [0.20, 0.80] {
            for i in 0..3 {
                let before = g.phase_points(3, i, boundary - 0.00001, 100.0);
                let after = g.phase_points(3, i, boundary + 0.00001, 100.0);
                for (a, b) in before.into_iter().zip(after) {
                    assert!((a.x - b.x).hypot(a.y - b.y) < 0.1);
                }
            }
        }
    }

    #[test]
    fn arriving_lines_remain_visible_through_quota_coiling() {
        for count in 1..=3 {
            for i in 0..count {
                for step in 12..=100 {
                    let p = step as f64 / 100.0;
                    assert_eq!(fixture().opacity(i, p, 24.0), 1.0);
                    assert_eq!(fixture().opacity(i, p, 0.0), 1.0);
                    assert_eq!(fixture().points(count, i, p, 24.0).len(), 65);
                }
            }
        }
    }

    #[test]
    fn final_path_matches_static_quota_arc_and_width() {
        let g = fixture();
        let radius = (g.quota_diameter - 4.0 * g.target_scale - g.quota_width) * 24.0 / 56.0;
        for (i, percent) in [24.0, 13.0, 2.0].into_iter().enumerate() {
            let points = g.points(3, i, 1.0, percent);
            let center = Vector {
                x: g.logo.x,
                y: g.logo.y + g.quota_offset + i as f64 * g.quota_step,
            };
            for (j, p) in points.iter().enumerate() {
                let angle = -PI / 2.0 + PI * 2.0 * percent / 100.0 * j as f64 / 64.0;
                assert!((p.x - center.x - radius * angle.cos()).abs() < 0.0001);
                assert!((p.y - center.y - radius * angle.sin()).abs() < 0.0001);
            }
            assert_eq!(g.width(3, i, 1.0), g.quota_width);
        }
    }

    #[test]
    fn lines_round_the_lower_corner_before_orbiting_to_quota_arcs() {
        let g = fixture();
        for count in 1..=3 {
            for i in 0..count {
                let p = (0..=10000)
                    .map(|step| step as f64 / 10000.0)
                    .min_by(|a, b| {
                        (g.coil(count, i, *a) - 0.32)
                            .abs()
                            .total_cmp(&(g.coil(count, i, *b) - 0.32).abs())
                    })
                    .unwrap();
                let midpoint = g.points(count, i, p, 24.0)[32];
                let center_y = g.logo.y + g.quota_offset + i as f64 * g.quota_step;
                // The entering meter is still at its smaller entrance size.
                // Its bottom corner must nevertheless be below the avatar.
                assert!(midpoint.y > center_y + g.quota_diameter * 0.30);
                assert!((midpoint.x - g.logo.x).abs() < 8.0 * g.target_scale);
            }
        }
    }

    #[test]
    fn whole_path_is_continuous_at_logo_and_quota_stages() {
        for spatial in [true, false] {
            let mut g = fixture();
            if !spatial {
                g.source = g.logo;
            }
            for count in 1..=3 {
                for i in 0..count {
                    for step in 1..=1000 {
                        let p = step as f64 / 1000.0;
                        let before = g.points(count, i, p - 0.000001, 24.0);
                        let after = g.points(count, i, p, 24.0);
                        for (a, b) in before.into_iter().zip(after) {
                            assert!(
                                (a.x - b.x).hypot(a.y - b.y) < 0.1,
                                "Discontinuity at {p}, spatial={spatial}, row={i}"
                            );
                        }
                    }
                }
            }
        }
    }
}
