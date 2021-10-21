use bevy::prelude::*;
use bevy_prototype_lyon::entity::Path as PathComponent;
use bevy_prototype_lyon::prelude::*;
use lerp::Lerp;
use std::{
    cmp::Ordering,
    iter::{self, FromIterator},
    ops::{Add, RangeBounds, RangeInclusive, Sub},
};
use tess::{
    geom::euclid::default::Point2D,
    path::{path::Builder, Event, Path},
};

enum Direction {
    Increasing,
    Decreasing,
}

impl Direction {
    fn inverted(&self) -> Self {
        match self {
            Direction::Increasing => Direction::Decreasing,
            Direction::Decreasing => Direction::Increasing,
        }
    }

    fn invert(&mut self) {
        *self = self.inverted();
    }

    fn get_operation<T: Add<Rhs, Output = Output> + Sub<Rhs, Output = Output>, Rhs, Output>(
        &self,
    ) -> &dyn Fn(T, Rhs) -> Output {
        match self {
            Direction::Increasing => &Add::add,
            Direction::Decreasing => &Sub::sub,
        }
    }
}

#[derive(Component)]
struct SidesChangingShape<T: RangeBounds<u8>> {
    sides: u8,
    bounds: T,
    direction: Direction,
}

impl<T: RangeBounds<u8>> SidesChangingShape<T> {
    fn increment_sides(&mut self) {
        let op = self.direction.get_operation();
        let new_sides = op(self.sides, 1);
        if self.bounds.contains(&new_sides) {
            self.sides = new_sides;
        } else {
            self.direction.invert();
            self.increment_sides();
        }
    }
}

#[derive(Component)]
struct LerpingShape {
    target: Path,
    lerp_t: f32,
    margin_of_error: f32,
}

// Event for when all points of a LerpingShape are within the margin-of-error of the target path
struct LerpFinished(Entity);

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, SystemLabel)]
enum System {
    ChangeSides,
    UpdateLerpTarget,
    LerpShape,
}

fn main() {
    App::new()
        .insert_resource(Msaa { samples: 8 })
        .add_plugins(DefaultPlugins)
        .add_plugin(ShapePlugin)
        .add_startup_system(setup)
        .add_event::<LerpFinished>()
        .add_system(change_sides::<RangeInclusive<u8>>.label(System::ChangeSides))
        .add_system(
            update_lerp_target::<RangeInclusive<u8>>
                .label(System::UpdateLerpTarget)
                .after(System::ChangeSides),
        )
        .add_system(
            lerp_shape
                .label(System::LerpShape)
                .after(System::UpdateLerpTarget),
        )
        .run();
}

fn setup(mut commands: Commands) {
    const SIDES: u8 = 5;

    let shape = shapes::RegularPolygon {
        sides: SIDES as usize,
        feature: shapes::RegularPolygonFeature::Radius(200.0),
        ..Default::default()
    };

    commands.spawn_bundle(OrthographicCameraBundle::new_2d());
    commands
        .spawn_bundle(GeometryBuilder::build_as(
            &shape,
            DrawMode::Outlined {
                fill_mode: FillMode::color(Color::ORANGE),
                outline_mode: StrokeMode::new(Color::ORANGE_RED, 8.0),
            },
            Transform::default(),
        ))
        .insert(SidesChangingShape {
            sides: SIDES,
            bounds: 3..=8,
            direction: Direction::Increasing,
        })
        .insert(LerpingShape {
            target: ShapePath::build_as(&shape).0,
            lerp_t: 0.1,
            margin_of_error: 1.0,
        });
}

fn change_sides<T: RangeBounds<u8> + 'static + Send + Sync>(
    mut lerp_events: EventReader<LerpFinished>,
    mut query: Query<&mut SidesChangingShape<T>>,
) {
    for LerpFinished(entity) in lerp_events.iter() {
        if let Ok(mut sides) = query.get_mut(*entity) {
            sides.increment_sides();
        }
    }
}

