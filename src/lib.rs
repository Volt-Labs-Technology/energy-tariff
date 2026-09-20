//! Types and calculations for a site's electricity contracts.
//!
//! The tariff is data: a [`SiteTariff`] holds contracts, and each contract holds
//! the charges that apply. This crate computes each charge that is present. It
//! does not open files, fetch prices, read invoices, or predict peaks.
//!
//! Parse with [`SiteTariff::from_json`] and [`SiteTariff::from_toml`]. Both
//! encodings share one serde schema. File open is the caller's job.

mod contract;
mod demand;
mod ledgers;
mod parse;
mod peak;
mod settlement;

pub use contract::{
    CalendarDate, Charge, ChargeName, DateRange, FixedCharge, Kilowatt, KilowattHour, MeterScope,
    Minutes, PassThrough, RateUnit, RateWithSource, SiteAlias, SiteTariff, TimeOfUse, Usd,
    UsdPerMwh, YearMonth,
};
pub use demand::{DemandCharge, DemandHistory, Ratchet, demand_bill};
pub use ledgers::{LedgerInputs, ledgers};
pub use peak::{CoincidentPeakRule, PeakExposure, coincident_peak_exposure};
pub use settlement::{EnergyPrices, HedgedRate, Settlement, energy_bill};

/// Why a constructor, parse, or calculation refused its input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TariffError {
    /// No text at all, which names no site.
    #[error("a site alias cannot be empty; it looks like synthetic-4cp")]
    EmptySiteAlias,
    /// A character an alias cannot hold.
    #[error("`{alias}` is not a site alias: `{character}` is not one of a-z, 0-9 or -")]
    SiteAliasCharacter { alias: String, character: char },
    /// A contract with no name cannot be told apart from another.
    #[error("a contract name cannot be empty")]
    EmptyContractName,
    /// A meter scope with no text names no meter.
    #[error("a meter scope cannot be empty")]
    EmptyMeterScope,
    /// Rate value is negative or not finite.
    #[error("rate is negative or not finite: {value}")]
    IllegalRate { value: String },
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
    /// Supplied coincident-peak kW count does not match the rule.
    #[error("coincident-peak intervals: expected {expected}, got {got}")]
    IntervalCountMismatch { expected: usize, got: usize },
    /// Energy load hours and the price series are different lengths.
    #[error("load length {load} does not match price length {prices}")]
    SeriesLengthMismatch { load: usize, prices: usize },
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
