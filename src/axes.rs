//! The background grid of the plane: how far apart its lines are at a given
//! zoom, and how the numbers on its axes read.
//!
//! Lines and labels sit at whole multiples of a step, so they are numbered by
//! an integer `k` (the line at `k · step`) and labels are written from `k`
//! exactly, without the rounding of `k as f64 * step`.

/// Most zeros a label on an axis writes out.
const MAX_ZEROS: i32 = 6;

/// A step of `mantissa · 10^exponent` plane units, with mantissa 1, 2 or 5:
/// steps whose multiples read as short decimals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub mantissa: i64,
    pub exponent: i32,
}

impl Step {
    /// The step whose lines are drawn closest to `spacing` screen points
    /// apart at `pixels_per_unit`. Closest by ratio, so the spacing on screen
    /// stays within a factor of about 1.6 of `spacing` at every zoom.
    pub fn near(spacing: f64, pixels_per_unit: f64) -> Step {
        let target = spacing / pixels_per_unit;
        let exponent = target.log10().floor() as i32;
        [
            (1, exponent),
            (2, exponent),
            (5, exponent),
            (1, exponent + 1),
        ]
        .map(|(mantissa, exponent)| Step { mantissa, exponent })
        .into_iter()
        .min_by(|a, b| {
            let off = |step: &Step| (step.value() / target).ln().abs();
            off(a).total_cmp(&off(b))
        })
        .expect("four candidates")
    }

    /// The step in plane units.
    pub fn value(self) -> f64 {
        self.mantissa as f64 * 10f64.powi(self.exponent)
    }

    /// How many minor lines divide each major one: fifths of a 1 or 5 step
    /// (0.2, 1), halves of a 2 step (1), so minor lines are 1·2·5 steps too.
    pub fn minor_divisions(self) -> i64 {
        if self.mantissa == 2 { 2 } else { 5 }
    }

    /// The numbers of the lines at multiples of `step` (plane units) that
    /// fall within `[min, max]`.
    /// None past where an f64 tells neighbouring multiples apart, which the
    /// zoom reaches first far from the origin.
    pub fn lines(step: f64, min: f64, max: f64) -> std::ops::RangeInclusive<i64> {
        let (first, last) = ((min / step).ceil(), (max / step).floor());
        if first.abs().max(last.abs()) >= (1u64 << 53) as f64 {
            return 1..=0;
        }
        first as i64..=last as i64
    }

    /// The label of the line at `k` times this step on the real axis: as
    /// many decimals as the step needs, trailing zeros dropped (0.8, 1, 1.2).
    /// Past `MAX_ZEROS` zeros, in scientific notation.
    pub fn real_label(self, k: i64) -> String {
        let digits = (k * self.mantissa).unsigned_abs().to_string();
        let number = if k != 0 && self.exponent.abs() > MAX_ZEROS {
            let significant = digits.trim_end_matches('0');
            let exponent = self.exponent + (digits.len() - significant.len()) as i32;
            with_minus_signs(&format!("{significant}e{exponent}"))
        } else if self.exponent >= 0 {
            digits + &"0".repeat(self.exponent as usize)
        } else {
            let decimals = (-self.exponent) as usize;
            let digits = format!("{digits:0>width$}", width = decimals + 1);
            let (whole, fraction) = digits.split_at(digits.len() - decimals);
            let fraction = fraction.trim_end_matches('0');
            if fraction.is_empty() {
                whole.to_owned()
            } else {
                format!("{whole}.{fraction}")
            }
        };
        if k < 0 {
            format!("−{number}")
        } else {
            number
        }
    }

    /// The label on the imaginary axis: 2i, i, 0.5i, −i, and 0 for the origin.
    pub fn imaginary_label(self, k: i64) -> String {
        match self.real_label(k).as_str() {
            "0" => "0".to_owned(),
            "1" => "i".to_owned(),
            "−1" => "−i".to_owned(),
            number => format!("{number}i"),
        }
    }
}

/// `text` with each hyphen as a real minus sign, as math is set.
pub fn with_minus_signs(text: &str) -> String {
    text.replace('-', "−")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(mantissa: i64, exponent: i32) -> Step {
        Step { mantissa, exponent }
    }

    #[test]
    fn steps_go_one_two_five() {
        // Around 120 points apart.
        assert_eq!(Step::near(120.0, 150.0), step(1, 0));
        assert_eq!(Step::near(120.0, 60.0), step(2, 0));
        assert_eq!(Step::near(120.0, 30.0), step(5, 0));
        assert_eq!(Step::near(120.0, 1500.0), step(1, -1));
        assert_eq!(Step::near(120.0, 1.0), step(1, 2));
        assert_eq!(Step::near(120.0, 1e6), step(1, -4));
    }

    #[test]
    fn steps_stay_near_the_spacing_at_every_zoom() {
        let mut ppu = 1e-30;
        while ppu <= 1e300 {
            let spacing = Step::near(120.0, ppu).value() * ppu;
            assert!((75.0..=192.0).contains(&spacing), "{ppu}: {spacing}");
            ppu *= 1.07;
        }
    }

    #[test]
    fn minor_lines_are_one_two_five_steps_too() {
        assert_eq!(step(1, 0).minor_divisions(), 5);
        assert_eq!(step(2, 0).minor_divisions(), 2);
        assert_eq!(step(5, -3).minor_divisions(), 5);
    }

    #[test]
    fn lines_cover_the_range() {
        assert_eq!(Step::lines(0.5, -1.2, 0.9), -2..=1);
        assert_eq!(Step::lines(1.0, 2.0, 3.0), 2..=3);
        assert!(Step::lines(1.0, 0.2, 0.8).is_empty());
    }

    #[test]
    fn labels_take_the_decimals_the_step_needs() {
        let fifth = step(2, -1);
        let labels: Vec<_> = (3..=6).map(|k| fifth.real_label(k)).collect();
        assert_eq!(labels, ["0.6", "0.8", "1", "1.2"]);
        assert_eq!(step(5, -4).real_label(3), "0.0015");
        assert_eq!(step(1, -4).real_label(10_001), "1.0001");
        assert_eq!(step(2, 2).real_label(3), "600");
        assert_eq!(step(1, 0).real_label(0), "0");
    }

    #[test]
    fn labels_use_a_real_minus_sign() {
        assert_eq!(step(5, -1).real_label(-3), "−1.5");
        assert_eq!(step(1, 1).real_label(-2), "−20");
    }

    #[test]
    fn imaginary_labels_drop_the_one() {
        let one = step(1, 0);
        let labels: Vec<_> = (-2..=2).map(|k| one.imaginary_label(k)).collect();
        assert_eq!(labels, ["−2i", "−i", "0", "i", "2i"]);
        assert_eq!(step(5, -1).imaginary_label(1), "0.5i");
        assert_eq!(step(2, -1).imaginary_label(-5), "−i");
    }

    #[test]
    fn minus_signs() {
        assert_eq!(with_minus_signs("-1.5 - 2i"), "−1.5 − 2i");
    }
}
