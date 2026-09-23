//! A month bill with four fields. There is no total.
//!
//! Coincident peak is not one of the four fields.

use crate::TariffError;
use crate::contract::{Charge, Kilowatt, KilowattHour, SiteTariff, Usd, YearMonth};
use crate::demand::{DemandHistory, demand_charge};
use crate::facilities::facilities_bill;
use crate::settlement::{EnergyPrices, energy_bill_local};
use crate::tou::CivilMinute;

/// Inputs for [`month_bill`]. Unused slices are ignored. There is no clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonthInputs<'a> {
    /// Hourly energy, kWh, in hour order.
    pub load_kwh_by_hour: &'a [KilowattHour],
    /// Civil local time of each load hour. Required when energy is time-of-use.
    /// Ignored otherwise. This crate does not convert UTC.
    pub local: &'a [CivilMinute],
    /// Day-ahead and real-time dollars-per-megawatt-hour series.
    pub prices: EnergyPrices<'a>,
    /// This month's demand-window kW samples.
    pub kw_by_interval: &'a [Kilowatt],
    /// Billed month.
    pub month: YearMonth,
    /// Prior months' demand peaks.
    pub history: &'a DemandHistory,
}

/// Energy, demand, facilities, and fixed for one month.
///
/// There is no `total()`. Coincident peak is not here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonthBill {
    energy: Usd,
    demand: Usd,
    facilities: Usd,
    fixed: Usd,
}

impl MonthBill {
    /// Energy dollars. **Calculation.**
    #[must_use]
    pub const fn energy(self) -> Usd {
        self.energy
    }

    /// Demand dollars. **Calculation.**
    #[must_use]
    pub const fn demand(self) -> Usd {
        self.demand
    }

    /// Facilities dollars. **Calculation.**
    #[must_use]
    pub const fn facilities(self) -> Usd {
        self.facilities
    }

    /// Fixed dollars. **Calculation.**
    #[must_use]
    pub const fn fixed(self) -> Usd {
        self.fixed
    }
}

/// `month_bill(tariff, inputs)` sums each of the four fields across every contract.
///
/// A charge type that is absent contributes zero for its field. Coincident peak
/// is skipped. The same inputs produce the same bill; nothing reads a clock.
///
/// **Calculation.**
///
/// # Errors
/// [`TariffError::SeriesLengthMismatch`], [`TariffError::CivilTimeLengthMismatch`],
/// [`TariffError::UnmatchedTouInterval`], or [`TariffError::MissingCivilTime`]
/// from an energy charge that needs them.
#[allow(
    clippy::module_name_repetitions,
    reason = "public name is specified as month_bill"
)]
pub fn month_bill(tariff: &SiteTariff, inputs: &MonthInputs<'_>) -> Result<MonthBill, TariffError> {
    let mut energy = 0.0;
    let mut demand = 0.0;
    let mut facilities = 0.0;
    let mut fixed = 0.0;
    for contract in tariff.contracts() {
        let ratcheted_kw = ratcheted_kilowatts(contract.charges(), inputs);
        for charge in contract.charges() {
            match charge {
                Charge::Energy(settlement) => {
                    energy += energy_bill_local(
                        settlement,
                        inputs.load_kwh_by_hour,
                        inputs.local,
                        &inputs.prices,
                    )?
                    .get();
                }
                Charge::Demand(demand_charge_spec) => {
                    demand += demand_charge(
                        demand_charge_spec,
                        inputs.kw_by_interval,
                        inputs.month,
                        inputs.history,
                    )
                    .charge()
                    .get();
                }
                Charge::Facilities(facilities_charge) => {
                    facilities +=
                        facilities_bill(facilities_charge, Kilowatt::new(ratcheted_kw)).get();
                }
                Charge::Fixed(fixed_charge) => fixed += fixed_charge.amount().get(),
                Charge::CoincidentPeak(_) => {}
            }
        }
    }
    Ok(MonthBill {
        energy: Usd::new(energy),
        demand: Usd::new(demand),
        facilities: Usd::new(facilities),
        fixed: Usd::new(fixed),
    })
}

