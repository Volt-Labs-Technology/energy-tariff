//! Types and calculations for a site's electricity contracts.
//!
//! The tariff is data: a [`SiteTariff`] holds contracts, and each contract holds
//! the charges that apply. This crate computes each charge that is present. It
//! does not open files, fetch prices, read invoices, or predict peaks.
//!
//! Parse with [`SiteTariff::from_json`] and [`SiteTariff::from_toml`]. Both
//! encodings share one serde schema. File open is the caller's job.
//!
//! ```
//! use energy_tariff::{SiteTariff, TariffError};
//!
//! fn load(toml: &str, json: &str) -> Result<SiteTariff, TariffError> {
//!     let from_toml = SiteTariff::from_toml(toml)?;
//!     let from_json = SiteTariff::from_json(json)?;
//!     assert_eq!(from_toml, from_json);
//!     Ok(from_toml)
//! }
//!
//! # // SYNTHETIC. Not a real site.
//! # let toml = concat!(
//! #     "site = \"synthetic-readme\"\n\n",
//! #     "[[contracts]]\n",
//! #     "name = \"primary\"\n",
//! #     "applies_to = \"site\"\n",
//! #     "period = { start = \"2026-01-01\", end = \"2026-12-31\" }\n\n",
//! #     "[[contracts.charges]]\n",
//! #     "type = \"energy\"\n",
//! #     "kind = \"dam_indexed\"\n",
//! # );
//! # let json = r#"{"site":"synthetic-readme","contracts":[{"name":"primary","applies_to":"site","period":{"start":"2026-01-01","end":"2026-12-31"},"charges":[{"type":"energy","kind":"dam_indexed"}]}]}"#;
//! # load(toml, json).expect("SYNTHETIC");
//! ```

#![deny(missing_docs)]

mod bill;
mod contract;
mod demand;
mod facilities;
mod ledgers;
mod parse;
mod peak;
mod settlement;
mod tou;

pub use bill::{MonthBill, MonthInputs, month_bill};
pub use contract::{
    CalendarDate, Charge, ChargeName, DateRange, FixedCharge, Kilowatt, KilowattHour, MeterScope,
    Minutes, PassThrough, RateUnit, RateWithSource, SiteAlias, SiteTariff, TimeOfUse, Usd,
    UsdPerMwh, YearMonth,
};
pub use demand::{
    DemandCharge, DemandHistory, DemandResult, Ratchet, demand_bill, demand_charge,
    marginal_demand_value,
};
pub use facilities::{FacilitiesBasis, FacilitiesCharge, facilities_bill};
pub use ledgers::{LedgerInputs, ledgers};
pub use peak::{CoincidentPeakRule, PeakExposure, coincident_peak_exposure};
pub use settlement::{
    EnergyAdder, EnergyPrices, HedgedRate, Settlement, energy_bill, energy_bill_local,
};
pub use tou::{CivilMinute, TouPeriod, TouSchedule};

