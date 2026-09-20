//! One ledger per present charge, in contract order. There is no total.

use crate::TariffError;
use crate::contract::{Charge, ChargeName, Kilowatt, KilowattHour, SiteTariff, Usd, YearMonth};
use crate::demand::{DemandHistory, demand_bill};
use crate::peak::coincident_peak_exposure;
use crate::settlement::{EnergyPrices, energy_bill};

/// Inputs every present charge may need. Unused fields are ignored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgerInputs<'a> {
    /// Hourly energy, kWh, in hour order.
    pub load_kwh_by_hour: &'a [KilowattHour],
    /// Day-ahead and real-time dollars-per-megawatt-hour series.
    pub prices: EnergyPrices<'a>,
    /// This month's demand-window kW samples.
    pub kw_by_interval: &'a [Kilowatt],
    /// Billed month for demand.
    pub month: YearMonth,
    /// Prior months' demand peaks.
    pub history: &'a DemandHistory,
    /// kW at each coincident-peak interval, length `intervals_per_year`.
    pub coincident_kw: &'a [Kilowatt],
}

/// `ledgers(tariff, inputs)` is one `(name, amount)` per present charge.
///
/// Order is contract order, then charge order inside each contract. A charge
/// type that is not on the tariff is absent from the result. Ledgers are never
/// netted: there is no `total()`.
///
/// **Calculation.**
///
/// # Errors
/// [`TariffError::SeriesLengthMismatch`] or [`TariffError::IntervalCountMismatch`]
/// from the charge that needs those inputs.
#[allow(
    clippy::module_name_repetitions,
    reason = "public name is specified as ledgers"
)]
pub fn ledgers(
    tariff: &SiteTariff,
    inputs: &LedgerInputs<'_>,
) -> Result<Vec<(ChargeName, Usd)>, TariffError> {
    let mut entries = Vec::new();
    for contract in tariff.contracts() {
        for charge in contract.charges() {
            let amount = match charge {
                Charge::Energy(settlement) => {
                    energy_bill(settlement, inputs.load_kwh_by_hour, &inputs.prices)?
                }
                Charge::Demand(demand) => {
                    demand_bill(demand, inputs.kw_by_interval, inputs.month, inputs.history)
                }
                Charge::CoincidentPeak(rule) => {
                    coincident_peak_exposure(rule, inputs.coincident_kw)?.amount()
                }
                Charge::Fixed(fixed) => fixed.amount(),
            };
            let kind = match charge {
                Charge::Energy(_) => "energy",
                Charge::Demand(_) => "demand",
                Charge::CoincidentPeak(_) => "coincident_peak",
                Charge::Fixed(_) => "fixed",
            };
            entries.push((
                ChargeName::new(format!("{}:{kind}", contract.name())),
                amount,
            ));
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UsdPerMwh;
    use crate::contract::{
        CalendarDate, Charge, Contract, DateRange, FixedCharge, MeterScope, Minutes, PassThrough,
        RateUnit, RateWithSource, SiteAlias, SiteTariff,
    };
    use crate::demand::DemandCharge;
    use crate::peak::CoincidentPeakRule;
    use crate::settlement::{HedgedRate, Settlement};

    fn period() -> DateRange {
        DateRange::new(
            CalendarDate::new(2026, 1, 1).expect("valid"),
            CalendarDate::new(2026, 12, 31).expect("valid"),
        )
        .expect("valid")
    }

    fn scope() -> MeterScope {
        MeterScope::new("site").expect("valid")
    }

    fn dated() -> CalendarDate {
        CalendarDate::new(2026, 1, 1).expect("valid")
    }

    const LOAD: [KilowattHour; 2] = [KilowattHour::new(1000.0), KilowattHour::new(1000.0)];
    const DAM: [UsdPerMwh; 2] = [UsdPerMwh::new(10.0), UsdPerMwh::new(20.0)];
    const RT: [UsdPerMwh; 2] = [UsdPerMwh::new(5.0), UsdPerMwh::new(100.0)];
    const KW: [Kilowatt; 1] = [Kilowatt::new(100.0)];

    fn two_hour_inputs<'a>(
        history: &'a DemandHistory,
        coincident: &'a [Kilowatt],
    ) -> LedgerInputs<'a> {
        LedgerInputs {
            load_kwh_by_hour: &LOAD,
            prices: EnergyPrices {
                dam: &DAM,
                real_time: &RT,
            },
            kw_by_interval: &KW,
            month: YearMonth::new(2026, 3).expect("valid"),
            history,
            coincident_kw: coincident,
        }
    }

    #[test]
    fn ledgers_returns_exactly_the_present_charges_in_contract_order() {
        let energy = Contract::new(
            "first",
            scope(),
            period(),
            vec![Charge::Energy(Settlement::DamIndexed)],
        )
        .expect("valid");
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
        let demand_contract =
            Contract::new("second", scope(), period(), vec![Charge::Demand(demand)])
                .expect("valid");
        let tariff = SiteTariff::new(
            SiteAlias::parse("synthetic-ledgers").expect("alias"),
            vec![energy, demand_contract],
        );
        let history = DemandHistory::new();
        let coincident = [];
        let entries = ledgers(&tariff, &two_hour_inputs(&history, &coincident)).expect("ok");
        let names: Vec<_> = entries.iter().map(|(name, _)| name.get()).collect();
        assert_eq!(names, ["first:energy", "second:demand"]);
        assert_eq!(entries[0].1, Usd::new(30.0));
        assert_eq!(entries[1].1, Usd::new(1000.0));
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn absent_charge_types_do_not_appear() {
        let contract = Contract::new(
            "only-fixed",
            scope(),
            period(),
            vec![Charge::Fixed(FixedCharge::new(7.0).expect("valid"))],
        )
        .expect("valid");
        let tariff = SiteTariff::new(
            SiteAlias::parse("synthetic-fixed").expect("alias"),
            vec![contract],
        );
        let history = DemandHistory::new();
        let entries = ledgers(&tariff, &two_hour_inputs(&history, &[])).expect("ok");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0.get(), "only-fixed:fixed");
        assert_eq!(entries[0].1, Usd::new(7.0));
    }

    #[test]
    fn hedged_energy_uses_the_flat_rate() {
        let hedged = HedgedRate::new(45.0).expect("valid");
        let contract = Contract::new(
            "hedged",
            scope(),
            period(),
            vec![Charge::Energy(Settlement::Hedged(hedged))],
        )
        .expect("valid");
        let tariff = SiteTariff::new(
            SiteAlias::parse("synthetic-hedged").expect("alias"),
            vec![contract],
        );
        let history = DemandHistory::new();
        let entries = ledgers(&tariff, &two_hour_inputs(&history, &[])).expect("ok");
        assert_eq!(entries[0].1, Usd::new(90.0));
    }

    #[test]
    fn coincident_peak_rule_is_present_when_listed() {
        let rate = RateWithSource::new(
            10.0,
            RateUnit::UsdPerKwYear,
            "SYNTHETIC ESTIMATE",
            dated(),
            false,
        )
        .expect("valid");
        let rule = CoincidentPeakRule::new(
            4,
            Minutes::positive(15).expect("positive"),
            vec![6, 7, 8, 9],
            rate,
            PassThrough::Assumed,
        )
        .expect("valid");
        let contract = Contract::new(
            "fourcp",
            scope(),
            period(),
            vec![Charge::CoincidentPeak(rule)],
        )
        .expect("valid");
        let tariff = SiteTariff::new(
            SiteAlias::parse("synthetic-4cp").expect("alias"),
            vec![contract],
        );
        let history = DemandHistory::new();
        let coincident = [Kilowatt::new(10.0); 4];
        let entries = ledgers(&tariff, &two_hour_inputs(&history, &coincident)).expect("ok");
        assert_eq!(entries[0].0.get(), "fourcp:coincident_peak");
        assert_eq!(entries[0].1, Usd::new(100.0));
    }
}
