use bevy_prototype_lyon::prelude::*;
use std::{
    cmp::Ordering,
    iter::{self, FromIterator},
};
use tess::{
    math::Point,
    path::{Event, Path, PathEvent},
};

pub trait Lerp<T = Self, U = Self> {
    fn lerped(self, other: T, t: f32, p: f32) -> (bool, U);
}

impl Lerp for Point {
    fn lerped(self, other: Self, t: f32, p: f32) -> (bool, Self) {
        let result = self.lerp(other, t);
        if result.distance_to(other) > p {
            (false, result)
        } else {
            (true, other)
        }
    }
}

impl Lerp for PathEvent {
    fn lerped(self, mut other: Self, t: f32, p: f32) -> (bool, Self) {
        fn lerp_other(
            from_from: Point,
            from_to: Point,
            from_ctrl: Point,
            from_ctrl2: Point,
            other: PathEvent,
            t: f32,
            p: f32,
        ) -> (bool, PathEvent) {
            match other {
                Event::Begin { at } => {
                    let (snapped, at) = from_from.lerped(at, t, p);
                    (snapped, Event::Begin { at })
                }
                Event::Line { from, to } => {
                    let (from_snapped, from) = from_from.lerped(from, t, p);
                    let (to_snapped, to) = from_to.lerped(to, t, p);
                    (from_snapped && to_snapped, Event::Line { from, to })
                }
                Event::Quadratic { from, ctrl, to } => {
                    let (from_snapped, from) = from_from.lerped(from, t, p);
                    let (ctrl_snapped, ctrl) = from_ctrl.lerped(ctrl, t, p);
                    let (to_snapped, to) = from_to.lerped(to, t, p);
                    (
                        from_snapped && ctrl_snapped && to_snapped,
                        Event::Quadratic { from, ctrl, to },
                    )
                }
                Event::Cubic {
                    from,
                    ctrl1,
                    ctrl2,
                    to,
                } => {
                    let (from_snapped, from) = from_from.lerped(from, t, p);
                    let (ctrl1_snapped, ctrl1) = from_ctrl.lerped(ctrl1, t, p);
                    let (ctrl2_snapped, ctrl2) = from_ctrl2.lerped(ctrl2, t, p);
                    let (to_snapped, to) = from_to.lerped(to, t, p);
                    (
                        from_snapped && ctrl1_snapped && ctrl2_snapped && to_snapped,
                        Event::Cubic {
                            from,
                            ctrl1,
                            ctrl2,
                            to,
                        },
                    )
                }
                Event::End {
                    last,
                    first,
                    close: _,
                } => {
                    let (last_snapped, last) = from_from.lerped(last, t, p);
                    let (first_snapped, first) = from_to.lerped(first, t, p);
                    (
                        last_snapped && first_snapped,
                        Event::End {
                            last,
                            first,
                            close: true,
                        },
                    )
                }
            }
        }

        match self {
            Event::Begin { at } => lerp_other(at, at, at, at, other, t, p),
            Event::Line { from, to } => match other {
                Event::Begin { at } => {
                    let (from_snapped, from) = from.lerped(at, t, p);
                    let (to_snapped, to) = to.lerped(at, t, p);
                    (from_snapped && to_snapped, Event::Line { from, to })
                }
                _ => {
                    let midpoint = from.lerp(to, 0.5);
                    lerp_other(from, to, midpoint, midpoint, other, t, p)
                }
            },
            Event::Quadratic { from, ctrl, to } => match other {
                Event::Begin { at } => {
                    let (from_snapped, from) = from.lerped(at, t, p);
                    let (ctrl_snapped, ctrl) = ctrl.lerped(at, t, p);
                    let (to_snapped, to) = to.lerped(at, t, p);
                    (
                        from_snapped && ctrl_snapped && to_snapped,
                        Event::Quadratic { from, ctrl, to },
                    )
                }
                Event::Line {
                    from: other_from,
                    to: other_to,
                }
                | Event::End {
                    last: other_from,
                    first: other_to,
                    close: _,
                } => {
                    let (from_snapped, from) = from.lerped(other_from, t, p);
                    let (ctrl_snapped, ctrl) = ctrl.lerped(other_from.lerp(other_to, 0.5), t, p);
                    let (to_snapped, to) = to.lerped(other_to, t, p);
                    let all_snapped = from_snapped && ctrl_snapped && to_snapped;
                    if !all_snapped {
                        other = Event::Quadratic { from, ctrl, to };
                    }
                    (all_snapped, other)
                }
                _ => lerp_other(from, to, ctrl, ctrl, other, t, p),
            },
            Event::Cubic {
                from,
                ctrl1,
                ctrl2,
                to,
            } => match other {
                Event::Begin { at } => {
                    let (from_snapped, from) = from.lerped(at, t, p);
                    let (ctrl1_snapped, ctrl1) = ctrl1.lerped(at, t, p);
                    let (ctrl2_snapped, ctrl2) = ctrl2.lerped(at, t, p);
                    let (to_snapped, to) = to.lerped(at, t, p);
                    (
                        from_snapped && ctrl1_snapped && ctrl2_snapped && to_snapped,
                        Event::Cubic {
                            from,
                            ctrl1,
                            ctrl2,
                            to,
                        },
                    )
                }
                Event::Line {
                    from: other_from,
                    to: other_to,
                }
                | Event::End {
                    last: other_from,
                    first: other_to,
                    close: _,
                } => {
                    let (from_snapped, from) = from.lerped(other_from, t, p);
                    let midpoint = other_from.lerp(other_to, 0.5);
                    let (ctrl1_snapped, ctrl1) = ctrl1.lerped(midpoint, t, p);
                    let (ctrl2_snapped, ctrl2) = ctrl2.lerped(midpoint, t, p);
                    let (to_snapped, to) = to.lerped(other_to, t, p);
                    let all_snapped = from_snapped && ctrl1_snapped && ctrl2_snapped && to_snapped;
                    if !all_snapped {
                        other = Event::Cubic {
                            from,
                            ctrl1,
                            ctrl2,
                            to,
                        };
                    }
                    (all_snapped, other)
                }
                Event::Quadratic {
                    from: other_from,
                    ctrl,
                    to: other_to,
                } => {
                    let (from_snapped, from) = from.lerped(other_from, t, p);
                    let (ctrl1_snapped, ctrl1) = ctrl1.lerped(ctrl, t, p);
                    let (ctrl2_snapped, ctrl2) = ctrl2.lerped(ctrl, t, p);
                    let (to_snapped, to) = to.lerped(other_to, t, p);
                    let all_snapped = from_snapped && ctrl1_snapped && ctrl2_snapped && to_snapped;
                    if !all_snapped {
                        other = Event::Cubic {
                            from,
                            ctrl1,
                            ctrl2,
                            to,
                        };
                    }
                    (all_snapped, other)
                }
                _ => lerp_other(from, to, ctrl1, ctrl2, other, t, p),
            },
            Event::End { last, first, close } => match other {
                Event::Begin { at } => {
                    let (last_snapped, last) = last.lerped(at, t, p);
                    let (first_snapped, first) = first.lerped(at, t, p);
                    (
                        last_snapped && first_snapped,
                        Event::End { last, first, close },
                    )
                }
                _ => {
                    let midpoint = last.lerp(first, 0.5);
                    lerp_other(last, first, midpoint, midpoint, other, t, p)
                }
            },
        }
    }
}