/// Why a constructor, parse, or calculation refused its input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TariffError {
    /// No text at all, which names no site.
    #[error("a site alias cannot be empty; it looks like synthetic-4cp")]
    EmptySiteAlias,
    /// A character an alias cannot hold.
    #[error("`{alias}` is not a site alias: `{character}` is not one of a-z, 0-9 or -")]
    SiteAliasCharacter {
        /// The rejected alias text.
        alias: String,
        /// The first character that is not `a-z`, `0-9`, or `-`.
        character: char,
    },
    /// A contract with no name cannot be told apart from another.
    #[error("a contract name cannot be empty")]
    EmptyContractName,
    /// A meter scope with no text names no meter.
    #[error("a meter scope cannot be empty")]
    EmptyMeterScope,
    /// Rate value is negative or not finite.
    #[error("rate is negative or not finite: {value}")]
    IllegalRate {
        /// The rejected number, as text so NaN and infinities still print.
        value: String,
    },
    /// Demand interval length is not 15 or 30 minutes.
    #[error("demand window must be 15 or 30 minutes, got {0}")]
    InvalidWindow(u16),
    /// Ratchet percent is not in `(0, 100]`.
    #[error("ratchet pct must be in (0, 100], got {0}")]
    InvalidRatchetPct(String),
    /// Ratchet lookback of zero months is not a lookback.
    #[error("ratchet months must be at least 1, got {0}")]
    InvalidRatchetMonths(u32),
    /// Coincident-peak count is not 4 or 12.
    #[error("intervals_per_year must be 4 or 12, got {0}")]
    InvalidIntervalsPerYear(u8),
    /// Interval length of zero minutes is not an interval.
    #[error("interval minutes must be positive, got {0}")]
    InvalidIntervalMinutes(u16),
    /// Calendar month outside 1..=12.
    #[error("month {0} is not in 1..=12")]
    InvalidMonth(u8),
    /// Text is not a calendar day in `YYYY-MM-DD`.
    #[error("date `{0}` is not YYYY-MM-DD")]
    InvalidDate(String),
    /// Range starts after it ends.
    #[error("date range starts after it ends")]
    InvertedDateRange,
    /// Hour-of-day outside 0..=23.
    #[error("time-of-use hour {0} is not in 0..=23")]
    InvalidHour(u8),
    /// Minute of day outside the legal window.
    #[error("minute {0} is outside 0..1440 (end may be 1440)")]
    InvalidMinute(u16),
    /// ISO weekday outside 1..=7.
    #[error("ISO weekday {0} is not in 1..=7")]
    InvalidWeekday(u8),
    /// Time-of-use period with no name.
    #[error("a time-of-use period name cannot be empty")]
    EmptyTouName,
    /// Time-of-use period that names no month.
    #[error("a time-of-use period has no months")]
    EmptyTouMonths,
    /// Time-of-use period that names no weekday.
    #[error("a time-of-use period has no weekdays")]
    EmptyTouWeekdays,
    /// The same month listed twice in one period.
    #[error("duplicate month {0} in a time-of-use period")]
    DuplicateTouMonth(u8),
    /// The same weekday listed twice in one period.
    #[error("duplicate weekday {0} in a time-of-use period")]
    DuplicateTouWeekday(u8),
    /// `start_min >= end_min`, so the window contains no minute.
    #[error("time-of-use window is empty")]
    EmptyTouWindow,
    /// Schedule with no periods.
    #[error("time-of-use schedule has no periods")]
    EmptyTouSchedule,
    /// Two periods cover the same local minute.
    #[error("time-of-use periods `{first}` and `{second}` overlap")]
    OverlappingTouPeriods {
        /// Name of the earlier period.
        first: String,
        /// Name of the later period.
        second: String,
    },
    /// Civil time fell in no period.
    #[error("civil time matches no time-of-use period")]
    UnmatchedCivilTime,
    /// Load hour fell in no period.
    #[error("time-of-use interval {index} matches no period")]
    UnmatchedTouInterval {
        /// Index into the load slice.
        index: usize,
    },
    /// Load hours and civil times have different lengths.
    #[error("load length {load} does not match civil-time length {local}")]
    CivilTimeLengthMismatch {
        /// Length of the load series.
        load: usize,
        /// Length of the civil-time series.
        local: usize,
    },
    /// Time-of-use settlement billed without civil time.
    #[error("time-of-use energy needs civil local time")]
    MissingCivilTime,
    /// Adder of zero is the unit settlement, not an adder variant.
    #[error("an energy adder of zero is the unit settlement, not an adder")]
    ZeroAdder,
    /// `contract_kw` basis without a quantity.
    #[error("facilities basis contract_kw needs contract_kw")]
    MissingContractKw,
    /// `ratcheted_peak` basis that also carries a contract quantity.
    #[error("facilities basis ratcheted_peak does not take contract_kw")]
    UnexpectedContractKw,
    /// Supplied coincident-peak kW count does not match the rule.
    #[error("coincident-peak intervals: expected {expected}, got {got}")]
    IntervalCountMismatch {
        /// Count the rule requires.
        expected: usize,
        /// Count the caller supplied.
        got: usize,
    },
    /// Energy load hours and the price series are different lengths.
    #[error("load length {load} does not match price length {prices}")]
    SeriesLengthMismatch {
        /// Length of the load series.
        load: usize,
        /// Length of the price series.
        prices: usize,
    },
    /// JSON text did not match the schema, or a constructor refused a value.
    #[error("JSON: {0}")]
    Json(String),
    /// TOML text did not match the schema, or a constructor refused a value.
    #[error("TOML: {0}")]
    Toml(String),
}

impl From<serde_json::Error> for TariffError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<toml::de::Error> for TariffError {
    fn from(error: toml::de::Error) -> Self {
        Self::Toml(error.to_string())
    }
}

impl From<toml::ser::Error> for TariffError {
    fn from(error: toml::ser::Error) -> Self {
        Self::Toml(error.to_string())
    }
}

pub(crate) fn illegal_rate(value: f64) -> TariffError {
    TariffError::IllegalRate {
        value: value.to_string(),
    }
}

pub(crate) fn require_non_negative_finite(value: f64) -> Result<f64, TariffError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(illegal_rate(value))
    }
}
