use bevy_ecs::{
    component::Component,
    prelude::*,
    schedule::{Stage, SystemStage},
    world::World,
};
use bevy_tasks::{ComputeTaskPool, TaskPool};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

criterion_group!(benches, empty_archetypes);
criterion_main!(benches);

#[derive(Component)]
struct A<const N: u16>(f32);

fn iter(query: Query<&A<0>>) {
    for comp in query.iter() {
        black_box(comp);
    }
}

fn for_each(query: Query<&A<0>>) {
    query.for_each(|comp| {
        black_box(comp);
    });
}

fn par_for_each(task_pool: Res<ComputeTaskPool>, query: Query<&A<0>>) {
    query.par_for_each(&*task_pool, 64, |comp| {
        black_box(comp);
    });
}

fn setup(parallel: bool, setup: impl FnOnce(&mut SystemStage)) -> (World, SystemStage) {
    let mut world = World::new();
    let mut stage = SystemStage::parallel();
    if parallel {
        world.insert_resource(ComputeTaskPool(TaskPool::default()));
    }
    setup(&mut stage);
    (world, stage)
}

/// create `count` entities with distinct archetypes
fn add_archetypes(world: &mut World, count: u16) {
    for i in 0..count {
        let mut e = world.spawn();
        e.insert(A::<0>(1.0));
        if i & 1 << 1 != 0 {
            e.insert(A::<1>(1.0));
        }
        if i & 1 << 2 != 0 {
            e.insert(A::<2>(1.0));
        }
        if i & 1 << 3 != 0 {
            e.insert(A::<3>(1.0));
        }
        if i & 1 << 4 != 0 {
            e.insert(A::<4>(1.0));
        }
        if i & 1 << 5 != 0 {
            e.insert(A::<5>(1.0));
        }
        if i & 1 << 6 != 0 {
            e.insert(A::<6>(1.0));
        }
        if i & 1 << 7 != 0 {
            e.insert(A::<7>(1.0));
        }
        if i & 1 << 8 != 0 {
            e.insert(A::<8>(1.0));
        }
        if i & 1 << 9 != 0 {
            e.insert(A::<9>(1.0));
        }
        if i & 1 << 10 != 0 {
            e.insert(A::<10>(1.0));
        }
        if i & 1 << 11 != 0 {
            e.insert(A::<11>(1.0));
        }
        if i & 1 << 12 != 0 {
            e.insert(A::<12>(1.0));
        }
        if i & 1 << 13 != 0 {
            e.insert(A::<13>(1.0));
        }
        if i & 1 << 14 != 0 {
            e.insert(A::<14>(1.0));
        }
        if i & 1 << 15 != 0 {
            e.insert(A::<15>(1.0));
        }
    }
}

fn empty_archetypes(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("empty_archetypes");
    for archetype_count in [10, 100, 500, 1000, 2000, 5000, 10000] {
        let (mut world, mut stage) = setup(true, |stage| {
            stage.add_system(iter);
        });
        add_archetypes(&mut world, archetype_count);
        world.clear_entities();
        let mut e = world.spawn();
        e.insert(A::<0>(1.0));
        stage.run(&mut world);
        group.bench_with_input(
            BenchmarkId::new("iter", archetype_count),
            &archetype_count,
            |bencher, &_| {
                bencher.iter(|| {
                    stage.run(&mut world);
                })
            },
        );
    }
    for archetype_count in [10, 100, 500, 1000, 2000, 5000, 10000] {
        let (mut world, mut stage) = setup(true, |stage| {
            stage.add_system(for_each);
        });
        add_archetypes(&mut world, archetype_count);
        world.clear_entities();
        let mut e = world.spawn();
        e.insert(A::<0>(1.0));
        stage.run(&mut world);
        group.bench_with_input(
            BenchmarkId::new("for_each", archetype_count),
            &archetype_count,
            |bencher, &_| {
                bencher.iter(|| {
                    stage.run(&mut world);
                })
            },
        );
    }
    for archetype_count in [10, 100, 500, 1000, 2000, 5000, 10000] {
        let (mut world, mut stage) = setup(true, |stage| {
            stage.add_system(par_for_each);
        });
        add_archetypes(&mut world, archetype_count);
        world.clear_entities();
        let mut e = world.spawn();
        e.insert(A::<0>(1.0));
        stage.run(&mut world);
        group.bench_with_input(
            BenchmarkId::new("par_for_each", archetype_count),
            &archetype_count,
            |bencher, &_| {
                bencher.iter(|| {
                    stage.run(&mut world);
                })
            },
        );
    }
}
