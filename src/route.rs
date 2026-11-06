//! Which stages of a formula the homotopy runs over, and how.
//!
//! Stages are numbered from 1, as in `Function::stages`, with stage 0 for the
//! identity before them: `Function::chain`. A route selects a run of stages
//! `first..=last`, and goes from the stage before the first to the last. All
//! the stages make the whole homotopy, from the identity to f.

/// How a route gets from its start to its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Path {
    /// The straight line between the two ends, however many stages lie between.
    #[default]
    Straight,
    /// Through every stage on the way.
    Stages,
}

/// The two stages being blended at some point of a route.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    pub from: usize,
    pub to: usize,
    /// How much of `to` there is: 0 is exactly `from`, 1 exactly `to`.
    pub weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub first: usize,
    pub last: usize,
    pub path: Path,
}

impl Route {
    /// Every one of `count` stages.
    pub fn all(count: usize, path: Path) -> Route {
        Route { first: 1, last: count.max(1), path }
    }

    pub fn is_all(&self, count: usize) -> bool {
        self.first == 1 && self.last == count.max(1)
    }

    /// Where the route starts: the stage before the first one selected.
    pub fn start(&self) -> usize {
        self.first - 1
    }

    pub fn contains(&self, stage: usize) -> bool {
        (self.first..=self.last).contains(&stage)
    }

    /// Parts the route plays one after the other.
    pub fn sections(&self) -> usize {
        match self.path {
            Path::Straight => 1,
            Path::Stages => self.last - self.start(),
        }
    }

    /// What shows at `t`, from 0 at the start of the route to 1 at its end.
    ///
    /// Through the stages this is a smooth partition of unity over them, with
    /// the weights of [`smooth`]. Only two stages ever overlap, so the shader
    /// never has to work out more. Weights that stay positive and reach 1 at
    /// each stage must peak there, so the motion always comes to rest for a
    /// moment on a stage: that is what shows each one.
    pub fn leg(&self, t: f64) -> Leg {
        let (start, last) = (self.start(), self.last);
        match self.path {
            Path::Straight => Leg { from: start, to: last, weight: t },
            Path::Stages => {
                let along = t * (last - start) as f64;
                let section = (along.floor() as usize).min(last - start - 1);
                let from = start + section;
                Leg { from, to: from + 1, weight: smooth(along - section as f64) }
            }
        }
    }

    /// The `t` at which exactly `stage` shows, if the route shows it.
    pub fn position_of(&self, stage: usize) -> Option<f64> {
        let (start, last) = (self.start(), self.last);
        match self.path {
            Path::Straight if stage == start => Some(0.0),
            Path::Straight if stage == last => Some(1.0),
            Path::Stages if (start..=last).contains(&stage) => {
                Some((stage - start) as f64 / (last - start) as f64)
            }
            _ => None,
        }
    }
}

/// The smooth step from 0 at `s` = 0 to 1 at `s` = 1, which has no corner in
/// any derivative at either end: with `ρ(x) = e^(−1/x)`, `ρ(s) / (ρ(s) + ρ(1 − s))`.
pub fn smooth(s: f64) -> f64 {
    let rho = |x: f64| if x <= 0.0 { 0.0 } else { (-1.0 / x).exp() };
    let (rise, fall) = (rho(s), rho(1.0 - s));
    rise / (rise + fall)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(first: usize, last: usize, path: Path) -> Route {
        Route { first, last, path }
    }

    #[test]
    fn all_the_stages_make_the_whole_homotopy() {
        let all = Route::all(3, Path::Straight);
        assert!(all.is_all(3));
        assert_eq!(all.leg(0.5), Leg { from: 0, to: 3, weight: 0.5 });
        assert!(!route(2, 3, Path::Straight).is_all(3));
        // The identity has one stage of its own.
        assert_eq!(Route::all(0, Path::Straight), route(1, 1, Path::Straight));
    }

    #[test]
    fn the_smooth_step_runs_from_0_to_1() {
        assert_eq!((smooth(0.0), smooth(1.0)), (0.0, 1.0));
        assert!((smooth(0.5) - 0.5).abs() < 1e-12);
        for k in 0..100 {
            let s = k as f64 / 100.0;
            assert!(smooth(s) <= smooth(s + 0.01), "{s}");
            assert!((smooth(s) + smooth(1.0 - s) - 1.0).abs() < 1e-12, "{s}");
        }
    }

    #[test]
    fn the_smooth_step_lies_flat_at_its_ends() {
        // Flatter than any polynomial: even s¹⁰ is not this small.
        assert!(smooth(0.02) < 1e-20);
        assert!(1.0 - smooth(0.98) < 1e-15);
    }

    #[test]
    fn a_route_through_the_stages_shows_each_one_exactly() {
        let through = route(2, 4, Path::Stages);
        assert_eq!(through.sections(), 3);
        for stage in 1..=4 {
            let t = through.position_of(stage).unwrap();
            let leg = through.leg(t);
            let shown = if leg.weight == 0.0 { leg.from } else { leg.to };
            assert_eq!(shown, stage, "at {t}: {leg:?}");
        }
        assert_eq!(through.leg(0.0), Leg { from: 1, to: 2, weight: 0.0 });
        assert_eq!(through.leg(1.0), Leg { from: 3, to: 4, weight: 1.0 });
        assert_eq!(through.position_of(0), None);
        assert_eq!(through.position_of(5), None);
    }

    #[test]
    fn a_straight_route_only_shows_its_ends() {
        let straight = route(2, 4, Path::Straight);
        assert_eq!(straight.sections(), 1);
        assert_eq!(straight.position_of(1), Some(0.0));
        assert_eq!(straight.position_of(4), Some(1.0));
        assert_eq!(straight.position_of(2), None);
    }

    /// One function per stage, far apart, so a jump in the blend would show.
    #[test]
    fn nothing_jumps_going_through_the_stages() {
        let through = route(1, 5, Path::Stages);
        let at = |t: f64| {
            let leg = through.leg(t);
            leg.from as f64 * (1.0 - leg.weight) + leg.to as f64 * leg.weight
        };
        let steps = 100_000;
        for k in 0..steps {
            let (a, b) = (k as f64 / steps as f64, (k + 1) as f64 / steps as f64);
            assert!((at(b) - at(a)).abs() < 1e-3, "jump at {a}");
            // Rounding aside.
            assert!(at(b) >= at(a) - 1e-12, "back at {a}");
        }
    }
}
