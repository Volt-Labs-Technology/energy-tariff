//! Time-of-use energy windows in civil local time.
//!
//! This module does not convert UTC and does not embed a time zone. The caller
//! supplies the month, the ISO weekday, and the minute of day.

use crate::contract::{KilowattHour, Usd, UsdPerMwh};
use crate::{TariffError, require_non_negative_finite};

/// Civil local time: month, ISO weekday, minute of day.
///
/// Month is 1..=12. Weekday is ISO-8601: 1 is Monday, 7 is Sunday. Minute of
/// day is 0..=1439. This crate does not convert UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilMinute {
    month: u8,
    iso_weekday: u8,
    minute_of_day: u16,
}

impl CivilMinute {
    /// `CivilMinute::new(month, iso_weekday, minute_of_day)` is that local time.
    /// **Calculation.** Does not read a clock.
    ///
    /// # Errors
    /// [`TariffError::InvalidMonth`], [`TariffError::InvalidWeekday`], or
    /// [`TariffError::InvalidMinute`].
    pub fn new(month: u8, iso_weekday: u8, minute_of_day: u16) -> Result<Self, TariffError> {
        if !(1..=12).contains(&month) {
            return Err(TariffError::InvalidMonth(month));
        }
        if !(1..=7).contains(&iso_weekday) {
            return Err(TariffError::InvalidWeekday(iso_weekday));
        }
        if minute_of_day >= 1440 {
            return Err(TariffError::InvalidMinute(minute_of_day));
        }
        Ok(Self {
            month,
            iso_weekday,
            minute_of_day,
        })
    }

    /// Month number 1..=12. **Calculation.**
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// ISO weekday, 1 = Monday … 7 = Sunday. **Calculation.**
    #[must_use]
    pub const fn iso_weekday(self) -> u8 {
        self.iso_weekday
    }

    /// Minute of the local day, 0..=1439. **Calculation.**
    #[must_use]
    pub const fn minute_of_day(self) -> u16 {
        self.minute_of_day
    }
}

/// One time-of-use window: which local times it covers, and its energy rate.
#[derive(Debug, Clone, PartialEq)]
pub struct TouPeriod {
    name: String,
    months: Vec<u8>,
    weekdays: Vec<u8>,
    start_min: u16,
    end_min: u16,
    rate: UsdPerMwh,
}

impl TouPeriod {
    /// `TouPeriod::new(name, months, weekdays, start_min, end_min, rate)` is that window.
    /// **Calculation.**
    ///
    /// `start_min` is inclusive and `end_min` is exclusive, both on a single
    /// local day (`end_min` may be 1440). The window does not wrap past midnight.
    ///
    /// # Errors
    /// [`TariffError::EmptyTouName`], [`TariffError::EmptyTouMonths`],
    /// [`TariffError::EmptyTouWeekdays`], [`TariffError::InvalidMonth`],
    /// [`TariffError::InvalidWeekday`], [`TariffError::DuplicateTouMonth`],
    /// [`TariffError::DuplicateTouWeekday`], [`TariffError::InvalidMinute`],
    /// [`TariffError::EmptyTouWindow`], or [`TariffError::IllegalRate`].
    pub fn new(
        name: impl Into<String>,
        months: Vec<u8>,
        weekdays: Vec<u8>,
        start_min: u16,
        end_min: u16,
        rate: f64,
    ) -> Result<Self, TariffError> {
        let name = name.into();
        if name.is_empty() {
            return Err(TariffError::EmptyTouName);
        }
        if months.is_empty() {
            return Err(TariffError::EmptyTouMonths);
        }
        if weekdays.is_empty() {
            return Err(TariffError::EmptyTouWeekdays);
        }
        for (index, month) in months.iter().enumerate() {
            if !(1..=12).contains(month) {
                return Err(TariffError::InvalidMonth(*month));
            }
            if months[..index].contains(month) {
                return Err(TariffError::DuplicateTouMonth(*month));
            }
        }
        for (index, weekday) in weekdays.iter().enumerate() {
            if !(1..=7).contains(weekday) {
                return Err(TariffError::InvalidWeekday(*weekday));
            }
            if weekdays[..index].contains(weekday) {
                return Err(TariffError::DuplicateTouWeekday(*weekday));
            }
        }
        if start_min >= 1440 {
            return Err(TariffError::InvalidMinute(start_min));
        }
        if end_min > 1440 {
            return Err(TariffError::InvalidMinute(end_min));
        }
        if start_min >= end_min {
            return Err(TariffError::EmptyTouWindow);
        }
        Ok(Self {
            name,
            months,
            weekdays,
            start_min,
            end_min,
            rate: UsdPerMwh::new(require_non_negative_finite(rate)?),
        })
    }

