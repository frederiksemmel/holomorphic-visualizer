//! The animation that plays when the visualizer opens: the grid bends from
//! the identity to `f` and back, again and again, until the user takes over
//! the homotopy.

use std::f32::consts::PI;

/// Seconds the source grid shows before it starts to bend.
const HOLD_SOURCE: f32 = 0.75;
/// Seconds to bend from the identity to `f`, and the same back.
const BEND: f32 = 3.0;
/// Seconds the image of the grid shows before it bends back.
const HOLD_IMAGE: f32 = 2.0;
/// One loop: hold, bend, hold, bend back, hold.
pub const PERIOD: f32 = 2.0 * (HOLD_SOURCE + BEND) + HOLD_IMAGE;

/// The homotopy blend `time` seconds into the animation.
pub fn blend(time: f32) -> f32 {
    let t = time.rem_euclid(PERIOD);
    let ease = |s: f32| (1.0 - (PI * s.clamp(0.0, 1.0)).cos()) / 2.0;
    if t < PERIOD / 2.0 {
        ease((t - HOLD_SOURCE) / BEND)
    } else {
        ease((PERIOD - HOLD_SOURCE - t) / BEND)
    }
}

/// A time at which [`blend`] is `homotopy`, on the way to `f`, so the
/// animation can pick up from wherever the user left the homotopy. For 0
/// that is the start, so the source grid shows for a moment first.
pub fn time_at(homotopy: f32) -> f32 {
    if homotopy <= 0.0 {
        return 0.0;
    }
    HOLD_SOURCE + BEND * (1.0 - 2.0 * homotopy.clamp(0.0, 1.0)).acos() / PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_and_ends_on_the_source_grid_and_reaches_f() {
        assert_eq!(blend(0.0), 0.0);
        assert_eq!(blend(PERIOD - 0.01), 0.0);
        assert_eq!(blend(PERIOD / 2.0), 1.0);
        assert!((blend(HOLD_SOURCE + BEND / 2.0) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn is_continuous() {
        let steps = 10_000;
        let dt = PERIOD * 2.0 / steps as f32;
        for k in 0..steps {
            let t = k as f32 * dt;
            assert!((blend(t + dt) - blend(t)).abs() < 0.01, "jump at {t}");
        }
    }

    #[test]
    fn time_at_inverts_blend() {
        for h in [0.0, 0.1, 0.5, 0.9, 1.0] {
            assert!((blend(time_at(h)) - h).abs() < 1e-4, "{h}");
        }
    }
}
