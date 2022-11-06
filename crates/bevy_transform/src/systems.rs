use crate::components::{GlobalTransform, Transform};
use bevy_ecs::prelude::{Changed, Entity, Query, With, Without};
use bevy_hierarchy::{Children, Parent};

/// Update [`GlobalTransform`] component of entities that aren't in the hierarchy
pub fn sync_simple_transforms(
    mut query: Query<
        (&Transform, &mut GlobalTransform),
        (Changed<Transform>, Without<Parent>, Without<Children>),
    >,
) {
    for (transform, mut global_transform) in query.iter_mut() {
        *global_transform = GlobalTransform::from(*transform);
    }
}

/// Update [`GlobalTransform`] component of entities based on entity hierarchy and
/// [`Transform`] component.
pub fn propagate_transforms(
    mut root_query: Query<
        (
            Entity,
            &Children,
            &Transform,
            Changed<Transform>,
            Changed<Children>,
            &mut GlobalTransform,
        ),
        Without<Parent>,
    >,
    transform_query: Query<(&Transform, Changed<Transform>, &mut GlobalTransform), With<Parent>>,
    parent_query: Query<&Parent>,
    children_query: Query<(&Children, Changed<Children>), (With<Parent>, With<GlobalTransform>)>,
) {
    root_query.par_for_each_mut(
        // The differing depths and sizes of hierarchy trees causes the work for each root to be
        // different. A batch size of 1 ensures that each tree gets it's own task and multiple
        // large trees are not clumped together.
        1,
        |(entity, children, transform, mut changed, children_changed, mut global_transform)| {
            if changed {
                *global_transform = GlobalTransform::from(*transform);
            }

            // If our `Children` has changed, we need to recalculate everything below us
            changed |= children_changed;

            for child in children.iter() {
                let _ = propagate_recursive(
                    &global_transform,
                    &transform_query,
                    &parent_query,
                    &children_query,
                    entity,
                    *child,
                    changed,
                );
            }
        },
    );
}

