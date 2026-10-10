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

pub fn popup(anchor: Point, size: (i32, i32), area: WorkArea) -> Point {
    let above = anchor.y - size.1 - 12;
    let below = anchor.y + 16;
    let y = if above >= area.top { above } else { below };
    constrain(
        Point {
            x: anchor.x - size.0 / 2,
            y,
        },
        size,
        area,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_stays_on_screen() {
        let a = WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        assert_eq!(
            popup(Point { x: 1900, y: 1060 }, (104, 440), a),
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
}
