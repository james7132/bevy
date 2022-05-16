use bevy_tasks::{prelude::TaskPoolThreadAssignmentPolicy, TaskPoolBuilder};

// This sample demonstrates creating a thread pool with 4 compute threads and spawning 40 tasks that
// spin for 100ms. It's expected to take about a second to run (assuming the machine has >= 4 logical
// cores)

fn main() {
    let pool = TaskPoolBuilder {
        compute: TaskPoolThreadAssignmentPolicy {
            min_threads: 4,
            max_threads: 4,
            percent: 1.0,
        },
        thread_name: Some("Busy Behavior ThreadPool".to_string()),
        ..Default::default()
    }
    .build();

    let t0 = instant::Instant::now();
    pool.scope(|s| {
        for i in 0..40 {
            s.spawn(async move {
                let now = instant::Instant::now();
                while instant::Instant::now() - now < instant::Duration::from_millis(100) {
                    // spin, simulating work being done
                }

                println!(
                    "Thread {:?} index {} finished",
                    std::thread::current().id(),
                    i
                );
            });
        }
    });

    let t1 = instant::Instant::now();
    println!("all tasks finished in {} secs", (t1 - t0).as_secs_f32());
}
