//! Measurements, not assertions: `cargo test --profile perf browser_perf -- --ignored --nocapture`.
//! The Lynx side of a frame at a typical window size: the copy out of shared memory, and the
//! picture the tab list shows once a second.

use std::time::Instant;

use super::page_thumbnail;

const WIDTH: usize = 1400;
const HEIGHT: usize = 800;

fn per_call_ms(rounds: u32, mut run: impl FnMut()) -> f64 {
    run();
    let started = Instant::now();
    for _ in 0..rounds {
        run();
    }
    started.elapsed().as_secs_f64() * 1000.0 / f64::from(rounds)
}

#[test]
#[ignore = "measurement"]
fn bench_frame_costs() {
    let shared: Vec<u8> = (0..WIDTH * HEIGHT * 4).map(|n| (n % 251) as u8).collect();

    // What `copy_frame` into a new Vec costs each frame, against copying into one kept around.
    let fresh = per_call_ms(300, || {
        let mut pixels = Vec::new();
        pixels.extend_from_slice(&shared);
        std::hint::black_box(&pixels);
    });
    let mut kept = Vec::with_capacity(shared.len());
    let reused = per_call_ms(300, || {
        kept.clear();
        kept.extend_from_slice(&shared);
        std::hint::black_box(&kept);
    });
    let thumbnail = per_call_ms(30, || {
        std::hint::black_box(page_thumbnail::encode(WIDTH as u32, HEIGHT as u32, &shared));
    });

    println!(
        "BENCH frame_costs copy_fresh_ms={fresh:.3} copy_reused_ms={reused:.3} \
         thumbnail_ms={thumbnail:.3} frame_mb={:.1}",
        shared.len() as f64 / 1e6
    );
}
