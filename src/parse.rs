//! JSON and TOML adapters. Wire values become domain types through constructors.

use crate::TariffError;
use crate::contract::{
    CalendarDate, Charge, Contract, DateRange, FixedCharge, Kilowatt, MeterScope, Minutes,
    PassThrough, RateUnit, RateWithSource, SiteAlias, SiteTariff, TimeOfUse,
};
use crate::demand::{DemandCharge, Ratchet};
use crate::facilities::{FacilitiesBasis, FacilitiesCharge};
use crate::peak::CoincidentPeakRule;
use crate::settlement::{EnergyAdder, HedgedRate, Settlement, is_zero};
use crate::tou::{TouPeriod, TouSchedule};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

impl SiteTariff {
    /// `SiteTariff::from_json(text)` is the tariff that JSON describes.
    /// Constructors still refuse illegal values. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::Json`] on a schema or constructor failure.
    pub fn from_json(text: &str) -> Result<Self, TariffError> {
        Ok(serde_json::from_str(text)?)
    }

    /// `SiteTariff::from_toml(text)` is the tariff that TOML describes.
    /// Constructors still refuse illegal values. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::Toml`] on a schema or constructor failure.
    pub fn from_toml(text: &str) -> Result<Self, TariffError> {
        Ok(toml::from_str(text)?)
    }

    /// JSON encoding of this tariff, same schema as [`Self::from_json`].
    /// **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::Json`] if serialisation fails.
    pub fn to_json(&self) -> Result<String, TariffError> {
        Ok(serde_json::to_string(self)?)
    }

    /// TOML encoding of this tariff, same schema as [`Self::from_toml`].
    /// **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::Toml`] if serialisation fails.
    pub fn to_toml(&self) -> Result<String, TariffError> {
        Ok(toml::to_string(self)?)
    }
}

