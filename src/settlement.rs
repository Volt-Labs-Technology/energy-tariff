//! Energy settlement: DAM-indexed, real-time, a hedged flat rate, an adder, or time-of-use.

use crate::contract::{KilowattHour, Usd, UsdPerMwh};
use crate::tou::{CivilMinute, TouSchedule, tou_energy};
use crate::{TariffError, require_non_negative_finite};

/// A hedged flat energy rate, in dollars per megawatt-hour. Negative and non-finite values are refused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HedgedRate {
    flat: UsdPerMwh,
}

impl HedgedRate {
    /// `HedgedRate::new(flat)` is that dollars-per-megawatt-hour rate. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::IllegalRate`].
    pub fn new(flat: f64) -> Result<Self, TariffError> {
        Ok(Self {
            flat: UsdPerMwh::new(require_non_negative_finite(flat)?),
        })
    }

    /// The flat dollars-per-megawatt-hour rate. **Calculation.**
    #[must_use]
    pub const fn flat(self) -> UsdPerMwh {
        self.flat
    }
}

/// A non-zero dollars-per-megawatt-hour adder on a price series.
///
/// Zero is not an adder: it is the unit settlement ([`Settlement::DamIndexed`]
/// or [`Settlement::RealTime`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyAdder {
    adder: UsdPerMwh,
}

impl EnergyAdder {
    /// `EnergyAdder::new(adder)` is that non-zero adder. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::IllegalRate`] when `adder` is negative or not finite.
    /// [`TariffError::ZeroAdder`] when `adder` is zero.
    pub fn new(adder: f64) -> Result<Self, TariffError> {
        let adder = require_non_negative_finite(adder)?;
        if is_zero(adder) {
            return Err(TariffError::ZeroAdder);
        }
        Ok(Self {
            adder: UsdPerMwh::new(adder),
        })
    }

    /// The adder in dollars per megawatt-hour. **Calculation.**
    #[must_use]
    pub const fn get(self) -> UsdPerMwh {
        self.adder
    }
}

/// True for positive and negative zero.
pub(crate) fn is_zero(value: f64) -> bool {
    value.to_bits() == 0.0_f64.to_bits() || value.to_bits() == (-0.0_f64).to_bits()
}

/// How energy is priced for a contract.
///
/// Not [`Copy`]: a time-of-use schedule owns its periods.
#[derive(Debug, Clone, PartialEq)]
pub enum Settlement {
    /// Each hour billed at the planning / day-ahead series.
    DamIndexed,
    /// Each hour billed at the settlement / real-time series.
    RealTime,
    /// Each hour billed at the same flat dollars-per-megawatt-hour rate.
    Hedged(HedgedRate),
    /// Day-ahead series plus a non-zero adder.
    DamIndexedAdder(EnergyAdder),
    /// Real-time series plus a non-zero adder.
    RealTimeAdder(EnergyAdder),
    /// Civil local time selects a period rate. This crate does not convert UTC.
    Tou(TouSchedule),
}

/// The two energy price series a site may be billed against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyPrices<'a> {
    /// Planning / day-ahead prices, one per load hour, dollars per megawatt-hour.
    pub dam: &'a [UsdPerMwh],
    /// Settlement / real-time prices, one per load hour, dollars per megawatt-hour.
    pub real_time: &'a [UsdPerMwh],
}

/// `energy_bill(settlement, load_kwh_by_hour, prices)` is kWh × dollars-per-megawatt-hour / 1000.
///
/// `DamIndexed` uses the DAM series. `RealTime` uses the settlement series.
/// An adder variant adds its dollars-per-megawatt-hour to that series.
/// `Hedged` uses the flat rate every hour and ignores `prices`. [`Settlement::Tou`]
/// needs civil time: use [`energy_bill_local`]. This crate does not apply a
/// utilisation factor; that belongs to a scorer.
///
/// **Calculation.**
///
/// # Errors
/// [`TariffError::SeriesLengthMismatch`] when a series settlement is used and
/// `load_kwh_by_hour` and the chosen price series have different lengths.
/// [`TariffError::MissingCivilTime`] for [`Settlement::Tou`].
pub fn energy_bill(
    settlement: &Settlement,
    load_kwh_by_hour: &[KilowattHour],
    prices: &EnergyPrices<'_>,
) -> Result<Usd, TariffError> {
    energy_bill_local(settlement, load_kwh_by_hour, &[], prices)
}