impl Lerp<Self, Path> for &Path {
    fn lerped(self, other: Self, t: f32, p: f32) -> (bool, Path) {
        match self.iter().count().cmp(&other.iter().count()) {
            Ordering::Equal => lerp_equal_sides(self, other, t, p),
            Ordering::Less => lerp_less_sides(self, other, t, p),
            Ordering::Greater => lerp_greater_sides(self, other, t, p),
        }
    }
}

fn lerp_equal_sides<T, U>(from: T, to: U, t: f32, p: f32) -> (bool, Path)
where
    T: IntoIterator,
    U: IntoIterator,
    T::Item: Lerp<U::Item>,
    Path: FromIterator<T::Item>,
{
    let mut all_snapped = true;
    let result = from
        .into_iter()
        .zip(to)
        .map(|(from, to)| from.lerped(to, t, p))
        .inspect(|(snapped, _)| all_snapped &= snapped)
        .map(|(_, event)| event)
        .collect::<Path>();
    (all_snapped, result)
}

fn lerp_less_sides(from: &Path, to: &Path, t: f32, p: f32) -> (bool, Path) {
    let from_count = from.iter().count();
    let to_count = to.iter().count();
    assert!(from_count < to_count);
    lerp_equal_sides(
        iter::repeat(
            from.iter()
                .next()
                .unwrap_or_else(|| to.iter().next().unwrap()),
        )
        .take(to_count - from_count)
        .chain(from),
        to,
        t,
        p,
    )
}

fn lerp_greater_sides(from: &Path, to: &Path, t: f32, p: f32) -> (bool, Path) {
    let from_count = from.iter().count();
    let to_count = to.iter().count();
    assert!(from_count > to_count);
    let (all_snapped, mut result) = lerp_equal_sides(
        from,
        iter::repeat(
            to.iter()
                .next()
                .unwrap_or_else(|| from.iter().next().unwrap()),
        )
        .take(from_count - to_count)
        .chain(to),
        t,
        p,
    );
    if all_snapped {
        result = to.clone();
    }
    (all_snapped, result)
}
