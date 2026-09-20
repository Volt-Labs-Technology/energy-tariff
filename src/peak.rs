//! Coincident-peak charges: a mean of named intervals times an annual dollars-per-kilowatt rate.

use crate::TariffError;
use crate::contract::{Kilowatt, Minutes, PassThrough, RateWithSource, Usd};

/// A coincident-peak rule: 4 or 12 intervals per year, an annual dollars-per-kilowatt rate.
#[derive(Debug, Clone, PartialEq)]
pub struct CoincidentPeakRule {
    intervals_per_year: u8,
    interval_minutes: Minutes,
    months: Vec<u8>,
    rate: RateWithSource,
    pass_through: PassThrough,
}

impl CoincidentPeakRule {
    /// `CoincidentPeakRule::new(...)` is that rule. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::InvalidIntervalsPerYear`], [`TariffError::InvalidMonth`],
    /// or [`TariffError::InvalidIntervalMinutes`].
    pub fn new(
        intervals_per_year: u8,
        interval_minutes: Minutes,
        months: Vec<u8>,
        rate: RateWithSource,
        pass_through: PassThrough,
    ) -> Result<Self, TariffError> {
        if intervals_per_year != 4 && intervals_per_year != 12 {
            return Err(TariffError::InvalidIntervalsPerYear(intervals_per_year));
        }
        for month in &months {
            if !(1..=12).contains(month) {
                return Err(TariffError::InvalidMonth(*month));
            }
        }
        Ok(Self {
            intervals_per_year,
            interval_minutes,
            months,
            rate,
            pass_through,
        })
    }

    /// 4 or 12. **Calculation.**
    #[must_use]
    pub const fn intervals_per_year(&self) -> u8 {
        self.intervals_per_year
    }

    /// Length of each named interval. **Calculation.**
    #[must_use]
    pub const fn interval_minutes(&self) -> Minutes {
        self.interval_minutes
    }

    /// Calendar months the rule names, each 1..=12. **Calculation.**
    #[must_use]
    pub fn months(&self) -> &[u8] {
        &self.months
    }

    /// Annual dollars-per-kilowatt rate. **Calculation.**
    #[must_use]
    pub const fn rate(&self) -> &RateWithSource {
        &self.rate
    }

    /// Whether the charge is billed through. **Calculation.**
    #[must_use]
    pub const fn pass_through(&self) -> PassThrough {
        self.pass_through
    }
}

/// Dollar exposure plus, when the charge is not passed through, the reason.
#[derive(Debug, Clone, PartialEq)]
pub struct PeakExposure {
    amount: Usd,
    report_line: Option<String>,
}

impl PeakExposure {
    /// Dollar exposure. `$0` when not passed through. **Calculation.**
    #[must_use]
    pub const fn amount(&self) -> Usd {
        self.amount
    }

    /// Why the exposure is zero, when it is. **Calculation.**
    #[must_use]
    pub fn report_line(&self) -> Option<&str> {
        self.report_line.as_deref()
    }
}

