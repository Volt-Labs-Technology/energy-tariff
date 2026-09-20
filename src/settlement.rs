//! Energy settlement: DAM-indexed, real-time, or a hedged flat rate.

use crate::contract::{KilowattHour, Usd, UsdPerMwh};
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

/// How energy is priced for a contract.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Settlement {
    /// Each hour billed at the planning / day-ahead series.
    DamIndexed,
    /// Each hour billed at the settlement / real-time series.
    RealTime,
    /// Each hour billed at the same flat dollars-per-megawatt-hour rate.
    Hedged(HedgedRate),
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
/// `Hedged` uses the flat rate every hour and ignores `prices`. This crate does
/// not apply a utilisation factor; that belongs to a scorer.
///
/// **Calculation.**
///
/// # Errors
/// [`TariffError::SeriesLengthMismatch`] when a series settlement is used and
/// `load_kwh_by_hour` and the chosen price series have different lengths.
pub fn energy_bill(
    settlement: &Settlement,
    load_kwh_by_hour: &[KilowattHour],
    prices: &EnergyPrices<'_>,
) -> Result<Usd, TariffError> {
    match settlement {
        Settlement::DamIndexed => bill_against(load_kwh_by_hour, prices.dam),
        Settlement::RealTime => bill_against(load_kwh_by_hour, prices.real_time),
        Settlement::Hedged(hedged) => Ok(bill_flat(load_kwh_by_hour, hedged.flat())),
    }
}

fn bill_against(load: &[KilowattHour], prices: &[UsdPerMwh]) -> Result<Usd, TariffError> {
    if load.len() != prices.len() {
        return Err(TariffError::SeriesLengthMismatch {
            load: load.len(),
            prices: prices.len(),
        });
    }
    let dollars = load
        .iter()
        .zip(prices)
        .map(|(kwh, price)| kwh.get() * price.get() / 1000.0)
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
        assert_eq!(hedged.flat(), UsdPerMwh::new(45.0));
        let bill = energy_bill(&Settlement::Hedged(hedged), &load_two_hours(), &prices)
            .expect("hedged ignores prices");
        assert_eq!(bill, Usd::new(90.0));
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
}
