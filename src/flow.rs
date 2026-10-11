//! Physical desktop geometry for the approved ring -> line -> Logo handoff.
use std::f64::consts::PI;

pub const OPEN_MS: u64 = 1080;
pub const CLOSE_MS: u64 = 930;
pub const LOCAL_OPEN_MS: u64 = 630;
pub const LOCAL_CLOSE_MS: u64 = 480;

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
            (self.source.y.max(self.logo.y) + margin - y).ceil() as u32,
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

    pub fn opacity(self, count: usize, index: usize, progress: f64) -> f32 {
        let q = ((progress * 720.0 - index as f64 * 18.0) / 330.0).clamp(0.0, 1.0);
        let stagger = 0.12 * index as f64 / count.saturating_sub(1).max(1) as f64;
        let travel = smooth((self.rail(progress) - 0.10 - stagger) / 0.46);
        ((q * 18.0).min(1.0) * (1.0 - smooth(travel * 2.5))) as f32
    }

    pub fn points(self, count: usize, index: usize, progress: f64) -> Vec<Vector> {
        let q = ((progress * 720.0 - index as f64 * 18.0) / 330.0).clamp(0.0, 1.0);
        self.phase_points(count, index, q)
    }

    fn phase_points(self, count: usize, index: usize, q: f64) -> Vec<Vector> {
        let count = count.max(1);
        let start = -PI / 2.0 + index as f64 * PI * 2.0 / count as f64;
        let span = PI * 1.6 / count as f64;
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
                    let from = Vector {
                        x: self.source.x + 29.0 * self.source_scale * angle.cos(),
                        y: self.source.y + 29.0 * self.source_scale * angle.sin(),
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
        }
    }
    #[test]
    fn rings_land_on_logo_not_quota_positions() {
        let g = fixture();
        for count in 1..=3 {
            for i in 0..count {
                for p in g.phase_points(count, i, 1.0) {
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
        let centers: Vec<_> = (0..3).map(|i| g.phase_points(3, i, 0.5)[32]).collect();
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
                    for p in g.points(3, i, step as f64 / 100.0) {
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
                let before = g.phase_points(3, i, boundary - 0.00001);
                let after = g.phase_points(3, i, boundary + 0.00001);
                for (a, b) in before.into_iter().zip(after) {
                    assert!((a.x - b.x).hypot(a.y - b.y) < 0.1);
                }
            }
        }
    }
}
