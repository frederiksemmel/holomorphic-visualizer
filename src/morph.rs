//! The animation that plays when the visualizer opens: the grid bends from
//! the start of the homotopy to its end and back, again and again, until the
//! user takes over. Each section of the homotopy (see `crate::route`) gets
//! its own share of the time, so that going through stages doesn't race.

use std::f64::consts::PI;

/// Seconds the source grid shows before it starts to bend.
const HOLD_SOURCE: f64 = 0.75;
/// Seconds to bend from the start of the homotopy to its end, and the same back.
const BEND: f64 = 3.0;
/// Seconds more for each section after the first.
const EXTRA_SECTION: f64 = 2.0;
/// Seconds the image of the grid shows before it bends back.
const HOLD_IMAGE: f64 = 2.0;

fn bend(sections: usize) -> f64 {
    BEND + EXTRA_SECTION * sections.saturating_sub(1) as f64
}

/// One loop: hold, bend, hold, bend back, hold.
fn period(sections: usize) -> f64 {
    2.0 * (HOLD_SOURCE + bend(sections)) + HOLD_IMAGE
}

/// The homotopy blend `time` seconds into the animation of `sections` sections.
pub fn blend(time: f64, sections: usize) -> f64 {
    let (period, bend) = (period(sections), bend(sections));
    let t = time.rem_euclid(period);
    let ease = |s: f64| (1.0 - (PI * s.clamp(0.0, 1.0)).cos()) / 2.0;
    if t < period / 2.0 {
        ease((t - HOLD_SOURCE) / bend)
    } else {
        ease((period - HOLD_SOURCE - t) / bend)
    }
}

/// A time at which [`blend`] is `homotopy`, on the way to `f`, so the
/// animation can pick up from wherever the user left the homotopy. For 0
/// that is the start, so the source grid shows for a moment first.
pub fn time_at(homotopy: f64, sections: usize) -> f64 {
    if homotopy <= 0.0 {
        return 0.0;
    }
    HOLD_SOURCE + bend(sections) * (1.0 - 2.0 * homotopy.clamp(0.0, 1.0)).acos() / PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_and_ends_on_the_source_grid_and_reaches_f() {
        for sections in [1, 2, 5] {
            let (period, bend) = (period(sections), bend(sections));
            assert_eq!(blend(0.0, sections), 0.0);
            assert_eq!(blend(period - 0.01, sections), 0.0);
            assert_eq!(blend(period / 2.0, sections), 1.0);
            assert!((blend(HOLD_SOURCE + bend / 2.0, sections) - 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn is_continuous() {
        let steps = 10_000;
        let dt = period(1) * 2.0 / steps as f64;
        for k in 0..steps {
            let t = k as f64 * dt;
            assert!((blend(t + dt, 1) - blend(t, 1)).abs() < 0.01, "jump at {t}");
        }
    }

    #[test]
    fn time_at_inverts_blend() {
        for h in [0.0, 0.1, 0.5, 0.9, 1.0] {
            assert!((blend(time_at(h, 3), 3) - h).abs() < 1e-4, "{h}");
        }
    }
}
