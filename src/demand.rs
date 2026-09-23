//! Monthly demand charges, with an optional ratchet.

use crate::TariffError;
use crate::contract::{Kilowatt, Minutes, RateWithSource, TimeOfUse, Usd, YearMonth};
use std::collections::BTreeMap;

/// Lookback that floors billed kW at a percentage of recent peaks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ratchet {
    pct: f64,
    months: u32,
}

impl Ratchet {
    /// `Ratchet::new(pct, months)` is that lookback. **Calculation.**
    ///
    /// `pct` is in `(0, 100]`. `months` is at least 1.
    ///
    /// # Errors
    /// [`TariffError::InvalidRatchetPct`] or [`TariffError::InvalidRatchetMonths`].
    pub fn new(pct: f64, months: u32) -> Result<Self, TariffError> {
        if !(pct > 0.0 && pct <= 100.0 && pct.is_finite()) {
            return Err(TariffError::InvalidRatchetPct(pct.to_string()));
        }
        if months == 0 {
            return Err(TariffError::InvalidRatchetMonths(months));
        }
        Ok(Self { pct, months })
    }

    /// Percent of the lookback peak, in `(0, 100]`. **Calculation.**
    #[must_use]
    pub const fn pct(self) -> f64 {
        self.pct
    }

    /// How many prior months to search. **Calculation.**
    #[must_use]
    pub const fn months(self) -> u32 {
        self.months
    }
}

/// A monthly demand charge: dollars per kilowatt-month, 15- or 30-minute window, optional ratchet.
///
/// one spike sets the month; this is the dollar objective on a retail tariff, DAM is a shape signal
#[derive(Debug, Clone, PartialEq)]
pub struct DemandCharge {
    rate: RateWithSource,
    window: Minutes,
    ratchet: Option<Ratchet>,
    hours: Option<TimeOfUse>,
}

impl DemandCharge {
    /// `DemandCharge::new(rate, window, ratchet, hours)` is that charge.
    /// **Calculation.**
    ///
    /// `window` must already be 15 or 30 ([`Minutes::demand_window`]).
    ///
    /// # Errors
    /// [`TariffError::InvalidWindow`] if `window` is not 15 or 30.
    pub fn new(
        rate: RateWithSource,
        window: Minutes,
        ratchet: Option<Ratchet>,
        hours: Option<TimeOfUse>,
    ) -> Result<Self, TariffError> {
        // Re-run the window rule so a `Minutes::positive` value cannot sneak in.
        let window = Minutes::demand_window(window.get())?;
        Ok(Self {
            rate,
            window,
            ratchet,
            hours,
        })
    }

    /// Dollars-per-kilowatt-month rate. **Calculation.**
    #[must_use]
    pub const fn rate(&self) -> &RateWithSource {
        &self.rate
    }

    /// 15 or 30 minutes. **Calculation.**
    #[must_use]
    pub const fn window(&self) -> Minutes {
        self.window
    }

    /// Ratchet, when the contract has one. **Calculation.**
    #[must_use]
    pub const fn ratchet(&self) -> Option<Ratchet> {
        self.ratchet
    }

    /// Time-of-use window, when the contract has one. **Calculation.**
    #[must_use]
    pub const fn hours(&self) -> Option<TimeOfUse> {
        self.hours
    }
}

/// Prior months' demand peaks, keyed by [`YearMonth`].
///
/// A [`BTreeMap`] so the same history always walks in the same order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DemandHistory {
    peaks: BTreeMap<YearMonth, Kilowatt>,
}

impl DemandHistory {
    /// Empty history. **Calculation.**
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `history.with_peak(month, kw)` is this history plus that month's peak.
    /// A later call for the same month replaces the peak. **Calculation.**
    #[must_use]
    pub fn with_peak(mut self, month: YearMonth, peak: Kilowatt) -> Self {
        self.peaks.insert(month, peak);
        self
    }

