//! Where the shadow path's time goes.
//!
//! Perf tracks, never gates (getstarted doctrine). This exists to answer one
//! question with numbers: is the blur itself expensive, or is the cost around it?

use std::hint::black_box;
use std::time::Instant;

use rev_ui_mech::{Canvas, Color, Point, Rect, Shadow};

fn main() {
    let round = 200;
    let mut canvas = Canvas::new(400, 160, 1.0).unwrap();
    let started = Instant::now();
    for _ in 0..round {
        canvas.paint(|p| {
            let s = Shadow::outer(Point::new(0.0, 2.0), 3.0, Color::rgba(0, 0, 0, 160));
            for n in 0..12 {
                let x = 10.0 + (n % 6) as f32 * 60.0;
                let y = 10.0 + (n / 6) as f32 * 60.0;
                p.shadow_round_rect(black_box(Rect::new(x, y, 30.0, 15.0)), 2.0, &s);
            }
        });
    }
    let elapsed = started.elapsed();
    let stat = canvas.stat();
    println!(
        "{round} frames · {} shadows/frame ({} distinct) · {} mask px/frame",
        stat.shadow,
        stat.distinct(),
        stat.blur_pixel
    );
    println!(
        "  wall {:.3} ms/frame · blur {:.3} ms/frame ({:.1} ns per mask px)",
        elapsed.as_secs_f64() * 1000.0 / round as f64,
        stat.blur_nanos as f64 / 1.0e6,
        stat.blur_nanos as f64 / stat.blur_pixel as f64
    );
}
