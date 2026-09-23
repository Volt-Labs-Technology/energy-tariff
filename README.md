# energy-tariff

## What this is

Types and calculations for a site's electricity contracts. The tariff is data:
this crate computes each charge that is present. A **demand charge** bills the
month's peak kilowatts at a dollars-per-kilowatt-month rate. A **ratchet**
floors that billed kW at a percentage of recent months' peaks. A **facilities
charge** bills dollars per kilowatt on a contract quantity or on that ratcheted
peak. A **coincident-peak charge** bills the mean kilowatts across a named set
of intervals (4 or 12 per year) at an annual dollars-per-kilowatt rate.

## Who it is for

Anyone who needs to turn a site's electricity contracts into typed values and
dollar ledgers in Rust, without opening files or talking to a utility.

## How to use it

Add the crate to a Rust project:

```sh
cargo add energy-tariff
```

That writes this line into `Cargo.toml`:

```toml
energy-tariff = "0.2.0"
```

Parse `&str` only. File open is not this crate's job. One serde schema, two
encodings: TOML is the authoring form (comments allowed); JSON is the wire form.
Public parse is `SiteTariff::from_json` and `SiteTariff::from_toml`. After parse,
constructors still refuse illegal values. `to_json` / `to_toml` round-trip the
same schema.

```
SiteTariff { site: SiteAlias, contracts: Vec<Contract> }
Contract { name, applies_to: MeterScope, period: DateRange, charges: Vec<Charge> }
Charge =
  | Energy(Settlement)                       // DamIndexed | RealTime | Hedged { flat } | DamIndexedAdder { adder } | RealTimeAdder { adder } | Tou { periods }
  | Demand(DemandCharge)                     // { rate: RateWithSource, window: Minutes(15|30), ratchet: Option<Ratchet{ pct, months }>, hours: Option<TimeOfUse> }
  | Facilities(FacilitiesCharge)             // { rate_per_kw, basis: ContractKw { contract_kw } | RatchetedPeak }
  | CoincidentPeak(CoincidentPeakRule)       // { intervals_per_year: 4|12, interval_minutes, months, rate: RateWithSource, pass_through: PassThrough }
  | Fixed(FixedCharge)                       // reported, never optimised
TouPeriod { name, months, weekdays, start_min, end_min, rate }   // civil local time; this crate does not convert UTC
A zero adder is parsed and stored as DamIndexed or RealTime, not as an adder variant.
PassThrough = Confirmed | Assumed | NotPassedThrough
RateWithSource { value, unit, source, dated, verified: bool }
```

`site` is an alias: lowercase letters, digits, hyphens. It is not a company or
place name.

Calculations:

- `energy_bill` — kWh × dollars-per-megawatt-hour / 1000. DamIndexed uses the
  day-ahead series; RealTime uses the settlement series; an adder variant adds
  its dollars-per-megawatt-hour to that series; Hedged uses the flat rate every
  hour.
- `energy_bill_local` — the same, plus civil local time (month, ISO weekday,
  minute of day) for a time-of-use settlement. This crate does not convert UTC.
- `demand_bill` — this month's peak kW, or with a ratchet
  `max(this_month_peak, pct/100 × max peak of the previous N months)`.
- `demand_charge` — the same dollars as `demand_bill`, plus the peak, the
  interval that set it, whether the ratchet applied, and the ratchet floor.
- `marginal_demand_value` — dollars one interval would add to the demand charge
  if it became the new peak. Zero at or below the running peak, and zero when
  above the running peak but at or below the ratchet floor.
- `facilities_bill` — `rate_per_kw` times contract kW, or times the ratcheted
  demand kW.
- `month_bill` — energy, demand, facilities, and fixed. No `total()`.
  Coincident peak is not one of the four fields.
- `coincident_peak_exposure` — mean of the supplied interval kW values times the
  annual rate. Length must equal `intervals_per_year`. `NotPassedThrough` yields
  `$0` and a report line that says why.
- `ledgers` — one ledger per **present** charge, contract order. Nothing for
  absent charges. No `total()`.

```rust
use energy_tariff::{SiteTariff, TariffError};

fn load(toml: &str, json: &str) -> Result<SiteTariff, TariffError> {
    let from_toml = SiteTariff::from_toml(toml)?;
    let from_json = SiteTariff::from_json(json)?;
    assert_eq!(from_toml, from_json);
    Ok(from_toml)
}
```

Two complete examples ship under `testdata/`, each as both `.toml` and `.json`.
Both are SYNTHETIC. Rates are ESTIMATE and unverified.

1. `synthetic_4cp` — one contract, energy `DamIndexed`, plus a 4-interval
   coincident-peak charge with `pass_through: Assumed`, `verified: false`.
2. `synthetic_demand_ratchet` — one contract, energy `DamIndexed`, plus a
   monthly 15-minute demand charge with an 80% ratchet.

JSON and TOML for the same site are the same schema. `ledgers` on a fixture
load is identical for the pair.

## What it deliberately does not do

It does not predict peaks, read invoices, fetch prices, or know anything about
a specific utility. It does no filesystem I/O, no network I/O, and does not
read a clock.

Version 0.x: the API may change before 1.0.