/// `energy_bill_local(settlement, load_kwh_by_hour, local, prices)` is [`energy_bill`]
/// with civil local time for a time-of-use settlement.
///
/// `local` is ignored unless `settlement` is [`Settlement::Tou`], in which case
/// `local[i]` is the civil time of `load_kwh_by_hour[i]`. This crate does not
/// convert UTC. **Calculation.**
///
/// # Errors
/// [`TariffError::SeriesLengthMismatch`], [`TariffError::CivilTimeLengthMismatch`],
/// or [`TariffError::UnmatchedTouInterval`].
pub fn energy_bill_local(
    settlement: &Settlement,
    load_kwh_by_hour: &[KilowattHour],
    local: &[CivilMinute],
    prices: &EnergyPrices<'_>,
) -> Result<Usd, TariffError> {
    match settlement {
        Settlement::DamIndexed => bill_against(load_kwh_by_hour, prices.dam, 0.0),
        Settlement::RealTime => bill_against(load_kwh_by_hour, prices.real_time, 0.0),
        Settlement::Hedged(hedged) => Ok(bill_flat(load_kwh_by_hour, hedged.flat())),
        Settlement::DamIndexedAdder(adder) => {
            bill_against(load_kwh_by_hour, prices.dam, adder.get().get())
        }
        Settlement::RealTimeAdder(adder) => {
            bill_against(load_kwh_by_hour, prices.real_time, adder.get().get())
        }
        Settlement::Tou(schedule) => {
            if local.is_empty() && !load_kwh_by_hour.is_empty() {
                return Err(TariffError::MissingCivilTime);
            }
            tou_energy(schedule, load_kwh_by_hour, local)
        }
    }
}

fn bill_against(
    load: &[KilowattHour],
    prices: &[UsdPerMwh],
    adder: f64,
) -> Result<Usd, TariffError> {
    if load.len() != prices.len() {
        return Err(TariffError::SeriesLengthMismatch {
            load: load.len(),
            prices: prices.len(),
        });
    }
    let dollars = load
        .iter()
        .zip(prices)
        .map(|(kwh, price)| kwh.get() * (price.get() + adder) / 1000.0)
        .sum();
    Ok(Usd::new(dollars))
}