fn propagate_recursive(
    parent: &GlobalTransform,
    unsafe_transform_query: &Query<
        (&Transform, Changed<Transform>, &mut GlobalTransform),
        With<Parent>,
    >,
    parent_query: &Query<&Parent>,
    children_query: &Query<(&Children, Changed<Children>), (With<Parent>, With<GlobalTransform>)>,
    expected_parent: Entity,
    entity: Entity,
    mut changed: bool,
    // We use a result here to use the `?` operator. Ideally we'd use a try block instead
) -> Result<(), ()> {
    if let Ok(actual_parent) = parent_query.get(entity) {
        assert_eq!(
            actual_parent.get(), expected_parent,
            "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
        );
    } else {
        panic!("Propagated child for {:?} has no Parent component!", entity);
    }

    // SAFE: With the check that each child has one and only one parent, each child must be globally unique within the
    // hierarchy. Because of this, it is impossible for this query to have aliased mutable access to the same GlobalTransform.
    // Any malformed hierarchy will cause a panic due to the above check.
    let global_matrix = unsafe {
        let (transform, transform_changed, mut global_transform) =
            unsafe_transform_query.get_unchecked(entity).map_err(drop)?;

        changed |= transform_changed;
        if changed {
            *global_transform = parent.mul_transform(*transform);
        }
        *global_transform
    };

    let (children, changed_children) = children_query.get(entity).map_err(drop)?;
    // If our `Children` has changed, we need to recalculate everything below us
    changed |= changed_children;
    for child in children {
        let _ = propagate_recursive(
            &global_matrix,
            unsafe_transform_query,
            parent_query,
            children_query,
            entity,
            *child,
            changed,
        );
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use bevy_app::prelude::*;
    use bevy_ecs::prelude::*;
    use bevy_ecs::system::CommandQueue;
    use bevy_math::vec3;
    use bevy_tasks::{ComputeTaskPool, TaskPool};

    use crate::components::{GlobalTransform, Transform};
    use crate::systems::*;
    use crate::TransformBundle;
    use bevy_hierarchy::{BuildChildren, BuildWorldChildren, Children, Parent};

    #[derive(StageLabel)]
    struct Update;

    #[test]
    fn did_propagate() {
        let mut world = World::default();
        world.insert_resource(ComputeTaskPool(TaskPool::default()));

        let mut update_stage = SystemStage::parallel();
        update_stage.add_system(sync_simple_transforms);
        update_stage.add_system(propagate_transforms);

        let mut schedule = Schedule::default();
        schedule.add_stage(Update, update_stage);

        // Root entity
        world.spawn(TransformBundle::from(Transform::from_xyz(1.0, 0.0, 0.0)));

        let mut children = Vec::new();
        world
            .spawn(TransformBundle::from(Transform::from_xyz(1.0, 0.0, 0.0)))
            .with_children(|parent| {
                children.push(
                    parent
                        .spawn(TransformBundle::from(Transform::from_xyz(0.0, 2.0, 0.)))
                        .id(),
                );
                children.push(
                    parent
                        .spawn(TransformBundle::from(Transform::from_xyz(0.0, 0.0, 3.)))
                        .id(),
                );
            });
        schedule.run(&mut world);

        assert_eq!(
            *world.get::<GlobalTransform>(children[0]).unwrap(),
            GlobalTransform::from_xyz(1.0, 0.0, 0.0) * Transform::from_xyz(0.0, 2.0, 0.0)
        );

        assert_eq!(
            *world.get::<GlobalTransform>(children[1]).unwrap(),
            GlobalTransform::from_xyz(1.0, 0.0, 0.0) * Transform::from_xyz(0.0, 0.0, 3.0)
        );
    }

    #[test]
    fn did_propagate_command_buffer() {
        let mut world = World::default();

        let mut update_stage = SystemStage::parallel();
        update_stage.add_system(sync_simple_transforms);
        update_stage.add_system(propagate_transforms);

        let mut schedule = Schedule::default();
        schedule.add_stage(Update, update_stage);

        // Root entity
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut children = Vec::new();
        commands
            .spawn(TransformBundle::from(Transform::from_xyz(1.0, 0.0, 0.0)))
            .with_children(|parent| {
                children.push(
                    parent
                        .spawn(TransformBundle::from(Transform::from_xyz(0.0, 2.0, 0.0)))
                        .id(),
                );
                children.push(
                    parent
                        .spawn(TransformBundle::from(Transform::from_xyz(0.0, 0.0, 3.0)))
                        .id(),
                );
            });
        queue.apply(&mut world);
        schedule.run(&mut world);

        assert_eq!(
            *world.get::<GlobalTransform>(children[0]).unwrap(),
            GlobalTransform::from_xyz(1.0, 0.0, 0.0) * Transform::from_xyz(0.0, 2.0, 0.0)
        );

        assert_eq!(
            *world.get::<GlobalTransform>(children[1]).unwrap(),
            GlobalTransform::from_xyz(1.0, 0.0, 0.0) * Transform::from_xyz(0.0, 0.0, 3.0)
        );
    }

    #[test]
    fn correct_children() {
        let mut world = World::default();
        world.insert_resource(ComputeTaskPool(TaskPool::default()));

        let mut update_stage = SystemStage::parallel();
        update_stage.add_system(sync_simple_transforms);
        update_stage.add_system(propagate_transforms);

        let mut schedule = Schedule::default();
        schedule.add_stage(Update, update_stage);

        // Add parent entities
        let mut children = Vec::new();
        let parent = {
            let mut command_queue = CommandQueue::default();
            let mut commands = Commands::new(&mut command_queue, &world);
            let parent = commands.spawn(Transform::from_xyz(1.0, 0.0, 0.0)).id();
            commands.entity(parent).with_children(|parent| {
                children.push(parent.spawn(Transform::from_xyz(0.0, 2.0, 0.0)).id());
                children.push(parent.spawn(Transform::from_xyz(0.0, 3.0, 0.0)).id());
            });
            command_queue.apply(&mut world);
            schedule.run(&mut world);
            parent
        };

        assert_eq!(
            world
                .get::<Children>(parent)
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            children,
        );

        // Parent `e1` to `e2`.
        {
            let mut command_queue = CommandQueue::default();
            let mut commands = Commands::new(&mut command_queue, &world);
            commands.entity(children[1]).add_child(children[0]);
            command_queue.apply(&mut world);
            schedule.run(&mut world);
        }

        assert_eq!(
            world
                .get::<Children>(parent)
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![children[1]]
        );

        assert_eq!(
            world
                .get::<Children>(children[1])
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![children[0]]
        );

        assert!(world.despawn(children[0]));

        schedule.run(&mut world);

        assert_eq!(
            world
                .get::<Children>(parent)
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![children[1]]
        );
    }

    #[test]
    fn correct_transforms_when_no_children() {
        let mut app = App::new();
        app.insert_resource(ComputeTaskPool(TaskPool::default()));

        app.add_system(sync_simple_transforms);
        app.add_system(propagate_transforms);

        let translation = vec3(1.0, 0.0, 0.0);

        // These will be overwritten.
        let mut child = Entity::from_raw(0);
        let mut grandchild = Entity::from_raw(1);
        let parent = app
            .world
            .spawn((
                Transform::from_translation(translation),
                GlobalTransform::IDENTITY,
            ))
            .with_children(|builder| {
                child = builder
                    .spawn(TransformBundle::IDENTITY)
                    .with_children(|builder| {
                        grandchild = builder.spawn(TransformBundle::IDENTITY).id();
                    })
                    .id();
            })
            .id();

        app.update();

        // check the `Children` structure is spawned
        assert_eq!(&**app.world.get::<Children>(parent).unwrap(), &[child]);
        assert_eq!(&**app.world.get::<Children>(child).unwrap(), &[grandchild]);
        // Note that at this point, the `GlobalTransform`s will not have updated yet, due to `Commands` delay
        app.update();

        let mut state = app.world.query::<&GlobalTransform>();
        for global in state.iter(&app.world) {
            assert_eq!(global, &GlobalTransform::from_translation(translation));
        }
    }

    #[test]
    #[should_panic]
    fn panic_when_hierarchy_cycle() {
        // We cannot directly edit Parent and Children, so we use a temp world to break
        // the hierarchy's invariants.
        let mut temp = World::new();
        let mut app = App::new();
        app.insert_resource(ComputeTaskPool(TaskPool::default()));

        // FIXME: Parallel executors seem to have some odd interaction with the other
        // tests in this crate. Using single_threaded until a root cause can be found.
        app.add_stage("single", SystemStage::single_threaded())
            .add_system_to_stage("single", sync_simple_transforms)
            .add_system_to_stage("single", propagate_transforms);

        fn setup_world(world: &mut World) -> (Entity, Entity) {
            let mut grandchild = Entity::from_raw(0);
            let child = world
                .spawn(TransformBundle::IDENTITY)
                .with_children(|builder| {
                    grandchild = builder.spawn(TransformBundle::IDENTITY).id();
                })
                .id();
            (child, grandchild)
        }

        let (temp_child, temp_grandchild) = setup_world(&mut temp);
        let (child, grandchild) = setup_world(&mut app.world);

        assert_eq!(temp_child, child);
        assert_eq!(temp_grandchild, grandchild);

        app.world
            .spawn(TransformBundle::IDENTITY)
            .push_children(&[child]);
        std::mem::swap(
            &mut *app.world.get_mut::<Parent>(child).unwrap(),
            &mut *temp.get_mut::<Parent>(grandchild).unwrap(),
        );

        app.update();
    }
}