    /// Greatest peak among the `n` months immediately before `month`. Missing
    /// months contribute nothing. **Calculation.**
    #[must_use]
    pub fn max_peak_before(&self, month: YearMonth, n: u32) -> f64 {
        let mut cursor = month;
        let mut max: f64 = 0.0;
        for _ in 0..n {
            cursor = cursor.pred();
            if let Some(peak) = self.peaks.get(&cursor) {
                max = max.max(peak.get());
            }
        }
        max
    }
}

/// Peak, interval, ratchet, and dollars for one demand charge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DemandResult {
    peak_kw: Kilowatt,
    peak_interval: Option<usize>,
    period: Option<TimeOfUse>,
    charge: Usd,
    ratchet_applied: bool,
    ratchet_basis_kw: Kilowatt,
    billed_kw: Kilowatt,
}

impl DemandResult {
    /// This month's peak kilowatts across the supplied intervals. **Calculation.**
    #[must_use]
    pub const fn peak_kw(self) -> Kilowatt {
        self.peak_kw
    }

    /// Index of the first interval that sets [`Self::peak_kw`], if any sample is positive.
    /// **Calculation.**
    #[must_use]
    pub const fn peak_interval(self) -> Option<usize> {
        self.peak_interval
    }

    /// The charge's time-of-use window, when it has one.
    ///
    /// The caller filters samples before [`demand_charge`]; this is the stored
    /// window, not a resample. **Calculation.**
    #[must_use]
    pub const fn period(self) -> Option<TimeOfUse> {
        self.period
    }

    /// Billed dollars: [`Self::billed_kw`] times the rate. **Calculation.**
    #[must_use]
    pub const fn charge(self) -> Usd {
        self.charge
    }

    /// True when the ratchet floor is strictly above this month's peak. **Calculation.**
    #[must_use]
    pub const fn ratchet_applied(self) -> bool {
        self.ratchet_applied
    }

    /// Ratchet floor in kilowatts: `pct/100 × max peak of the previous N months`.
    /// Zero when the charge has no ratchet. **Calculation.**
    #[must_use]
    pub const fn ratchet_basis_kw(self) -> Kilowatt {
        self.ratchet_basis_kw
    }

    /// Kilowatts the rate multiplies: the month peak, or the ratchet floor when
    /// that is higher. **Calculation.**
    #[must_use]
    pub(crate) const fn billed_kw(self) -> Kilowatt {
        self.billed_kw
    }
}

/// `demand_charge(charge, kw_by_interval, month, history)` is the peak and the dollars.
///
/// `peak_kw` is this month's maximum sample. With a ratchet the billed kilowatts
/// are `max(peak_kw, pct/100 × max peak of the previous N months)`. `charge` is
/// those billed kilowatts times the rate, the same dollars as [`demand_bill`].
///
/// `kw_by_interval` is already in the charge's demand window. The caller applies
/// any time-of-use filter; this function does not resample.
///
/// **Calculation.**
#[must_use]
#[allow(
    clippy::module_name_repetitions,
    reason = "public name is specified as demand_charge"
)]
pub fn demand_charge(
    charge: &DemandCharge,
    kw_by_interval: &[Kilowatt],
    month: YearMonth,
    history: &DemandHistory,
) -> DemandResult {
    let mut peak_kw = 0.0;
    let mut peak_interval = None;
    for (index, sample) in kw_by_interval.iter().enumerate() {
        if sample.get() > peak_kw {
            peak_kw = sample.get();
            peak_interval = Some(index);
        }
    }
    let ratchet_basis_kw = match charge.ratchet() {
        None => 0.0,
        Some(ratchet) => ratchet.pct() / 100.0 * history.max_peak_before(month, ratchet.months()),
    };
    let ratchet_applied = charge.ratchet().is_some() && ratchet_basis_kw > peak_kw;
    let billed_kw = peak_kw.max(ratchet_basis_kw);
    DemandResult {
        peak_kw: Kilowatt::new(peak_kw),
        peak_interval,
        period: charge.hours(),
        charge: Usd::new(billed_kw * charge.rate().value()),
        ratchet_applied,
        ratchet_basis_kw: Kilowatt::new(ratchet_basis_kw),
        billed_kw: Kilowatt::new(billed_kw),
    }
}

