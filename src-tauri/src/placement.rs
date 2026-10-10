//! Physical-pixel window placement, independent from the native window backend.
//! The work area excludes normal taskbars and handles negative virtual-desktop origins.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl Bounds {
    pub fn right(self) -> i32 {
        self.left.saturating_add(self.width)
    }

    pub fn bottom(self) -> i32 {
        self.top.saturating_add(self.height)
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.left
            && point.x < self.right()
            && point.y >= self.top
            && point.y < self.bottom()
    }
}

pub fn clamp_to_work_area(position: Point, window: (i32, i32), work: Bounds) -> Point {
    let right = work.right().saturating_sub(window.0).max(work.left);
    let bottom = work.bottom().saturating_sub(window.1).max(work.top);
    Point {
        x: position.x.clamp(work.left, right),
        y: position.y.clamp(work.top, bottom),
    }
}

pub fn popup_position(work: Bounds, window: (i32, i32), anchor: Option<Point>) -> Point {
    let margin = 9;
    let position = if let Some(anchor) = anchor {
        // Prefer above or below the tray edge; otherwise stay close to the pointer.
        let y = if anchor.y >= work.bottom() {
            work.bottom() - window.1 - margin
        } else if anchor.y < work.top {
            work.top + margin
        } else {
            anchor.y - window.1 / 2
        };
        Point { x: anchor.x - window.0 / 2, y }
    } else {
        Point {
            x: work.right() - window.0 - 18,
            y: work.bottom() - window.1 - 12,
        }
    };
    clamp_to_work_area(position, window, work)
}

pub fn default_pinned_position(work: Bounds, window: (i32, i32)) -> Point {
    clamp_to_work_area(
        Point {
            x: work.right() - window.0 - 18,
            y: work.top + (work.height - window.1) / 2,
        },
        window,
        work,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_tray_opens_above_work_area() {
        let work = Bounds { left: 0, top: 0, width: 1920, height: 1040 };
        let pos = popup_position(work, (114, 458), Some(Point { x: 1830, y: 1078 }));
        assert_eq!(pos, Point { x: 1773, y: 573 });
    }

    #[test]
    fn negative_monitor_coordinates_are_supported() {
        let work = Bounds { left: -1920, top: -20, width: 1920, height: 1080 };
        let pos = popup_position(work, (114, 458), Some(Point { x: -1910, y: 1070 }));
        assert_eq!(pos.x, -1920);
        assert!(pos.y >= -20 && pos.y + 458 <= 1060);
    }

    #[test]
    fn move_cannot_restore_outside_work_area() {
        let work = Bounds { left: 100, top: 60, width: 1200, height: 800 };
        assert_eq!(
            clamp_to_work_area(Point { x: 4500, y: -500 }, (114, 458), work),
            Point { x: 1186, y: 60 }
        );
    }

    #[test]
    fn window_larger_than_work_area_uses_top_left() {
        let work = Bounds { left: 200, top: 40, width: 70, height: 120 };
        assert_eq!(clamp_to_work_area(Point { x: 999, y: 999 }, (114, 458), work), Point { x: 200, y: 40 });
    }

    #[test]
    fn default_pinned_uses_available_area_not_full_monitor() {
        let work = Bounds { left: 0, top: 45, width: 2560, height: 1300 };
        let pos = default_pinned_position(work, (114, 458));
        assert!(work.contains(pos));
        assert!(pos.y + 458 <= work.bottom());
    }
}