    /// Period name. **Calculation.**
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Months 1..=12 this period applies in, in the order stored. **Calculation.**
    #[must_use]
    pub fn months(&self) -> &[u8] {
        &self.months
    }

    /// ISO weekdays this period applies on, in the order stored. **Calculation.**
    #[must_use]
    pub fn weekdays(&self) -> &[u8] {
        &self.weekdays
    }

    /// Inclusive start minute. **Calculation.**
    #[must_use]
    pub const fn start_min(&self) -> u16 {
        self.start_min
    }

    /// Exclusive end minute. **Calculation.**
    #[must_use]
    pub const fn end_min(&self) -> u16 {
        self.end_min
    }

    /// Dollars per megawatt-hour inside this window. **Calculation.**
    #[must_use]
    pub const fn rate(&self) -> UsdPerMwh {
        self.rate
    }

    fn matches(&self, local: CivilMinute) -> bool {
        self.months.contains(&local.month)
            && self.weekdays.contains(&local.iso_weekday)
            && local.minute_of_day >= self.start_min
            && local.minute_of_day < self.end_min
    }
}

/// A non-empty set of time-of-use periods that do not overlap.
#[derive(Debug, Clone, PartialEq)]
pub struct TouSchedule {
    periods: Vec<TouPeriod>,
}

impl TouSchedule {
    /// `TouSchedule::new(periods)` is that schedule. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::EmptyTouSchedule`] or [`TariffError::OverlappingTouPeriods`].
    pub fn new(periods: Vec<TouPeriod>) -> Result<Self, TariffError> {
        if periods.is_empty() {
            return Err(TariffError::EmptyTouSchedule);
        }
        for (index, period) in periods.iter().enumerate() {
            for other in &periods[..index] {
                if periods_overlap(period, other) {
                    return Err(TariffError::OverlappingTouPeriods {
                        first: other.name.clone(),
                        second: period.name.clone(),
                    });
                }
            }
        }
        Ok(Self { periods })
    }

    /// Periods in schedule order. **Calculation.**
    #[must_use]
    pub fn periods(&self) -> &[TouPeriod] {
        &self.periods
    }

    /// The rate whose window contains `local`. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::UnmatchedCivilTime`] when no period contains `local`.
    pub fn rate_at(&self, local: CivilMinute) -> Result<UsdPerMwh, TariffError> {
        for period in &self.periods {
            if period.matches(local) {
                return Ok(period.rate);
            }
        }
        Err(TariffError::UnmatchedCivilTime)
    }
}

fn periods_overlap(left: &TouPeriod, right: &TouPeriod) -> bool {
    shares_any(&left.months, &right.months)
        && shares_any(&left.weekdays, &right.weekdays)
        && left.start_min < right.end_min
        && right.start_min < left.end_min
}

fn shares_any(left: &[u8], right: &[u8]) -> bool {
    left.iter().any(|value| right.contains(value))
}

