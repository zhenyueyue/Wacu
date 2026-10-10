#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub fn constrain(position: Point, size: (i32, i32), area: WorkArea) -> Point {
    Point {
        x: position
            .x
            .clamp(area.left, (area.right - size.0).max(area.left)),
        y: position
            .y
            .clamp(area.top, (area.bottom - size.1).max(area.top)),
    }
}

pub fn pinned(size: (i32, i32), area: WorkArea) -> Point {
    constrain(
        Point {
            x: area.right - size.0 - 15,
            y: area.top + ((area.bottom - area.top - size.1) / 2),
        },
        size,
        area,
    )
}

pub fn docked(size: (i32, i32), area: WorkArea, left: bool) -> Point {
    if !left {
        return pinned(size, area);
    }
    constrain(
        Point {
            x: area.left + 15,
            y: area.top + (area.bottom - area.top - size.1) / 2,
        },
        size,
        area,
    )
}

/// The visible surface starts 12px inside its transparent native window.
/// Leave a 6px grab target at the chosen monitor edge.
pub fn edge_position(
    position: Point,
    size: (i32, i32),
    area: WorkArea,
    scale: f32,
) -> Option<Point> {
    let margin = (12.0 * scale).round() as i32;
    let grab = (6.0 * scale).round() as i32;
    let threshold = (32.0 * scale).round() as i32;
    if position.x + margin <= area.left + threshold {
        Some(Point {
            x: area.left - size.0 + margin + grab,
            y: position.y,
        })
    } else if position.x + size.0 - margin >= area.right - threshold {
        Some(Point {
            x: area.right - margin - grab,
            y: position.y,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_position_stays_on_screen_after_resize() {
        let a = WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        assert_eq!(
            constrain(Point { x: 1900, y: 1060 }, (104, 440), a),
            Point { x: 1816, y: 600 }
        );
    }
    #[test]
    fn secondary_negative_coordinates() {
        let a = WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        assert_eq!(
            constrain(Point { x: -2500, y: 1300 }, (104, 440), a),
            Point { x: -1920, y: 600 }
        );
    }
    #[test]
    fn default_pinned_is_visible() {
        let a = WorkArea {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1030,
        };
        let pos = pinned((104, 440), a);
        assert!(pos.x >= 1920 && pos.x + 104 <= 3840);
        assert!(pos.y >= 0 && pos.y + 440 <= 1030);
    }

    #[test]
    fn edge_hidden_leaves_a_surface_grab_target() {
        let area = WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let size = (72, 244);
        let left = docked(size, area, true);
        let right = docked(size, area, false);
        let hidden_left = edge_position(left, size, area, 1.0).unwrap();
        let hidden_right = edge_position(right, size, area, 1.0).unwrap();
        assert_eq!(hidden_left.x + size.0 - 12, area.left + 6);
        assert_eq!(hidden_right.x + 12, area.right - 6);
        assert_eq!(hidden_left.y, left.y);
        assert_eq!(
            edge_position(Point { x: -900, y: 100 }, size, area, 1.0),
            None
        );
        let scaled = edge_position(Point { x: -160, y: 80 }, (144, 488), area, 2.0).unwrap();
        assert_eq!(scaled.x + 24, area.right - 12);
    }
}