/// `coincident_peak_exposure(rule, kw_at_intervals)` is mean kW times dollars-per-kilowatt-year.
///
/// `kw_at_intervals.len()` must equal `intervals_per_year`. When
/// `pass_through` is [`PassThrough::NotPassedThrough`] the amount is `$0` and
/// the report line says why.
///
/// **Calculation.**
///
/// # Errors
/// [`TariffError::IntervalCountMismatch`].
pub fn coincident_peak_exposure(
    rule: &CoincidentPeakRule,
    kw_at_intervals: &[Kilowatt],
) -> Result<PeakExposure, TariffError> {
    let expected = usize::from(rule.intervals_per_year());
    if kw_at_intervals.len() != expected {
        return Err(TariffError::IntervalCountMismatch {
            expected,
            got: kw_at_intervals.len(),
        });
    }
    if rule.pass_through() == PassThrough::NotPassedThrough {
        return Ok(PeakExposure {
            amount: Usd::new(0.0),
            report_line: Some(
                "coincident-peak charge is not passed through; exposure is $0".to_owned(),
            ),
        });
    }
    let sum: f64 = kw_at_intervals.iter().map(|sample| sample.get()).sum();
    let mean = sum / f64::from(rule.intervals_per_year());
    Ok(PeakExposure {
        amount: Usd::new(mean * rule.rate().value()),
        report_line: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CalendarDate, RateUnit};

    fn annual_ten() -> RateWithSource {
        RateWithSource::new(
            10.0,
            RateUnit::UsdPerKwYear,
            "SYNTHETIC ESTIMATE",
            CalendarDate::new(2026, 1, 1).expect("valid"),
            false,
        )
        .expect("valid rate")
    }

    fn fifteen() -> Minutes {
        Minutes::positive(15).expect("positive")
    }

    fn rule(intervals: u8, pass: PassThrough) -> CoincidentPeakRule {
        CoincidentPeakRule::new(intervals, fifteen(), vec![6, 7, 8, 9], annual_ten(), pass)
            .expect("valid rule")
    }

    #[test]
    fn four_intervals_mean_times_rate() {
        // mean(10, 20, 30, 40) = 25 kW × $10/kW-year = $250.
        let kw = [
            Kilowatt::new(10.0),
            Kilowatt::new(20.0),
            Kilowatt::new(30.0),
            Kilowatt::new(40.0),
        ];
        let exposure: PeakExposure =
            coincident_peak_exposure(&rule(4, PassThrough::Assumed), &kw).expect("4 vs 4");
        assert_eq!(exposure.amount(), Usd::new(250.0));
        assert_eq!(exposure.report_line(), None);
    }

    #[test]
    fn twelve_intervals_mean_times_rate() {
        // Twelve 10 kW intervals: mean 10 × $10/kW-year = $100.
        let kw = [Kilowatt::new(10.0); 12];
        let exposure =
            coincident_peak_exposure(&rule(12, PassThrough::Confirmed), &kw).expect("12 vs 12");
        assert_eq!(exposure.amount(), Usd::new(100.0));
    }

    #[test]
    fn not_passed_through_is_zero_with_a_why_line() {
        let kw = [Kilowatt::new(100.0); 4];
        let exposure: PeakExposure =
            coincident_peak_exposure(&rule(4, PassThrough::NotPassedThrough), &kw).expect("4 vs 4");
        assert_eq!(exposure.amount(), Usd::new(0.0));
        let line = exposure.report_line().expect("why line");
        assert!(line.contains("not passed through"), "got {line}");
        assert!(line.contains("$0"), "got {line}");
    }

    #[test]
    fn interval_count_mismatch_is_a_typed_error() {
        let kw = [Kilowatt::new(10.0); 3];
        assert_eq!(
            coincident_peak_exposure(&rule(4, PassThrough::Assumed), &kw),
            Err(TariffError::IntervalCountMismatch {
                expected: 4,
                got: 3,
            })
        );
    }

    #[test]
    fn intervals_per_year_other_than_four_or_twelve_is_refused() {
        assert_eq!(
            CoincidentPeakRule::new(6, fifteen(), vec![1], annual_ten(), PassThrough::Assumed)
                .unwrap_err(),
            TariffError::InvalidIntervalsPerYear(6)
        );
    }

    #[test]
    fn month_outside_one_to_twelve_is_refused() {
        assert_eq!(
            CoincidentPeakRule::new(4, fifteen(), vec![0], annual_ten(), PassThrough::Assumed)
                .unwrap_err(),
            TariffError::InvalidMonth(0)
        );
        assert_eq!(
            CoincidentPeakRule::new(4, fifteen(), vec![13], annual_ten(), PassThrough::Assumed)
                .unwrap_err(),
            TariffError::InvalidMonth(13)
        );
    }

    #[test]
    fn rule_stores_the_intervals_months_and_rate() {
        let built: CoincidentPeakRule = rule(4, PassThrough::Confirmed);
        assert_eq!(built.intervals_per_year(), 4);
        assert_eq!(built.interval_minutes(), fifteen());
        assert_eq!(built.months(), &[6, 7, 8, 9]);
        assert_eq!(built.rate().value().to_string(), "10");
        assert_eq!(built.pass_through(), PassThrough::Confirmed);
    }
}
