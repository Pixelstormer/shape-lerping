use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use lerp::Lerp;
use std::iter::{self, FromIterator};
use tess::path::{path::Builder, Event};

#[derive(Component)]
struct SidesChangingShape(isize);

#[derive(Component)]
struct SidesRange(isize, isize);

#[derive(Component)]
struct LerpingShape(Path, f32, f32);

struct LerpFinished(Entity);

fn main() {
    App::new()
        .insert_resource(Msaa { samples: 8 })
        .add_plugins(DefaultPlugins)
        .add_plugin(ShapePlugin)
        .add_startup_system(setup.system())
        .add_event::<LerpFinished>()
        .add_system(change_sides.system().label("change_sides"))
        .add_system(
            update_lerp_target
                .system()
                .label("update_lerp_target")
                .after("change_sides"),
        )
        .add_system(
            lerp_shape
                .system()
                .label("lerp_shape")
                .after("update_lerp_target"),
        )
        .run();
}

fn setup(mut commands: Commands) {
    const SIDES: u8 = 6;

    let shape = shapes::RegularPolygon {
        sides: SIDES as usize,
        feature: shapes::RegularPolygonFeature::Radius(200.0),
        ..shapes::RegularPolygon::default()
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
        .insert(SidesChangingShape(SIDES as isize))
        .insert(SidesRange(3, 8))
        .insert(LerpingShape(ShapePath::build_as(&shape), 0.1, 1.0));
}

fn change_sides(
    mut lerp_events: EventReader<LerpFinished>,
    mut query: Query<(&mut SidesChangingShape, &SidesRange)>,
) {
    for LerpFinished(entity) in lerp_events.iter() {
        if let Ok((mut sides, SidesRange(min, max))) = query.get_mut(*entity) {
            let (min, max) = (*min, *max);
            if sides.0 == max {
                sides.0 = -(max - 1);
            } else if sides.0 == -min {
                sides.0 = min + 1;
            } else if (sides.0 >= min && sides.0 < max) || (sides.0 >= -max && sides.0 < -min) {
                sides.0 += 1;
            } else {
                sides.0 = min;
            }
        }
    }
}

fn update_lerp_target(
    mut query: Query<(&SidesChangingShape, &mut LerpingShape), Changed<SidesChangingShape>>,
) {
    for (sides, mut shape) in query.iter_mut() {
        shape.0 = ShapePath::build_as(&shapes::RegularPolygon {
            sides: sides.0.abs() as usize,
            feature: shapes::RegularPolygonFeature::Radius(200.0),
            ..Default::default()
        });
    }
}

fn lerp_shape(
    mut lerp_events: EventWriter<LerpFinished>,
    mut query: Query<(Entity, &mut Path, &LerpingShape)>,
) {
    for (entity, mut path, target) in query.iter_mut() {
        let t = target.1;
        let precision = target.2;
        let target = &target.0;

        let count = path.0.iter().count();
        let target_count = target.0.iter().count();

        let diff = target_count as isize - count as isize;
        if match diff.signum() {
            0 => path.lerp_same_sides(target, count, t, precision),
            1 => path.lerp_greater_sides(target, count, target_count, t, precision),
            -1 => path.lerp_less_sides(target, count, target_count, t, precision),
            _ => unreachable!(),
        } {
            lerp_events.send(LerpFinished(entity));
        }
    }
}

trait Lerpable {
    fn lerp_same_sides(&mut self, target: &Path, count: usize, t: f32, precision: f32) -> bool;
    fn lerp_greater_sides(
        &mut self,
        target: &Path,
        self_count: usize,
        target_count: usize,
        t: f32,
        precision: f32,
    ) -> bool;
    fn lerp_less_sides(
        &mut self,
        target: &Path,
        self_count: usize,
        target_count: usize,
        t: f32,
        precision: f32,
    ) -> bool;
}

impl Lerpable for Path {
    fn lerp_same_sides(&mut self, target: &Path, count: usize, t: f32, precision: f32) -> bool {
        use tess::{geom::euclid::default::Point2D, path::Path};

        fn check_if_unfinished(
            result: Point2D<f32>,
            target: Point2D<f32>,
            precision: f32,
            all_finished: &mut bool,
        ) {
            if ((result.x - target.x).abs() > precision)
                || ((result.y - target.y).abs() > precision)
            {
                *all_finished = false;
            }
        }

        let all_finished: &mut bool = &mut true;

        let parts = self
            .0
            .iter()
            .zip(target.0.iter())
            .map(
                |(self_event, target_event)| match (self_event, target_event) {
                    (Event::Begin { at: self_at }, Event::Begin { at: target_at }) => {
                        let at = Point2D::new(
                            Lerp::lerp(self_at.x, target_at.x, t),
                            Lerp::lerp(self_at.y, target_at.y, t),
                        );
                        check_if_unfinished(at, target_at, precision, all_finished);
                        Event::Begin { at }
                    }
                    (
                        Event::Line {
                            from: self_from,
                            to: self_to,
                        },
                        Event::Line {
                            from: target_from,
                            to: target_to,
                        },
                    ) => {
                        let from = Point2D::new(
                            Lerp::lerp(self_from.x, target_from.x, t),
                            Lerp::lerp(self_from.y, target_from.y, t),
                        );
                        let to = Point2D::new(
                            Lerp::lerp(self_to.x, target_to.x, t),
                            Lerp::lerp(self_to.y, target_to.y, t),
                        );
                        check_if_unfinished(from, target_from, precision, all_finished);
                        check_if_unfinished(to, target_to, precision, all_finished);
                        Event::Line { from, to }
                    }
                    (
                        Event::End {
                            last: self_last,
                            first: self_first,
                            ..
                        },
                        Event::End {
                            last: target_last,
                            first: target_first,
                            ..
                        },
                    ) => {
                        let last = Point2D::new(
                            Lerp::lerp(self_last.x, target_last.x, t),
                            Lerp::lerp(self_last.y, target_last.y, t),
                        );
                        let first = Point2D::new(
                            Lerp::lerp(self_first.x, target_first.x, t),
                            Lerp::lerp(self_first.y, target_first.y, t),
                        );
                        check_if_unfinished(last, target_last, precision, all_finished);
                        check_if_unfinished(first, target_first, precision, all_finished);
                        Event::End {
                            last,
                            first,
                            close: true,
                        }
                    }
                    _ => unreachable!(),
                },
            );

        let mut builder = Builder::with_capacity(count * 2 - 1, count);
        builder.concatenate(&[Path::from_iter(parts).as_slice()]);
        self.0 = builder.build();

        *all_finished
    }

    fn lerp_greater_sides(
        &mut self,
        target: &Path,
        self_count: usize,
        target_count: usize,
        t: f32,
        precision: f32,
    ) -> bool {
        use tess::path::Path;
        let insert_index = self_count / 2;

        if let Event::Line { to, .. } = self.0.iter().nth(insert_index).unwrap() {
            let diff = target_count - self_count;
            let parts = self
                .0
                .iter()
                .take(insert_index)
                .chain(iter::repeat(Event::Line { from: to, to }).take(diff))
                .chain(self.0.iter().skip(insert_index));

            let mut builder = Builder::with_capacity(target_count * 2 - 1, target_count);
            builder.concatenate(&[Path::from_iter(parts).as_slice()]);
            self.0 = builder.build();

            self.lerp_same_sides(target, target_count, t, precision)
        } else {
            unreachable!()
        }
    }

    fn lerp_less_sides(
        &mut self,
        target: &Path,
        self_count: usize,
        target_count: usize,
        t: f32,
        precision: f32,
    ) -> bool {
        use tess::path::Path;

        let diff = self_count - target_count;

        let insert_index = target_count / 2;
        let duplicated = target.0.iter().nth(insert_index).unwrap();
        let parts = target
            .0
            .iter()
            .take(insert_index)
            .chain(iter::repeat(duplicated).take(diff))
            .chain(target.0.iter().skip(insert_index));

        let mut builder = Builder::with_capacity(self_count * 2 - 1, self_count);
        builder.concatenate(&[Path::from_iter(parts).as_slice()]);
        let is_finished = self.lerp_same_sides(&Path(builder.build()), self_count, t, precision);
        if is_finished {
            let remove_index = self_count / 2;

            let parts = self
                .0
                .iter()
                .take(remove_index)
                .chain(self.0.iter().skip(remove_index + diff));

            let mut builder = Builder::with_capacity(target_count * 2 - 1, target_count);
            builder.concatenate(&[Path::from_iter(parts).as_slice()]);
            self.0 = builder.build();
        }
        is_finished
    }
}