impl Serialize for SiteTariff {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WireSiteTariff::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SiteTariff {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireSiteTariff::deserialize(deserializer)?;
        Self::try_from(wire).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireSiteTariff {
    site: String,
    contracts: Vec<WireContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireContract {
    name: String,
    applies_to: String,
    period: WireDateRange,
    charges: Vec<WireCharge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireDateRange {
    start: String,
    end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireCharge {
    Energy {
        #[serde(flatten)]
        settlement: WireSettlement,
    },
    Demand {
        rate: WireRate,
        window: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        ratchet: Option<WireRatchet>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hours: Option<WireTimeOfUse>,
    },
    CoincidentPeak {
        intervals_per_year: u8,
        interval_minutes: u16,
        months: Vec<u8>,
        rate: WireRate,
        pass_through: WirePassThrough,
    },
    Fixed {
        amount: f64,
    },
    Facilities {
        rate_per_kw: f64,
        basis: WireFacilitiesBasis,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        contract_kw: Option<f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireSettlement {
    DamIndexed,
    RealTime,
    Hedged { flat: f64 },
    DamIndexedAdder { adder: f64 },
    RealTimeAdder { adder: f64 },
    Tou { periods: Vec<WireTouPeriod> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireTouPeriod {
    name: String,
    months: Vec<u8>,
    weekdays: Vec<u8>,
    start_min: u16,
    end_min: u16,
    rate: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireFacilitiesBasis {
    ContractKw,
    RatchetedPeak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireRate {
    value: f64,
    unit: WireRateUnit,
    source: String,
    dated: String,
    verified: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    clippy::enum_variant_names,
    reason = "wire names match RateUnit; serde rename_all is the schema"
)]
enum WireRateUnit {
    UsdPerKwMonth,
    UsdPerKwYear,
    UsdPerMwh,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct WireRatchet {
    pct: f64,
    months: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct WireTimeOfUse {
    start_hour: u8,
    end_hour: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WirePassThrough {
    Confirmed,
    Assumed,
    NotPassedThrough,
}

impl From<&SiteTariff> for WireSiteTariff {
    fn from(tariff: &SiteTariff) -> Self {
        Self {
            site: tariff.site().get().to_owned(),
            contracts: tariff.contracts().iter().map(WireContract::from).collect(),
        }
    }
}

impl From<&Contract> for WireContract {
    fn from(contract: &Contract) -> Self {
        Self {
            name: contract.name().to_owned(),
            applies_to: contract.applies_to().get().to_owned(),
            period: WireDateRange {
                start: contract.period().start().to_iso(),
                end: contract.period().end().to_iso(),
            },
            charges: contract.charges().iter().map(WireCharge::from).collect(),
        }
    }
}

impl From<&Charge> for WireCharge {
    fn from(charge: &Charge) -> Self {
        match charge {
            Charge::Energy(settlement) => Self::Energy {
                settlement: WireSettlement::from(settlement),
            },
            Charge::Demand(demand) => Self::Demand {
                rate: WireRate::from(demand.rate()),
                window: demand.window().get(),
                ratchet: demand.ratchet().map(WireRatchet::from),
                hours: demand.hours().map(WireTimeOfUse::from),
            },
            Charge::CoincidentPeak(rule) => Self::CoincidentPeak {
                intervals_per_year: rule.intervals_per_year(),
                interval_minutes: rule.interval_minutes().get(),
                months: rule.months().to_vec(),
                rate: WireRate::from(rule.rate()),
                pass_through: WirePassThrough::from(rule.pass_through()),
            },
            Charge::Fixed(fixed) => Self::Fixed {
                amount: fixed.amount().get(),
            },
            Charge::Facilities(facilities) => {
                let (basis, contract_kw) = match facilities.basis() {
                    FacilitiesBasis::ContractKw { contract_kw } => {
                        (WireFacilitiesBasis::ContractKw, Some(contract_kw.get()))
                    }
                    FacilitiesBasis::RatchetedPeak => (WireFacilitiesBasis::RatchetedPeak, None),
                };
                Self::Facilities {
                    rate_per_kw: facilities.rate_per_kw(),
                    basis,
                    contract_kw,
                }
            }
        }
    }
}

impl From<&Settlement> for WireSettlement {
    fn from(settlement: &Settlement) -> Self {
        match settlement {
            Settlement::DamIndexed => Self::DamIndexed,
            Settlement::RealTime => Self::RealTime,
            Settlement::Hedged(hedged) => Self::Hedged {
                flat: hedged.flat().get(),
            },
            Settlement::DamIndexedAdder(adder) => Self::DamIndexedAdder {
                adder: adder.get().get(),
            },
            Settlement::RealTimeAdder(adder) => Self::RealTimeAdder {
                adder: adder.get().get(),
            },
            Settlement::Tou(schedule) => Self::Tou {
                periods: schedule.periods().iter().map(WireTouPeriod::from).collect(),
            },
        }
    }
}

impl From<&TouPeriod> for WireTouPeriod {
    fn from(period: &TouPeriod) -> Self {
        Self {
            name: period.name().to_owned(),
            months: period.months().to_vec(),
            weekdays: period.weekdays().to_vec(),
            start_min: period.start_min(),
            end_min: period.end_min(),
            rate: period.rate().get(),
        }
    }
}

impl From<&RateWithSource> for WireRate {
    fn from(rate: &RateWithSource) -> Self {
        Self {
            value: rate.value(),
            unit: WireRateUnit::from(rate.unit()),
            source: rate.source().to_owned(),
            dated: rate.dated().to_iso(),
            verified: rate.verified(),
        }
    }
}

impl From<RateUnit> for WireRateUnit {
    fn from(unit: RateUnit) -> Self {
        match unit {
            RateUnit::UsdPerKwMonth => Self::UsdPerKwMonth,
            RateUnit::UsdPerKwYear => Self::UsdPerKwYear,
            RateUnit::UsdPerMwh => Self::UsdPerMwh,
        }
    }
}

impl From<Ratchet> for WireRatchet {
    fn from(ratchet: Ratchet) -> Self {
        Self {
            pct: ratchet.pct(),
            months: ratchet.months(),
        }
    }
}

impl From<TimeOfUse> for WireTimeOfUse {
    fn from(hours: TimeOfUse) -> Self {
        Self {
            start_hour: hours.start_hour(),
            end_hour: hours.end_hour(),
        }
    }
}

impl From<PassThrough> for WirePassThrough {
    fn from(pass: PassThrough) -> Self {
        match pass {
            PassThrough::Confirmed => Self::Confirmed,
            PassThrough::Assumed => Self::Assumed,
            PassThrough::NotPassedThrough => Self::NotPassedThrough,
        }
    }
}

impl TryFrom<WireSiteTariff> for SiteTariff {
    type Error = TariffError;

    fn try_from(wire: WireSiteTariff) -> Result<Self, Self::Error> {
        let site = SiteAlias::parse(&wire.site)?;
        let contracts = wire
            .contracts
            .into_iter()
            .map(Contract::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(site, contracts))
    }
}

impl TryFrom<WireContract> for Contract {
    type Error = TariffError;

    fn try_from(wire: WireContract) -> Result<Self, Self::Error> {
        let applies_to = MeterScope::new(wire.applies_to)?;
        let start = CalendarDate::parse(&wire.period.start)?;
        let end = CalendarDate::parse(&wire.period.end)?;
        let period = DateRange::new(start, end)?;
        let charges = wire
            .charges
            .into_iter()
            .map(Charge::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(wire.name, applies_to, period, charges)
    }
}

impl TryFrom<WireCharge> for Charge {
    type Error = TariffError;

    fn try_from(wire: WireCharge) -> Result<Self, Self::Error> {
        match wire {
            WireCharge::Energy { settlement } => {
                Ok(Self::Energy(Settlement::try_from(settlement)?))
            }
            WireCharge::Demand {
                rate,
                window,
                ratchet,
                hours,
            } => {
                let charge = DemandCharge::new(
                    RateWithSource::try_from(rate)?,
                    Minutes::demand_window(window)?,
                    ratchet.map(Ratchet::try_from).transpose()?,
                    hours.map(TimeOfUse::try_from).transpose()?,
                )?;
                Ok(Self::Demand(charge))
            }
            WireCharge::CoincidentPeak {
                intervals_per_year,
                interval_minutes,
                months,
                rate,
                pass_through,
            } => {
                let rule = CoincidentPeakRule::new(
                    intervals_per_year,
                    Minutes::positive(interval_minutes)?,
                    months,
                    RateWithSource::try_from(rate)?,
                    PassThrough::from(pass_through),
                )?;
                Ok(Self::CoincidentPeak(rule))
            }
            WireCharge::Fixed { amount } => Ok(Self::Fixed(FixedCharge::new(amount)?)),
            WireCharge::Facilities {
                rate_per_kw,
                basis,
                contract_kw,
            } => {
                let basis = match (basis, contract_kw) {
                    (WireFacilitiesBasis::ContractKw, Some(kilowatts)) => {
                        FacilitiesBasis::ContractKw {
                            contract_kw: Kilowatt::new(kilowatts),
                        }
                    }
                    (WireFacilitiesBasis::ContractKw, None) => {
                        return Err(TariffError::MissingContractKw);
                    }
                    (WireFacilitiesBasis::RatchetedPeak, None) => FacilitiesBasis::RatchetedPeak,
                    (WireFacilitiesBasis::RatchetedPeak, Some(_)) => {
                        return Err(TariffError::UnexpectedContractKw);
                    }
                };
                Ok(Self::Facilities(FacilitiesCharge::new(rate_per_kw, basis)?))
            }
        }
    }
}

impl TryFrom<WireSettlement> for Settlement {
    type Error = TariffError;

    fn try_from(wire: WireSettlement) -> Result<Self, Self::Error> {
        match wire {
            WireSettlement::DamIndexed => Ok(Self::DamIndexed),
            WireSettlement::RealTime => Ok(Self::RealTime),
            WireSettlement::Hedged { flat } => Ok(Self::Hedged(HedgedRate::new(flat)?)),
            WireSettlement::DamIndexedAdder { adder } if is_zero(adder) => Ok(Self::DamIndexed),
            WireSettlement::RealTimeAdder { adder } if is_zero(adder) => Ok(Self::RealTime),
            WireSettlement::DamIndexedAdder { adder } => {
                Ok(Self::DamIndexedAdder(EnergyAdder::new(adder)?))
            }
            WireSettlement::RealTimeAdder { adder } => {
                Ok(Self::RealTimeAdder(EnergyAdder::new(adder)?))
            }
            WireSettlement::Tou { periods } => {
                let periods = periods
                    .into_iter()
                    .map(TouPeriod::try_from)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self::Tou(TouSchedule::new(periods)?))
            }
        }
    }
}

impl TryFrom<WireTouPeriod> for TouPeriod {
    type Error = TariffError;

    fn try_from(wire: WireTouPeriod) -> Result<Self, Self::Error> {
        Self::new(
            wire.name,
            wire.months,
            wire.weekdays,
            wire.start_min,
            wire.end_min,
            wire.rate,
        )
    }
}

impl TryFrom<WireRate> for RateWithSource {
    type Error = TariffError;

    fn try_from(wire: WireRate) -> Result<Self, Self::Error> {
        Self::new(
            wire.value,
            RateUnit::from(wire.unit),
            wire.source,
            CalendarDate::parse(&wire.dated)?,
            wire.verified,
        )
    }
}

impl From<WireRateUnit> for RateUnit {
    fn from(unit: WireRateUnit) -> Self {
        match unit {
            WireRateUnit::UsdPerKwMonth => Self::UsdPerKwMonth,
            WireRateUnit::UsdPerKwYear => Self::UsdPerKwYear,
            WireRateUnit::UsdPerMwh => Self::UsdPerMwh,
        }
    }
}

impl TryFrom<WireRatchet> for Ratchet {
    type Error = TariffError;

    fn try_from(wire: WireRatchet) -> Result<Self, Self::Error> {
        Self::new(wire.pct, wire.months)
    }
}

impl TryFrom<WireTimeOfUse> for TimeOfUse {
    type Error = TariffError;

    fn try_from(wire: WireTimeOfUse) -> Result<Self, Self::Error> {
        Self::new(wire.start_hour, wire.end_hour)
    }
}

impl From<WirePassThrough> for PassThrough {
    fn from(pass: WirePassThrough) -> Self {
        match pass {
            WirePassThrough::Confirmed => Self::Confirmed,
            WirePassThrough::Assumed => Self::Assumed,
            WirePassThrough::NotPassedThrough => Self::NotPassedThrough,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Usd;
    use crate::contract::{Kilowatt, KilowattHour, UsdPerMwh, YearMonth};
    use crate::demand::DemandHistory;
    use crate::ledgers::{LedgerInputs, ledgers};
    use crate::settlement::EnergyPrices;

    const SYNTHETIC_4CP_TOML: &str = include_str!("../testdata/synthetic_4cp.toml");
    const SYNTHETIC_4CP_JSON: &str = include_str!("../testdata/synthetic_4cp.json");
    const SYNTHETIC_DEMAND_TOML: &str = include_str!("../testdata/synthetic_demand_ratchet.toml");
    const SYNTHETIC_DEMAND_JSON: &str = include_str!("../testdata/synthetic_demand_ratchet.json");

    const LOAD: [KilowattHour; 2] = [KilowattHour::new(1000.0), KilowattHour::new(1000.0)];
    const DAM: [UsdPerMwh; 2] = [UsdPerMwh::new(10.0), UsdPerMwh::new(20.0)];
    const RT: [UsdPerMwh; 2] = [UsdPerMwh::new(5.0), UsdPerMwh::new(100.0)];
    const KW: [Kilowatt; 1] = [Kilowatt::new(100.0)];
    const FOUR_CP: [Kilowatt; 4] = [Kilowatt::new(100.0); 4];
    const NONE: [Kilowatt; 0] = [];

    fn four_cp_inputs(history: &DemandHistory) -> LedgerInputs<'_> {
        LedgerInputs {
            load_kwh_by_hour: &LOAD,
            prices: EnergyPrices {
                dam: &DAM,
                real_time: &RT,
            },
            local: &[],
            kw_by_interval: &KW,
            month: YearMonth::new(2026, 3).expect("valid"),
            history,
            coincident_kw: &FOUR_CP,
        }
    }

    fn demand_inputs(history: &DemandHistory) -> LedgerInputs<'_> {
        LedgerInputs {
            load_kwh_by_hour: &LOAD,
            prices: EnergyPrices {
                dam: &DAM,
                real_time: &RT,
            },
            local: &[],
            kw_by_interval: &KW,
            month: YearMonth::new(2026, 3).expect("valid"),
            history,
            coincident_kw: &NONE,
        }
    }

    #[test]
    fn synthetic_4cp_toml_and_json_are_the_same_tariff_and_the_same_ledgers() {
        let from_toml = SiteTariff::from_toml(SYNTHETIC_4CP_TOML).expect("toml");
        let from_json = SiteTariff::from_json(SYNTHETIC_4CP_JSON).expect("json");
        assert_eq!(from_toml, from_json);
        let history = DemandHistory::new();
        let toml_ledgers = ledgers(&from_toml, &four_cp_inputs(&history)).expect("toml ledgers");
        let json_ledgers = ledgers(&from_json, &four_cp_inputs(&history)).expect("json ledgers");
        assert_eq!(toml_ledgers, json_ledgers);
        assert_eq!(toml_ledgers.len(), 2);
        assert_eq!(toml_ledgers[0].0.get(), "primary:energy");
        assert_eq!(toml_ledgers[0].1, Usd::new(30.0));
        assert_eq!(toml_ledgers[1].0.get(), "primary:coincident_peak");
        // mean 100 kW × $50/kW-year ESTIMATE = $5000.
        assert_eq!(toml_ledgers[1].1, Usd::new(5000.0));
    }

    #[test]
    fn synthetic_demand_ratchet_toml_and_json_are_the_same_tariff_and_the_same_ledgers() {
        let from_toml = SiteTariff::from_toml(SYNTHETIC_DEMAND_TOML).expect("toml");
        let from_json = SiteTariff::from_json(SYNTHETIC_DEMAND_JSON).expect("json");
        assert_eq!(from_toml, from_json);
        let february = YearMonth::new(2026, 2).expect("valid");
        let history = DemandHistory::new().with_peak(february, Kilowatt::new(200.0));
        let toml_ledgers = ledgers(&from_toml, &demand_inputs(&history)).expect("toml ledgers");
        let json_ledgers = ledgers(&from_json, &demand_inputs(&history)).expect("json ledgers");
        assert_eq!(toml_ledgers, json_ledgers);
        assert_eq!(toml_ledgers[0].0.get(), "primary:energy");
        assert_eq!(toml_ledgers[0].1, Usd::new(30.0));
        assert_eq!(toml_ledgers[1].0.get(), "primary:demand");
        // max(100, 0.8 × 200) = 160 kW × $10/kW-month ESTIMATE = $1600.
        assert_eq!(toml_ledgers[1].1, Usd::new(1600.0));
    }

    #[test]
    fn json_round_trip_preserves_the_tariff() {
        let original = SiteTariff::from_json(SYNTHETIC_4CP_JSON).expect("json");
        let encoded = SiteTariff::to_json(&original).expect("encode");
        let again = SiteTariff::from_json(&encoded).expect("decode");
        assert_eq!(original, again);
    }

    #[test]
    fn toml_round_trip_preserves_the_tariff() {
        let original = SiteTariff::from_toml(SYNTHETIC_DEMAND_TOML).expect("toml");
        let encoded = SiteTariff::to_toml(&original).expect("encode");
        let again = SiteTariff::from_toml(&encoded).expect("decode");
        assert_eq!(original, again);
    }

    #[test]
    fn json_constructor_still_refuses_a_negative_rate() {
        let text = r#"{
            "site": "synthetic-bad",
            "contracts": [{
                "name": "primary",
                "applies_to": "site",
                "period": { "start": "2026-01-01", "end": "2026-12-31" },
                "charges": [{
                    "type": "demand",
                    "rate": {
                        "value": -1.0,
                        "unit": "usd_per_kw_month",
                        "source": "SYNTHETIC ESTIMATE",
                        "dated": "2026-01-01",
                        "verified": false
                    },
                    "window": 15
                }]
            }]
        }"#;
        let err = SiteTariff::from_json(text).expect_err("negative rate");
        assert!(matches!(err, TariffError::Json(_)), "got {err:?}");
        assert!(err.to_string().contains("negative"), "got {err}");
    }

    #[test]
    fn toml_constructor_still_refuses_a_bad_window() {
        let text = r#"
site = "synthetic-bad"
[[contracts]]
name = "primary"
applies_to = "site"
period = { start = "2026-01-01", end = "2026-12-31" }
[[contracts.charges]]
type = "demand"
window = 20
rate = { value = 10.0, unit = "usd_per_kw_month", source = "SYNTHETIC ESTIMATE", dated = "2026-01-01", verified = false }
"#;
        let err = SiteTariff::from_toml(text).expect_err("bad window");
        assert!(matches!(err, TariffError::Toml(_)), "got {err:?}");
        assert!(err.to_string().contains("15 or 30"), "got {err}");
    }

    fn shell(charge: &str) -> String {
        format!(
            "site = \"synthetic-shell\"\n\n[[contracts]]\nname = \"primary\"\napplies_to = \"site\"\nperiod = {{ start = \"2026-01-01\", end = \"2026-12-31\" }}\n\n{charge}"
        )
    }

    #[test]
    fn a_zero_adder_round_trips_as_the_unit_variant() {
        let text = shell(
            "[[contracts.charges]]\ntype = \"energy\"\nkind = \"dam_indexed_adder\"\nadder = 0.0\n",
        );
        let tariff = SiteTariff::from_toml(&text).expect("zero adder");
        assert!(matches!(
            tariff.contracts()[0].charges()[0],
            Charge::Energy(Settlement::DamIndexed)
        ));
        let encoded = SiteTariff::to_toml(&tariff).expect("encode");
        assert!(encoded.contains("dam_indexed"));
        assert!(!encoded.contains("dam_indexed_adder"));
        let again = SiteTariff::from_toml(&encoded).expect("unit variant");
        assert_eq!(tariff, again);
    }

    #[test]
    fn a_non_zero_adder_round_trips() {
        let text = shell(
            "[[contracts.charges]]\ntype = \"energy\"\nkind = \"real_time_adder\"\nadder = 5.0\n",
        );
        let tariff = SiteTariff::from_toml(&text).expect("adder");
        let encoded = SiteTariff::to_toml(&tariff).expect("encode");
        let again = SiteTariff::from_toml(&encoded).expect("decode");
        assert_eq!(tariff, again);
        assert!(encoded.contains("real_time_adder"));
    }

    #[test]
    fn facilities_contract_kw_and_ratcheted_peak_round_trip() {
        let contract_kw = shell(
            "[[contracts.charges]]\ntype = \"facilities\"\nrate_per_kw = 2.0\nbasis = \"contract_kw\"\ncontract_kw = 50.0\n",
        );
        let tariff = SiteTariff::from_toml(&contract_kw).expect("facilities");
        let again =
            SiteTariff::from_toml(&SiteTariff::to_toml(&tariff).expect("encode")).expect("decode");
        assert_eq!(tariff, again);
        let ratcheted = shell(
            "[[contracts.charges]]\ntype = \"facilities\"\nrate_per_kw = 2.0\nbasis = \"ratcheted_peak\"\n",
        );
        let tariff = SiteTariff::from_toml(&ratcheted).expect("ratcheted");
        let again =
            SiteTariff::from_toml(&SiteTariff::to_toml(&tariff).expect("encode")).expect("decode");
        assert_eq!(tariff, again);
    }

    #[test]
    fn facilities_basis_fields_are_refused_when_they_do_not_match() {
        let missing = shell(
            "[[contracts.charges]]\ntype = \"facilities\"\nrate_per_kw = 2.0\nbasis = \"contract_kw\"\n",
        );
        let err = SiteTariff::from_toml(&missing).expect_err("missing contract_kw");
        assert!(err.to_string().contains("contract_kw"), "got {err}");
        let extra = shell(
            "[[contracts.charges]]\ntype = \"facilities\"\nrate_per_kw = 2.0\nbasis = \"ratcheted_peak\"\ncontract_kw = 50.0\n",
        );
        let err = SiteTariff::from_toml(&extra).expect_err("unexpected contract_kw");
        assert!(err.to_string().contains("ratcheted_peak"), "got {err}");
    }
}