/// `demand_bill(charge, kw_by_interval, month, history)` is billed kW times dollars-per-kilowatt-month.
///
/// Same dollars as [`demand_charge`]. Billed kW is this month's peak across
/// `kw_by_interval`. With a ratchet it is
/// `max(this_month_peak, pct/100 × max peak of the previous N months in history)`.
///
/// `kw_by_interval` is already in the charge's demand window. The caller applies
/// any time-of-use filter; this function does not resample.
///
/// **Calculation.**
#[must_use]
#[allow(
    clippy::module_name_repetitions,
    reason = "public name is specified as demand_bill"
)]
pub fn demand_bill(
    charge: &DemandCharge,
    kw_by_interval: &[Kilowatt],
    month: YearMonth,
    history: &DemandHistory,
) -> Usd {
    demand_charge(charge, kw_by_interval, month, history).charge()
}

/// `marginal_demand_value(interval_mean_kw, running_peak_kw, month, history, charge)`
/// is the dollars this interval would add to the demand charge if it became the new peak.
///
/// Zero when `interval_mean_kw` is at or below `running_peak_kw`. Zero when it is
/// above the running peak but at or below the ratchet floor. Otherwise
/// `(interval - max(running_peak, floor)) * rate`. **Calculation.**
#[must_use]
pub fn marginal_demand_value(
    interval_mean_kw: Kilowatt,
    running_peak_kw: Kilowatt,
    month: YearMonth,
    history: &DemandHistory,
    charge: &DemandCharge,
) -> Usd {
    let floor = match charge.ratchet() {
        None => 0.0,
        Some(ratchet) => ratchet.pct() / 100.0 * history.max_peak_before(month, ratchet.months()),
    };
    let interval = interval_mean_kw.get();
    let running = running_peak_kw.get();
    if interval <= running || interval <= floor {
        return Usd::new(0.0);
    }
    let above = interval - running.max(floor);
    Usd::new(above * charge.rate().value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CalendarDate, RateUnit};

    fn ten_dollars_per_kw_month() -> RateWithSource {
        RateWithSource::new(
            10.0,
            RateUnit::UsdPerKwMonth,
            "SYNTHETIC ESTIMATE",
            CalendarDate::new(2026, 1, 1).expect("valid"),
            false,
        )
        .expect("valid rate")
    }

    fn window_15() -> Minutes {
        Minutes::demand_window(15).expect("15 is legal")
    }

    #[test]
    fn demand_without_ratchet_one_interval_is_one_thousand_dollars() {
        // Peak 100 kW × $10/kW-month = $1000.
        let charge = DemandCharge::new(ten_dollars_per_kw_month(), window_15(), None, None)
            .expect("valid charge");
        let month = YearMonth::new(2026, 3).expect("valid");
        let bill = demand_bill(
            &charge,
            &[Kilowatt::new(100.0)],
            month,
            &DemandHistory::new(),
        );
        assert_eq!(bill, Usd::new(1000.0));
    }

    #[test]
    fn demand_with_eighty_percent_ratchet_bills_one_hundred_sixty_kilowatts() {
        // This month peak 100 kW. Prior month peak 200 kW.
        // Ratchet floor = 80/100 × 200 = 160 kW.
        // Billed kW = max(100, 160) = 160. 160 × $10/kW-month = $1600.
        let ratchet = Ratchet::new(80.0, 1).expect("80% of 1 month");
        let charge =
            DemandCharge::new(ten_dollars_per_kw_month(), window_15(), Some(ratchet), None)
                .expect("valid charge");
        let march = YearMonth::new(2026, 3).expect("valid");
        let february = YearMonth::new(2026, 2).expect("valid");
        let history = DemandHistory::new().with_peak(february, Kilowatt::new(200.0));
        let bill = demand_bill(&charge, &[Kilowatt::new(100.0)], march, &history);
        assert_eq!(bill, Usd::new(1600.0));
    }

    #[test]
    fn demand_charge_with_time_of_use_bills_the_supplied_intervals() {
        let hours = TimeOfUse::new(7, 19).expect("valid");
        let charge = DemandCharge::new(ten_dollars_per_kw_month(), window_15(), None, Some(hours))
            .expect("constructor accepts time-of-use");
        assert_eq!(DemandCharge::hours(&charge), Some(hours));
        assert_eq!(Minutes::get(DemandCharge::window(&charge)), 15);
        assert_eq!(DemandCharge::ratchet(&charge), None);
        assert_eq!(
            RateWithSource::value(DemandCharge::rate(&charge)).to_string(),
            "10"
        );
        let month = YearMonth::new(2026, 3).expect("valid");
        let bill = demand_bill(
            &charge,
            &[Kilowatt::new(100.0), Kilowatt::new(40.0)],
            month,
            &DemandHistory::new(),
        );
        // Peak of the supplied intervals is 100 kW × $10/kW-month = $1000.
        // Time-of-use is stored; demand_bill does not resample.
        assert_eq!(bill, Usd::new(1000.0));
    }

    #[test]
    fn max_peak_before_reads_the_lookback_months() {
        let march = YearMonth::new(2026, 3).expect("valid");
        let february = YearMonth::new(2026, 2).expect("valid");
        let history =
            DemandHistory::with_peak(DemandHistory::new(), february, Kilowatt::new(200.0));
        assert_eq!(
            Kilowatt::new(DemandHistory::max_peak_before(&history, march, 1)),
            Kilowatt::new(200.0)
        );
        assert_eq!(
            Kilowatt::new(DemandHistory::max_peak_before(&history, march, 0)),
            Kilowatt::new(0.0)
        );
    }

    #[test]
    fn ratchet_pct_returns_the_stored_percent() {
        let ratchet = Ratchet::new(80.0, 1).expect("80% of 1 month");
        assert_eq!(Ratchet::pct(ratchet).to_string(), "80");
    }

    #[test]
    fn ratchet_months_returns_the_stored_lookback() {
        let ratchet = Ratchet::new(80.0, 1).expect("80% of 1 month");
        assert_eq!(Ratchet::months(ratchet), 1);
    }

    #[test]
    fn window_that_is_not_fifteen_or_thirty_is_refused() {
        assert_eq!(
            Minutes::demand_window(20),
            Err(TariffError::InvalidWindow(20))
        );
        assert_eq!(
            Minutes::demand_window(0),
            Err(TariffError::InvalidWindow(0))
        );
    }

    #[test]
    fn ratchet_pct_outside_open_zero_to_closed_hundred_is_refused() {
        assert!(matches!(
            Ratchet::new(0.0, 1),
            Err(TariffError::InvalidRatchetPct(_))
        ));
        assert!(matches!(
            Ratchet::new(100.1, 1),
            Err(TariffError::InvalidRatchetPct(_))
        ));
        assert!(matches!(
            Ratchet::new(-5.0, 1),
            Err(TariffError::InvalidRatchetPct(_))
        ));
        assert!(Ratchet::new(100.0, 1).is_ok());
    }

    #[test]
    fn ratchet_of_zero_months_is_refused() {
        assert_eq!(
            Ratchet::new(80.0, 0),
            Err(TariffError::InvalidRatchetMonths(0))
        );
    }

    fn month_of_fifteen_minute_samples(spike_at: usize, spike_kw: f64) -> Vec<Kilowatt> {
        // 28 days of 15-minute means. SYNTHETIC. Not a real month of a real site.
        let mut samples = vec![Kilowatt::new(10.0); 28 * 24 * 4];
        samples[spike_at] = Kilowatt::new(spike_kw);
        samples
    }

    #[test]
    fn one_spike_sets_peak_kw_and_charge_and_a_ninety_percent_ratchet_raises_the_charge() {
        let samples = month_of_fifteen_minute_samples(100, 80.0);
        let plain =
            DemandCharge::new(ten_dollars_per_kw_month(), window_15(), None, None).expect("valid");
        let march = YearMonth::new(2026, 3).expect("valid");
        let open = demand_charge(&plain, &samples, march, &DemandHistory::new());
        assert_eq!(DemandResult::peak_kw(open), Kilowatt::new(80.0));
        assert_eq!(DemandResult::peak_interval(open), Some(100));
        assert_eq!(DemandResult::period(open), None);
        assert_eq!(DemandResult::charge(open), Usd::new(800.0));
        assert!(!DemandResult::ratchet_applied(open));
        assert_eq!(DemandResult::ratchet_basis_kw(open), Kilowatt::new(0.0));
        assert_eq!(
            demand_bill(&plain, &samples, march, &DemandHistory::new()),
            Usd::new(800.0)
        );

        // Prior peak 200 kW. Floor = 90/100 × 200 = 180 kW, above the 80 kW spike.
        // Charge = 180 × $10/kW-month = $1800. peak_kw stays 80.
        let ratchet = Ratchet::new(90.0, 1).expect("90% of 1 month");
        let ratcheted =
            DemandCharge::new(ten_dollars_per_kw_month(), window_15(), Some(ratchet), None)
                .expect("valid");
        let february = YearMonth::new(2026, 2).expect("valid");
        let history = DemandHistory::new().with_peak(february, Kilowatt::new(200.0));
        let assessed = demand_charge(&ratcheted, &samples, march, &history);
        assert_eq!(DemandResult::peak_kw(assessed), Kilowatt::new(80.0));
        assert_eq!(DemandResult::charge(assessed), Usd::new(1800.0));
        assert!(DemandResult::ratchet_applied(assessed));
        assert_eq!(
            DemandResult::ratchet_basis_kw(assessed),
            Kilowatt::new(180.0)
        );
        assert_eq!(
            demand_bill(&ratcheted, &samples, march, &history),
            Usd::new(1800.0)
        );
    }

    #[test]
    fn demand_result_period_is_the_stored_window() {
        let hours = TimeOfUse::new(7, 19).expect("valid");
        let charge = DemandCharge::new(ten_dollars_per_kw_month(), window_15(), None, Some(hours))
            .expect("valid");
        let month = YearMonth::new(2026, 3).expect("valid");
        let assessed = demand_charge(
            &charge,
            &[Kilowatt::new(10.0)],
            month,
            &DemandHistory::new(),
        );
        assert_eq!(DemandResult::period(assessed), Some(hours));
    }

    #[test]
    fn marginal_demand_value_is_zero_below_the_running_peak_and_below_the_ratchet_floor() {
        let plain =
            DemandCharge::new(ten_dollars_per_kw_month(), window_15(), None, None).expect("valid");
        let march = YearMonth::new(2026, 3).expect("valid");
        let history = DemandHistory::new();
        let below = marginal_demand_value(
            Kilowatt::new(40.0),
            Kilowatt::new(50.0),
            march,
            &history,
            &plain,
        );
        assert_eq!(below, Usd::new(0.0));
        // 100 kW above a 50 kW running peak × $10/kW-month = $1000.
        let above = marginal_demand_value(
            Kilowatt::new(150.0),
            Kilowatt::new(50.0),
            march,
            &history,
            &plain,
        );
        assert_eq!(above, Usd::new(1000.0));

        let ratchet = Ratchet::new(90.0, 1).expect("90%");
        let ratcheted =
            DemandCharge::new(ten_dollars_per_kw_month(), window_15(), Some(ratchet), None)
                .expect("valid");
        let february = YearMonth::new(2026, 2).expect("valid");
        let history = DemandHistory::new().with_peak(february, Kilowatt::new(200.0));
        // Floor = 180 kW. 100 kW is above the 50 kW running peak and below the floor.
        let under_floor = marginal_demand_value(
            Kilowatt::new(100.0),
            Kilowatt::new(50.0),
            march,
            &history,
            &ratcheted,
        );
        assert_eq!(under_floor, Usd::new(0.0));
    }
}
