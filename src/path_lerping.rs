use bevy_prototype_lyon::prelude::*;
//use lerp::Lerp;
use std::{
    iter,
    ops::{Add, Mul, Sub},
};
use tess::{
    geom::{
        euclid::{default::Point2D, num::One},
        traits::Transformation,
        Vector,
    },
    path::{path::Builder, Event, Path},
};

pub struct Lerp2D<T> {
    pub x: T,
    pub y: T,
    pub t: T,
}

impl<T> Transformation<T> for Lerp2D<T>
where
    T: Copy + One + Sub<Output = T> + Add<Output = T> + Mul<Output = T>,
{
    fn transform_point(&self, p: Point2D<T>) -> Point2D<T> {
        p.lerp((self.x, self.y).into(), self.t)
    }

    fn transform_vector(&self, v: Vector<T>) -> Vector<T> {
        v.lerp((self.x, self.y).into(), self.t)
    }
}

pub struct PathLerp {}

pub fn lerp_equal_sides(from: &Path, to: &Path, t: f32, margin_of_error: f32) -> (bool, Path) {
    fn check_if_within_margin_of_error(
        from: Point2D<f32>,
        to: Point2D<f32>,
        margin_of_error: f32,
        out: &mut bool,
    ) {
        if ((from.x - to.x).abs() > margin_of_error) || ((from.y - to.y).abs() > margin_of_error) {
            *out = false;
        }
    }

    from.transformed(PathLerp::from(to));

    let count = from.iter().count();
    assert!(count == to.iter().count());

    let mut all_within_margin_of_error = true;

    let parts =
        from.iter()
            .zip(to.iter())
            .map(|(from_event, to_event)| match (from_event, to_event) {
                (Event::Begin { at: from_at }, Event::Begin { at: to_at }) => {
                    let at = Point2D::new(
                        Lerp::lerp(from_at.x, to_at.x, t),
                        Lerp::lerp(from_at.y, to_at.y, t),
                    );
                    check_if_within_margin_of_error(
                        at,
                        to_at,
                        margin_of_error,
                        &mut all_within_margin_of_error,
                    );
                    Event::Begin { at }
                }
                (
                    Event::Line {
                        from: from_from,
                        to: from_to,
                    },
                    Event::Line {
                        from: to_from,
                        to: to_to,
                    },
                ) => {
                    let from = Point2D::new(
                        Lerp::lerp(from_from.x, to_from.x, t),
                        Lerp::lerp(from_from.y, to_from.y, t),
                    );
                    let to = Point2D::new(
                        Lerp::lerp(from_to.x, to_to.x, t),
                        Lerp::lerp(from_to.y, to_to.y, t),
                    );
                    check_if_within_margin_of_error(
                        from,
                        to_from,
                        margin_of_error,
                        &mut all_within_margin_of_error,
                    );
                    check_if_within_margin_of_error(
                        to,
                        to_to,
                        margin_of_error,
                        &mut all_within_margin_of_error,
                    );
                    Event::Line { from, to }
                }
                (
                    Event::End {
                        last: from_last,
                        first: from_first,
                        ..
                    },
                    Event::End {
                        last: to_last,
                        first: to_first,
                        ..
                    },
                ) => {
                    let last = Point2D::new(
                        Lerp::lerp(from_last.x, to_last.x, t),
                        Lerp::lerp(from_last.y, to_last.y, t),
                    );
                    let first = Point2D::new(
                        Lerp::lerp(from_first.x, to_first.x, t),
                        Lerp::lerp(from_first.y, to_first.y, t),
                    );
                    check_if_within_margin_of_error(
                        last,
                        to_last,
                        margin_of_error,
                        &mut all_within_margin_of_error,
                    );
                    check_if_within_margin_of_error(
                        first,
                        to_first,
                        margin_of_error,
                        &mut all_within_margin_of_error,
                    );
                    Event::End {
                        last,
                        first,
                        close: true,
                    }
                }
                _ => unreachable!(),
            });

    let mut builder = Builder::with_capacity(count * 2 - 1, count);
    builder.concatenate(&[parts.collect::<Path>().as_slice()]);
    (all_within_margin_of_error, builder.build())
}

pub fn lerp_less_sides(from: &Path, to: &Path, t: f32, margin_of_error: f32) -> (bool, Path) {
    let from_count = from.iter().count();
    let to_count = to.iter().count();
    assert!(from_count < to_count);

    let insert_index = from_count / 2;
    if let Event::Line { to: duplicated, .. } = from.iter().nth(insert_index).unwrap() {
        let diff = to_count - from_count;
        let parts = from
            .iter()
            .take(insert_index)
            .chain(
                iter::repeat(Event::Line {
                    from: duplicated,
                    to: duplicated,
                })
                .take(diff),
            )
            .chain(from.iter().skip(insert_index));

        let mut builder = Builder::with_capacity(to_count * 2 - 1, to_count);
        builder.concatenate(&[parts.collect::<Path>().as_slice()]);
        lerp_equal_sides(&builder.build(), to, t, margin_of_error)
    } else {
        unreachable!()
    }
}

pub fn lerp_greater_sides(from: &Path, to: &Path, t: f32, margin_of_error: f32) -> (bool, Path) {
    let from_count = from.iter().count();
    let to_count = to.iter().count();
    assert!(from_count > to_count);

    let diff = from_count - to_count;
    let insert_index = to_count / 2;
    let duplicated = to.iter().nth(insert_index).unwrap();
    let parts = to
        .iter()
        .take(insert_index)
        .chain(iter::repeat(duplicated).take(diff))
        .chain(to.iter().skip(insert_index));

    let mut builder = Builder::with_capacity(from_count * 2 - 1, from_count);
    builder.concatenate(&[parts.collect::<Path>().as_slice()]);

    let (is_within_margin_of_error, mut lerped) =
        lerp_equal_sides(from, &builder.build(), t, margin_of_error);
    if is_within_margin_of_error {
        let remove_index = from_count / 2;

        let parts = lerped
            .iter()
            .take(remove_index)
            .chain(lerped.iter().skip(remove_index + diff));

        let mut builder = Builder::with_capacity(to_count * 2 - 1, to_count);
        builder.concatenate(&[parts.collect::<Path>().as_slice()]);
        lerped = builder.build();
    }
    (is_within_margin_of_error, lerped)
}
