mod path_lerping;

use bevy::prelude::*;
use bevy_prototype_lyon::entity::Path as PathComponent;
use bevy_prototype_lyon::prelude::*;
use lerp::{num_traits::Float, Lerp};
use std::{
    cmp::Ordering,
    iter::{self, FromIterator},
    ops::{Add, Mul, RangeBounds, RangeInclusive, Sub},
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
        let (new_path, is_within_margin_of_error) =
            from.0.lerped(&to.target, to.lerp_t, to.margin_of_error);
        from.0 = new_path;
        if is_within_margin_of_error {
            lerp_events.send(LerpFinished(entity));
        }
    }
}

trait WithinMarginOfError<T, M> {
    fn is_within_margin_of_error(&self, target: &T, margin_of_error: &M) -> bool;
}

impl<S, T, M: PartialOrd<S> + PartialOrd<T>> WithinMarginOfError<T, M> for S {
    fn is_within_margin_of_error(&self, target: &T, margin_of_error: &M) -> bool {
        margin_of_error.partial_cmp(self) == margin_of_error.partial_cmp(target)
    }
}

//trait Lerpable<T = f32, M = f32>: Lerp<T> {
//    fn lerped(&self, target: &Self, t: &T, margin_of_error: &M) -> (bool, Self);
//}
//
//impl<S: WithinMarginOfError<S, M> + Add<Output = S> + Mul<T, Output = S>, T: Float, M>
//    Lerpable<T, M> for S
//{
//    fn lerped(&self, target: &Self, t: &T, margin_of_error: &M) -> (bool, Self) {
//        let result = self.lerp(*target, *t);
//        (
//            result.is_within_margin_of_error(target, margin_of_error),
//            result,
//        )
//    }
//}

//impl Lerpable for Path {
//    fn lerped(&self, target: &Self, t: f32, margin_of_error: f32) -> (Self, bool) {
//        match self.iter().count().cmp(&target.iter().count()) {
//            Ordering::Equal => path_lerping::lerp_equal_sides(self, target, t, margin_of_error),
//            Ordering::Less => path_lerping::lerp_less_sides(self, target, t, margin_of_error),
//            Ordering::Greater => path_lerping::lerp_greater_sides(self, target, t, margin_of_error),
//        }
//    }
//}