/// `tou_energy(schedule, load, local)` is kWh × the matching period's dollars-per-megawatt-hour / 1000.
///
/// `local[i]` is the civil time of `load[i]`. **Calculation.**
///
/// # Errors
/// [`TariffError::CivilTimeLengthMismatch`] or [`TariffError::UnmatchedTouInterval`].
pub(crate) fn tou_energy(
    schedule: &TouSchedule,
    load: &[KilowattHour],
    local: &[CivilMinute],
) -> Result<Usd, TariffError> {
    if load.len() != local.len() {
        return Err(TariffError::CivilTimeLengthMismatch {
            load: load.len(),
            local: local.len(),
        });
    }
    let mut dollars = 0.0;
    for (index, (kwh, minute)) in load.iter().zip(local).enumerate() {
        let rate = match schedule.rate_at(*minute) {
            Ok(rate) => rate,
            Err(TariffError::UnmatchedCivilTime) => {
                return Err(TariffError::UnmatchedTouInterval { index });
            }
            Err(other) => return Err(other),
        };
        dollars += kwh.get() * rate.get() / 1000.0;
    }
    Ok(Usd::new(dollars))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_months() -> Vec<u8> {
        (1..=12).collect()
    }

    fn all_weekdays() -> Vec<u8> {
        (1..=7).collect()
    }

    fn period(name: &str, start_min: u16, end_min: u16, rate: f64) -> TouPeriod {
        TouPeriod::new(name, all_months(), all_weekdays(), start_min, end_min, rate)
            .expect("valid period")
    }

    #[test]
    fn civil_minute_stores_month_weekday_and_minute() {
        let local = CivilMinute::new(6, 3, 720).expect("noon on a Wednesday in June");
        assert_eq!(CivilMinute::month(local), 6);
        assert_eq!(CivilMinute::iso_weekday(local), 3);
        assert_eq!(CivilMinute::minute_of_day(local), 720);
    }

    #[test]
    fn civil_minute_refuses_out_of_range_parts() {
        assert_eq!(CivilMinute::new(0, 1, 0), Err(TariffError::InvalidMonth(0)));
        assert_eq!(
            CivilMinute::new(1, 0, 0),
            Err(TariffError::InvalidWeekday(0))
        );
        assert_eq!(
            CivilMinute::new(1, 8, 0),
            Err(TariffError::InvalidWeekday(8))
        );
        assert_eq!(
            CivilMinute::new(1, 1, 1440),
            Err(TariffError::InvalidMinute(1440))
        );
    }

    #[test]
    fn tou_period_stores_its_window_and_rate() {
        let period = TouPeriod::new("off", vec![6], vec![3], 0, 720, 10.0).expect("valid");
        assert_eq!(TouPeriod::name(&period), "off");
        assert_eq!(TouPeriod::months(&period), &[6]);
        assert_eq!(TouPeriod::weekdays(&period), &[3]);
        assert_eq!(TouPeriod::start_min(&period), 0);
        assert_eq!(TouPeriod::end_min(&period), 720);
        assert_eq!(TouPeriod::rate(&period), UsdPerMwh::new(10.0));
    }

    #[test]
    fn tou_period_refuses_an_empty_name_months_or_weekdays() {
        assert_eq!(
            TouPeriod::new("", vec![1], vec![1], 0, 60, 1.0).unwrap_err(),
            TariffError::EmptyTouName
        );
        assert_eq!(
            TouPeriod::new("off", vec![], vec![1], 0, 60, 1.0).unwrap_err(),
            TariffError::EmptyTouMonths
        );
        assert_eq!(
            TouPeriod::new("off", vec![1], vec![], 0, 60, 1.0).unwrap_err(),
            TariffError::EmptyTouWeekdays
        );
    }

    #[test]
    fn tou_period_refuses_a_bad_month_weekday_duplicate_or_window() {
        assert_eq!(
            TouPeriod::new("off", vec![13], vec![1], 0, 60, 1.0).unwrap_err(),
            TariffError::InvalidMonth(13)
        );
        assert_eq!(
            TouPeriod::new("off", vec![1], vec![9], 0, 60, 1.0).unwrap_err(),
            TariffError::InvalidWeekday(9)
        );
        assert_eq!(
            TouPeriod::new("off", vec![1, 1], vec![1], 0, 60, 1.0).unwrap_err(),
            TariffError::DuplicateTouMonth(1)
        );
        assert_eq!(
            TouPeriod::new("off", vec![1], vec![1, 1], 0, 60, 1.0).unwrap_err(),
            TariffError::DuplicateTouWeekday(1)
        );
        assert_eq!(
            TouPeriod::new("off", vec![1], vec![1], 1440, 1440, 1.0).unwrap_err(),
            TariffError::InvalidMinute(1440)
        );
        assert_eq!(
            TouPeriod::new("off", vec![1], vec![1], 0, 1441, 1.0).unwrap_err(),
            TariffError::InvalidMinute(1441)
        );
        assert_eq!(
            TouPeriod::new("off", vec![1], vec![1], 60, 60, 1.0).unwrap_err(),
            TariffError::EmptyTouWindow
        );
    }

    #[test]
    fn tou_period_refuses_a_negative_rate() {
        assert!(matches!(
            TouPeriod::new("off", vec![1], vec![1], 0, 60, -1.0),
            Err(TariffError::IllegalRate { .. })
        ));
    }

    #[test]
    fn empty_schedule_is_refused() {
        assert_eq!(
            TouSchedule::new(vec![]).unwrap_err(),
            TariffError::EmptyTouSchedule
        );
    }

    #[test]
    fn overlapping_periods_are_refused_and_disjoint_ones_are_kept() {
        let morning = period("morning", 0, 720, 10.0);
        let overlap = period("overlap", 600, 800, 20.0);
        assert_eq!(
            TouSchedule::new(vec![morning, overlap]).unwrap_err(),
            TariffError::OverlappingTouPeriods {
                first: "morning".to_owned(),
                second: "overlap".to_owned(),
            }
        );
        let off = period("off", 0, 720, 10.0);
        let on = period("on", 720, 1440, 40.0);
        let schedule = TouSchedule::new(vec![off, on]).expect("boundary is not an overlap");
        assert_eq!(TouSchedule::periods(&schedule).len(), 2);
        let noon = CivilMinute::new(6, 3, 720).expect("valid");
        assert_eq!(
            TouSchedule::rate_at(&schedule, noon).expect("on period"),
            UsdPerMwh::new(40.0)
        );
    }

    #[test]
    fn periods_on_different_weekdays_do_not_overlap() {
        let monday = TouPeriod::new("mon", all_months(), vec![1], 0, 1440, 10.0).expect("valid");
        let tuesday = TouPeriod::new("tue", all_months(), vec![2], 0, 1440, 20.0).expect("valid");
        assert!(TouSchedule::new(vec![monday, tuesday]).is_ok());
    }

    #[test]
    fn rate_at_refuses_an_unmatched_civil_time() {
        let june = TouPeriod::new("june", vec![6], all_weekdays(), 0, 1440, 10.0).expect("valid");
        let schedule = TouSchedule::new(vec![june]).expect("valid");
        let july = CivilMinute::new(7, 1, 0).expect("valid");
        assert_eq!(
            TouSchedule::rate_at(&schedule, july).unwrap_err(),
            TariffError::UnmatchedCivilTime
        );
    }

    #[test]
    fn four_intervals_cross_the_period_boundary() {
        // SYNTHETIC. Not a real tariff.
        // off is [0, 720) at 10 $/MWh. on is [720, 1440) at 40 $/MWh.
        // Four 1000 kWh intervals at 10:00, 11:30, 12:00 and 13:00.
        // 1000 kWh × rate / 1000 = rate dollars each: 10 + 10 + 40 + 40 = 100.
        let schedule = TouSchedule::new(vec![
            period("off", 0, 720, 10.0),
            period("on", 720, 1440, 40.0),
        ])
        .expect("valid");
        let load = [KilowattHour::new(1000.0); 4];
        let local =
            [600_u16, 690, 720, 780].map(|minute| CivilMinute::new(6, 3, minute).expect("valid"));
        let bill = tou_energy(&schedule, &load, &local).expect("each interval matches");
        assert_eq!(bill, Usd::new(100.0));
        let first_two = tou_energy(&schedule, &load[..2], &local[..2]).expect("off");
        let last_two = tou_energy(&schedule, &load[2..], &local[2..]).expect("on");
        assert_eq!(first_two, Usd::new(20.0));
        assert_eq!(last_two, Usd::new(80.0));
    }

    #[test]
    fn tou_energy_refuses_a_length_mismatch_and_an_unmatched_interval() {
        let schedule = TouSchedule::new(vec![period("off", 0, 720, 10.0)]).expect("valid");
        let load = [KilowattHour::new(1000.0); 2];
        let one = [CivilMinute::new(6, 3, 0).expect("valid")];
        assert_eq!(
            tou_energy(&schedule, &load, &one).unwrap_err(),
            TariffError::CivilTimeLengthMismatch { load: 2, local: 1 }
        );
        let past_noon = [
            CivilMinute::new(6, 3, 600).expect("valid"),
            CivilMinute::new(6, 3, 720).expect("valid"),
        ];
        assert_eq!(
            tou_energy(&schedule, &load, &past_noon).unwrap_err(),
            TariffError::UnmatchedTouInterval { index: 1 }
        );
    }
}