fn update_lerp_target<T: RangeBounds<u8> + 'static + Send + Sync>(
    mut query: Query<(&SidesChangingShape<T>, &mut LerpingShape), Changed<SidesChangingShape<T>>>,
) {
    for (sides, mut shape) in query.iter_mut() {
        shape.target = ShapePath::build_as(&shapes::RegularPolygon {
            sides: sides.sides as usize,
            feature: shapes::RegularPolygonFeature::Radius(200.0),
            ..Default::default()
        })
        .0;
    }
}

fn lerp_shape(
    mut lerp_events: EventWriter<LerpFinished>,
    mut query: Query<(Entity, &mut PathComponent, &LerpingShape)>,
) {
    for (entity, mut from, to) in query.iter_mut() {
        let (new_path, result) = from.0.lerp(&to.target, to.lerp_t, to.margin_of_error);
        from.0 = new_path;
        if let LerpResult::WithinMarginOfError = result {
            lerp_events.send(LerpFinished(entity));
        }
    }
}

pub enum LerpResult {
    OutsideMarginOfError,
    WithinMarginOfError,
}

trait Lerpable<T = Self> {
    type Output;
    fn lerp(&self, target: &T, t: f32, margin_of_error: f32) -> (Self::Output, LerpResult);
}

impl Lerpable for Path {
    type Output = Self;
    fn lerp(&self, target: &Self, t: f32, margin_of_error: f32) -> (Self, LerpResult) {
        match self.iter().count().cmp(&target.iter().count()) {
            Ordering::Equal => path_lerping::lerp_equal_sides(self, target, t, margin_of_error),
            Ordering::Less => path_lerping::lerp_less_sides(self, target, t, margin_of_error),
            Ordering::Greater => path_lerping::lerp_greater_sides(self, target, t, margin_of_error),
        }
    }
}

mod path_lerping {
    use super::*;
    pub fn lerp_equal_sides(
        from: &Path,
        to: &Path,
        t: f32,
        margin_of_error: f32,
    ) -> (Path, LerpResult) {
        fn check_if_within_margin_of_error(
            from: Point2D<f32>,
            to: Point2D<f32>,
            margin_of_error: f32,
            out: &mut LerpResult,
        ) {
            if ((from.x - to.x).abs() > margin_of_error)
                || ((from.y - to.y).abs() > margin_of_error)
            {
                *out = LerpResult::OutsideMarginOfError;
            }
        }

        let count = from.iter().count();
        assert!(count == to.iter().count());

        let mut all_within_margin_of_error = LerpResult::WithinMarginOfError;

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
        builder.concatenate(&[Path::from_iter(parts).as_slice()]);
        (builder.build(), all_within_margin_of_error)
    }

    pub fn lerp_less_sides(
        from: &Path,
        to: &Path,
        t: f32,
        margin_of_error: f32,
    ) -> (Path, LerpResult) {
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
            builder.concatenate(&[Path::from_iter(parts).as_slice()]);
            lerp_equal_sides(&builder.build(), to, t, margin_of_error)
        } else {
            unreachable!()
        }
    }

    pub fn lerp_greater_sides(
        from: &Path,
        to: &Path,
        t: f32,
        margin_of_error: f32,
    ) -> (Path, LerpResult) {
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
        builder.concatenate(&[Path::from_iter(parts).as_slice()]);

        let (mut lerped, result) = lerp_equal_sides(from, &builder.build(), t, margin_of_error);
        if let LerpResult::WithinMarginOfError = result {
            let remove_index = from_count / 2;

            let parts = lerped
                .iter()
                .take(remove_index)
                .chain(lerped.iter().skip(remove_index + diff));

            let mut builder = Builder::with_capacity(to_count * 2 - 1, to_count);
            builder.concatenate(&[Path::from_iter(parts).as_slice()]);
            lerped = builder.build();
        }
        (lerped, result)
    }
}
