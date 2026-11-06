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
    /// One stage after the other, each finishing before the next begins, so
    /// the motion comes to rest on every stage and shows it exactly.
    Stages,
    /// Stages that overlap, so the motion never stops: the next is already
    /// rising before the last has finished.
    Overlap,
}

impl Path {
    pub const ALL: [Path; 3] = [Path::Straight, Path::Stages, Path::Overlap];

    pub fn label(self) -> &'static str {
        match self {
            Path::Straight => "Straight line",
            Path::Stages => "Stage by stage",
            Path::Overlap => "Overlapping",
        }
    }

    pub fn next(self) -> Path {
        match self {
            Path::Straight => Path::Stages,
            Path::Stages => Path::Overlap,
            Path::Overlap => Path::Straight,
        }
    }
}

/// What shows at some point of a route: stages with their weights, which add
/// up to 1. No path needs more than three at a time, so the shader never
/// works out more; the slots it doesn't use have weight 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix(pub [(usize, f64); 3]);

impl Mix {
    /// The one stage on show, if nothing else is mixed in.
    pub fn exact(&self) -> Option<usize> {
        self.0.iter().find(|&&(_, weight)| weight == 1.0).map(|&(stage, _)| stage)
    }
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
            Path::Stages | Path::Overlap => self.last - self.start(),
        }
    }

    /// What shows at `t`, from 0 at the start of the route to 1 at its end.
    pub fn mix(&self, t: f64) -> Mix {
        let (start, last) = (self.start(), self.last);
        let pair = |from, to, weight: f64| Mix([(from, 1.0 - weight), (to, weight), (to, 0.0)]);
        match self.path {
            Path::Straight => pair(start, last, t),
            Path::Stages => {
                let along = t * (last - start) as f64;
                let section = (along.floor() as usize).min(last - start - 1);
                let from = start + section;
                pair(from, from + 1, smooth(along - section as f64))
            }
            Path::Overlap => self.overlap(t),
        }
    }

    /// A partition of unity over the stages: a quadratic B-spline around each
    /// one, three stages wide, so the weights are never negative and add up to
    /// 1 by construction. They also add up to the position, `Σ k·φₖ(x) = x`:
    /// evenly spaced stages go by at an even speed. Normalised bumps don't do
    /// that: they rush past each stage and crawl between them.
    ///
    /// Splines past either end count for the first or last stage, and the
    /// route runs half a stage further each way, to where only those are on,
    /// so that its ends show exactly.
    fn overlap(&self, t: f64) -> Mix {
        let (start, sections) = (self.start(), (self.last - self.start()) as f64);
        let along = t * (sections + 1.0) - 0.5;
        let nearest = along.round();
        let off = along - nearest;
        let splines = [
            (nearest - 1.0, (0.5 - off) * (0.5 - off) / 2.0),
            (nearest, 0.75 - off * off),
            (nearest + 1.0, (0.5 + off) * (0.5 + off) / 2.0),
        ];
        let mut parts = [(start, 0.0); 3];
        let mut used = 0;
        for (k, weight) in splines {
            let stage = start + k.clamp(0.0, sections) as usize;
            match parts[..used].iter_mut().find(|part| part.0 == stage) {
                Some(part) => part.1 += weight,
                None => {
                    parts[used] = (stage, weight);
                    used += 1;
                }
            }
        }
        Mix(parts)
    }

    /// The `t` at which `stage` shows, as far as the route shows it: exactly,
    /// but for the stages inside an overlapping route, which is where they
    /// peak.
    pub fn position_of(&self, stage: usize) -> Option<f64> {
        let (start, last) = (self.start(), self.last);
        let sections = (last - start) as f64;
        let inside = (start..=last).contains(&stage);
        match self.path {
            _ if stage == start => Some(0.0),
            _ if stage == last => Some(1.0),
            Path::Stages if inside => Some((stage - start) as f64 / sections),
            Path::Overlap if inside => Some(((stage - start) as f64 + 0.5) / (sections + 1.0)),
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
        assert_eq!(all.mix(0.5).0[..2], [(0, 0.5), (3, 0.5)]);
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
    fn stage_by_stage_shows_each_one_exactly() {
        let through = route(2, 4, Path::Stages);
        assert_eq!(through.sections(), 3);
        for stage in 1..=4 {
            let t = through.position_of(stage).unwrap();
            assert_eq!(through.mix(t).exact(), Some(stage), "at {t}");
        }
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

    /// The stages as numbers: what shows at `t` is then a number that moves
    /// as the picture does.
    fn value(route: Route, t: f64) -> f64 {
        route.mix(t).0.iter().map(|&(stage, weight)| stage as f64 * weight).sum()
    }

    #[test]
    fn nothing_jumps_going_through_the_stages() {
        for path in [Path::Stages, Path::Overlap] {
            let through = route(1, 5, path);
            let steps = 100_000;
            for k in 0..steps {
                let (a, b) = (k as f64 / steps as f64, (k + 1) as f64 / steps as f64);
                let (from, to) = (value(through, a), value(through, b));
                assert!((to - from).abs() < 1e-3, "{path:?} jumps at {a}");
                // Rounding aside.
                assert!(to >= from - 1e-12, "{path:?} goes back at {a}");
            }
        }
    }

    #[test]
    fn weights_always_add_up_to_one() {
        for path in Path::ALL {
            let through = route(2, 6, path);
            for k in 0..=10_000 {
                let t = k as f64 / 10_000.0;
                let parts = through.mix(t).0;
                let total: f64 = parts.iter().map(|part| part.1).sum();
                assert!((total - 1.0).abs() < 1e-12, "{path:?} at {t}: {total}");
                assert!(parts.iter().all(|part| (0.0..=1.0).contains(&part.1)), "{parts:?}");
            }
        }
    }

    #[test]
    fn overlapping_stages_start_and_end_exactly() {
        for last in [1, 2, 5] {
            let through = route(1, last, Path::Overlap);
            assert_eq!(through.mix(0.0).exact(), Some(0));
            assert_eq!(through.mix(1.0).exact(), Some(last));
        }
    }

    /// Between the ramps at either end, evenly spaced stages go by at one
    /// speed: as fast on a stage as between two.
    #[test]
    fn overlapping_stages_never_slow_down() {
        let through = route(1, 5, Path::Overlap);
        let h = 1e-5;
        let speed = |t: f64| (value(through, t + h) - value(through, t - h)) / (2.0 * h);
        let mut t = through.position_of(1).unwrap() + 0.1;
        while t < through.position_of(4).unwrap() {
            assert!((speed(t) - 6.0).abs() < 1e-3, "{} at {t}", speed(t));
            t += 1e-3;
        }
    }

    /// Right at the ends it eases, with no sudden start: it comes off the
    /// first stage from rest, and has picked up speed a short way in.
    #[test]
    fn overlapping_stages_ease_off_the_ends() {
        let through = route(1, 5, Path::Overlap);
        let early = value(through, 0.01);
        assert!(early > 0.0 && early < 0.01, "{early}");
        assert!(value(through, 0.1) > 0.15);
    }

    #[test]
    fn stage_by_stage_does_come_to_rest_on_a_stage() {
        let through = route(1, 5, Path::Stages);
        let (t, h) = (through.position_of(2).unwrap(), 1e-3);
        assert!(value(through, t + h) - value(through, t) < 1e-9);
    }

    #[test]
    fn a_stage_of_an_overlap_is_mixed_with_both_neighbours() {
        let through = route(1, 4, Path::Overlap);
        let parts = through.mix(through.position_of(2).unwrap()).0;
        let weight = |stage| parts.iter().find(|part| part.0 == stage).unwrap().1;
        assert!((weight(2) - 0.75).abs() < 1e-12, "{parts:?}");
        assert!((weight(1) - 0.125).abs() < 1e-12, "{parts:?}");
        assert!((weight(3) - 0.125).abs() < 1e-12, "{parts:?}");
    }

    #[test]
    fn t_steps_through_all_the_paths() {
        assert_eq!(Path::Straight.next().next().next(), Path::Straight);
    }
}