fn ratcheted_kilowatts(charges: &[Charge], inputs: &MonthInputs<'_>) -> f64 {
    charges
        .iter()
        .filter_map(|charge| match charge {
            Charge::Demand(demand_charge_spec) => Some(
                demand_charge(
                    demand_charge_spec,
                    inputs.kw_by_interval,
                    inputs.month,
                    inputs.history,
                )
                .billed_kw()
                .get(),
            ),
            _ => None,
        })
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PassThrough;
    use crate::UsdPerMwh;
    use crate::contract::{
        CalendarDate, Contract, DateRange, FixedCharge, MeterScope, Minutes, RateUnit,
        RateWithSource, SiteAlias,
    };
    use crate::demand::DemandCharge;
    use crate::facilities::{FacilitiesBasis, FacilitiesCharge};
    use crate::peak::CoincidentPeakRule;
    use crate::settlement::{EnergyPrices, Settlement};

    fn period() -> DateRange {
        DateRange::new(
            CalendarDate::new(2026, 1, 1).expect("valid"),
            CalendarDate::new(2026, 12, 31).expect("valid"),
        )
        .expect("valid")
    }

    fn dated() -> CalendarDate {
        CalendarDate::new(2026, 1, 1).expect("valid")
    }

    fn inputs(history: &DemandHistory) -> MonthInputs<'_> {
        const LOAD: [KilowattHour; 2] = [KilowattHour::new(1000.0), KilowattHour::new(1000.0)];
        const DAM: [UsdPerMwh; 2] = [UsdPerMwh::new(10.0), UsdPerMwh::new(20.0)];
        const RT: [UsdPerMwh; 2] = [UsdPerMwh::new(5.0), UsdPerMwh::new(100.0)];
        const KW: [Kilowatt; 1] = [Kilowatt::new(100.0)];
        MonthInputs {
            load_kwh_by_hour: &LOAD,
            local: &[],
            prices: EnergyPrices {
                dam: &DAM,
                real_time: &RT,
            },
            kw_by_interval: &KW,
            month: YearMonth::new(2026, 3).expect("valid"),
            history,
        }
    }

    #[test]
    fn month_bill_is_four_fields_and_the_same_inputs_match() {
        // SYNTHETIC. Not a real tariff.
        let demand_rate = RateWithSource::new(
            10.0,
            RateUnit::UsdPerKwMonth,
            "SYNTHETIC ESTIMATE",
            dated(),
            false,
        )
        .expect("valid");
        let demand = DemandCharge::new(
            demand_rate,
            Minutes::demand_window(15).expect("15"),
            None,
            None,
        )
        .expect("valid");
        let facilities = FacilitiesCharge::new(
            2.0,
            FacilitiesBasis::ContractKw {
                contract_kw: Kilowatt::new(50.0),
            },
        )
        .expect("valid");
        let peak_rate = RateWithSource::new(
            50.0,
            RateUnit::UsdPerKwYear,
            "SYNTHETIC ESTIMATE",
            dated(),
            false,
        )
        .expect("valid");
        let coincident = CoincidentPeakRule::new(
            4,
            Minutes::positive(15).expect("positive"),
            vec![6, 7, 8, 9],
            peak_rate,
            PassThrough::Assumed,
        )
        .expect("valid");
        let contract = Contract::new(
            "primary",
            MeterScope::new("site").expect("valid"),
            period(),
            vec![
                Charge::Energy(Settlement::DamIndexed),
                Charge::Demand(demand),
                Charge::Facilities(facilities),
                Charge::Fixed(FixedCharge::new(7.0).expect("valid")),
                Charge::CoincidentPeak(coincident),
            ],
        )
        .expect("valid");
        let tariff = SiteTariff::new(
            SiteAlias::parse("synthetic-month").expect("alias"),
            vec![contract],
        );
        let history = DemandHistory::new();
        let first = month_bill(&tariff, &inputs(&history)).expect("bill");
        let second = month_bill(&tariff, &inputs(&history)).expect("same bill");
        assert_eq!(first, second);
        // Energy 30, demand 100 kW × $10, facilities 50 kW × $2, fixed 7.
        // Coincident peak is not added to any field.
        assert_eq!(MonthBill::energy(first), Usd::new(30.0));
        assert_eq!(MonthBill::demand(first), Usd::new(1000.0));
        assert_eq!(MonthBill::facilities(first), Usd::new(100.0));
        assert_eq!(MonthBill::fixed(first), Usd::new(7.0));
    }
}