fn bill_flat(load: &[KilowattHour], flat: UsdPerMwh) -> Usd {
    let dollars = load.iter().map(|kwh| kwh.get() * flat.get() / 1000.0).sum();
    Usd::new(dollars)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_two_hours() -> [KilowattHour; 2] {
        [KilowattHour::new(1000.0), KilowattHour::new(1000.0)]
    }

    #[test]
    fn dam_indexed_two_hours_is_thirty_dollars() {
        // 1000 kWh × 10 $/MWh / 1000 + 1000 kWh × 20 $/MWh / 1000 = 10 + 20 = 30.
        let prices = EnergyPrices {
            dam: &[UsdPerMwh::new(10.0), UsdPerMwh::new(20.0)],
            real_time: &[UsdPerMwh::new(5.0), UsdPerMwh::new(100.0)],
        };
        let bill = energy_bill(&Settlement::DamIndexed, &load_two_hours(), &prices)
            .expect("matching lengths");
        assert_eq!(bill, Usd::new(30.0));
    }

    #[test]
    fn real_time_two_hours_is_one_hundred_five_dollars() {
        // 1000 kWh × 5 $/MWh / 1000 + 1000 kWh × 100 $/MWh / 1000 = 5 + 100 = 105.
        let prices = EnergyPrices {
            dam: &[UsdPerMwh::new(10.0), UsdPerMwh::new(20.0)],
            real_time: &[UsdPerMwh::new(5.0), UsdPerMwh::new(100.0)],
        };
        let bill = energy_bill(&Settlement::RealTime, &load_two_hours(), &prices)
            .expect("matching lengths");
        assert_eq!(bill, Usd::new(105.0));
    }

    #[test]
    fn hedged_flat_forty_five_two_hours_is_ninety_dollars() {
        // 1000 kWh × 45 $/MWh / 1000 × 2 hours = 90. Prices are ignored.
        let prices = EnergyPrices {
            dam: &[UsdPerMwh::new(10.0), UsdPerMwh::new(20.0)],
            real_time: &[UsdPerMwh::new(5.0), UsdPerMwh::new(100.0)],
        };
        let hedged = HedgedRate::new(45.0).expect("valid flat");
        let bill = energy_bill(&Settlement::Hedged(hedged), &load_two_hours(), &prices)
            .expect("hedged ignores prices");
        assert_eq!(bill, Usd::new(90.0));
    }

    #[test]
    fn hedged_rate_flat_returns_the_stored_price() {
        let hedged = HedgedRate::new(45.0).expect("valid flat");
        assert_eq!(HedgedRate::flat(hedged), UsdPerMwh::new(45.0));
    }

    #[test]
    fn dam_indexed_refuses_length_mismatch() {
        let prices = EnergyPrices {
            dam: &[UsdPerMwh::new(10.0)],
            real_time: &[],
        };
        assert_eq!(
            energy_bill(&Settlement::DamIndexed, &load_two_hours(), &prices),
            Err(TariffError::SeriesLengthMismatch { load: 2, prices: 1 })
        );
    }

    #[test]
    fn hedged_refuses_a_negative_flat_rate() {
        assert!(matches!(
            HedgedRate::new(-1.0),
            Err(TariffError::IllegalRate { .. })
        ));
    }

    #[test]
    fn energy_adder_stores_a_non_zero_rate_and_refuses_zero_and_negative() {
        let adder = EnergyAdder::new(5.0).expect("non-zero");
        assert_eq!(EnergyAdder::get(adder), UsdPerMwh::new(5.0));
        assert_eq!(EnergyAdder::new(0.0).unwrap_err(), TariffError::ZeroAdder);
        assert!(matches!(
            EnergyAdder::new(-1.0),
            Err(TariffError::IllegalRate { .. })
        ));
    }

    #[test]
    fn dam_indexed_adder_adds_five_dollars_per_megawatt_hour() {
        // (10+5) + (20+5) = 40, each hour 1000 kWh.
        let prices = EnergyPrices {
            dam: &[UsdPerMwh::new(10.0), UsdPerMwh::new(20.0)],
            real_time: &[UsdPerMwh::new(5.0), UsdPerMwh::new(100.0)],
        };
        let adder = EnergyAdder::new(5.0).expect("non-zero");
        let bill = energy_bill(
            &Settlement::DamIndexedAdder(adder),
            &load_two_hours(),
            &prices,
        )
        .expect("matching lengths");
        assert_eq!(bill, Usd::new(40.0));
        let real_time = energy_bill(
            &Settlement::RealTimeAdder(adder),
            &load_two_hours(),
            &prices,
        )
        .expect("matching lengths");
        // (5+5) + (100+5) = 115.
        assert_eq!(real_time, Usd::new(115.0));
    }

    #[test]
    fn energy_bill_local_prices_four_intervals_across_a_time_of_use_boundary() {
        use crate::tou::{CivilMinute, TouPeriod, TouSchedule};
        let off = TouPeriod::new("off", (1..=12).collect(), (1..=7).collect(), 0, 720, 10.0)
            .expect("valid");
        let on = TouPeriod::new("on", (1..=12).collect(), (1..=7).collect(), 720, 1440, 40.0)
            .expect("valid");
        let schedule = TouSchedule::new(vec![off, on]).expect("valid");
        let load = [KilowattHour::new(1000.0); 4];
        let local =
            [600_u16, 690, 720, 780].map(|minute| CivilMinute::new(6, 3, minute).expect("valid"));
        let prices = EnergyPrices {
            dam: &[],
            real_time: &[],
        };
        let bill =
            energy_bill_local(&Settlement::Tou(schedule), &load, &local, &prices).expect("matched");
        // 10 + 10 + 40 + 40.
        assert_eq!(bill, Usd::new(100.0));
    }

    #[test]
    fn energy_bill_without_civil_time_refuses_time_of_use() {
        use crate::tou::{TouPeriod, TouSchedule};
        let off = TouPeriod::new("off", vec![6], vec![3], 0, 1440, 10.0).expect("valid");
        let schedule = TouSchedule::new(vec![off]).expect("valid");
        let prices = EnergyPrices {
            dam: &[],
            real_time: &[],
        };
        assert_eq!(
            energy_bill(&Settlement::Tou(schedule), &load_two_hours(), &prices).unwrap_err(),
            TariffError::MissingCivilTime
        );
    }
}
