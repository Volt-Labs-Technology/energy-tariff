//! Contract data: a site, its contracts, and the charges each contract lists.

use crate::demand::DemandCharge;
use crate::peak::CoincidentPeakRule;
use crate::settlement::Settlement;
use crate::{TariffError, require_non_negative_finite};
use std::fmt;

/// US dollars. Every ledger amount is this.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Usd(f64);

impl Usd {
    /// `Usd::new(amount)` is that dollar amount. **Calculation.**
    #[must_use]
    pub const fn new(amount: f64) -> Self {
        Self(amount)
    }

    /// The dollar amount as `f64`. **Calculation.**
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// Kilowatt. Interval demand samples are this.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Kilowatt(f64);

impl Kilowatt {
    /// `Kilowatt::new(kw)` is that demand sample. **Calculation.**
    #[must_use]
    pub const fn new(kw: f64) -> Self {
        Self(kw)
    }

    /// The kW value as `f64`. **Calculation.**
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// Kilowatt-hour. Hourly energy load is this.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct KilowattHour(f64);

impl KilowattHour {
    /// `KilowattHour::new(kwh)` is that energy. **Calculation.**
    #[must_use]
    pub const fn new(kwh: f64) -> Self {
        Self(kwh)
    }

    /// The kWh value as `f64`. **Calculation.**
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// Dollars per megawatt-hour. Energy prices and a hedged flat rate are this.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct UsdPerMwh(f64);

impl UsdPerMwh {
    /// `UsdPerMwh::new(price)` is that price. **Calculation.**
    #[must_use]
    pub const fn new(price: f64) -> Self {
        Self(price)
    }

    /// The dollars-per-megawatt-hour value as `f64`. **Calculation.**
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A site's alias: lowercase letters, digits and hyphens, at least one character.
///
/// The rule refuses the shapes a company or place name arrives in — capitals,
/// spaces, punctuation — so a tariff file cannot carry a real name by accident.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SiteAlias(String);

impl SiteAlias {
    /// `SiteAlias::parse(text)` is that alias, or why the text is not one.
    /// **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::EmptySiteAlias`] or [`TariffError::SiteAliasCharacter`].
    pub fn parse(text: &str) -> Result<Self, TariffError> {
        if text.is_empty() {
            return Err(TariffError::EmptySiteAlias);
        }
        match text.chars().find(|character| !is_alias_char(*character)) {
            Some(character) => Err(TariffError::SiteAliasCharacter {
                alias: text.to_owned(),
                character,
            }),
            None => Ok(Self(text.to_owned())),
        }
    }

    /// The alias as text. **Calculation.**
    #[must_use]
    pub fn get(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SiteAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_alias_char(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
}

/// Which meters a contract applies to. Any non-empty label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterScope(String);

impl MeterScope {
    /// `MeterScope::new(text)` is that scope, or empty-text refusal. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::EmptyMeterScope`].
    pub fn new(text: impl Into<String>) -> Result<Self, TariffError> {
        let text = text.into();
        if text.is_empty() {
            Err(TariffError::EmptyMeterScope)
        } else {
            Ok(Self(text))
        }
    }

    /// The scope as text. **Calculation.**
    #[must_use]
    pub fn get(&self) -> &str {
        &self.0
    }
}

/// A calendar day in the proleptic Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalendarDate {
    year: i32,
    month: u8,
    day: u8,
}

impl CalendarDate {
    /// `CalendarDate::new(year, month, day)` is that day, if it exists.
    /// **Calculation.** Does not read a clock.
    ///
    /// # Errors
    /// [`TariffError::InvalidMonth`] or [`TariffError::InvalidDate`].
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, TariffError> {
        let max = days_in_month(year, month)?;
        if day == 0 || day > max {
            return Err(TariffError::InvalidDate(format!(
                "{year:04}-{month:02}-{day:02}"
            )));
        }
        Ok(Self { year, month, day })
    }

    /// `CalendarDate::parse("YYYY-MM-DD")` is that day. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::InvalidDate`] or [`TariffError::InvalidMonth`].
    pub fn parse(text: &str) -> Result<Self, TariffError> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(TariffError::InvalidDate(text.to_owned()));
        }
        let year: i32 = text[..4]
            .parse()
            .map_err(|_| TariffError::InvalidDate(text.to_owned()))?;
        let month: u8 = text[5..7]
            .parse()
            .map_err(|_| TariffError::InvalidDate(text.to_owned()))?;
        let day: u8 = text[8..10]
            .parse()
            .map_err(|_| TariffError::InvalidDate(text.to_owned()))?;
        Self::new(year, month, day)
    }

    /// ISO-8601 calendar date. **Calculation.**
    #[must_use]
    pub fn to_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in_month(year: i32, month: u8) -> Result<u8, TariffError> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Ok(31),
        4 | 6 | 9 | 11 => Ok(30),
        2 => Ok(if is_leap(year) { 29 } else { 28 }),
        _ => Err(TariffError::InvalidMonth(month)),
    }
}

fn is_leap(year: i32) -> bool {
    let y = year.unsigned_abs();
    y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400))
}

/// Inclusive start and end calendar days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    start: CalendarDate,
    end: CalendarDate,
}

impl DateRange {
    /// `DateRange::new(start, end)` is that span. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::InvertedDateRange`] when `start` is after `end`.
    pub fn new(start: CalendarDate, end: CalendarDate) -> Result<Self, TariffError> {
        if start > end {
            Err(TariffError::InvertedDateRange)
        } else {
            Ok(Self { start, end })
        }
    }

    /// First day, inclusive. **Calculation.**
    #[must_use]
    pub const fn start(self) -> CalendarDate {
        self.start
    }

    /// Last day, inclusive. **Calculation.**
    #[must_use]
    pub const fn end(self) -> CalendarDate {
        self.end
    }
}

/// Year and month for demand billing. Not a wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YearMonth {
    year: i32,
    month: u8,
}

impl YearMonth {
    /// `YearMonth::new(year, month)` is that month. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::InvalidMonth`].
    pub fn new(year: i32, month: u8) -> Result<Self, TariffError> {
        if (1..=12).contains(&month) {
            Ok(Self { year, month })
        } else {
            Err(TariffError::InvalidMonth(month))
        }
    }

    /// Calendar year. **Calculation.**
    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    /// Month number 1..=12. **Calculation.**
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// The previous calendar month. **Calculation.**
    #[must_use]
    pub fn pred(self) -> Self {
        if self.month == 1 {
            Self {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Self {
                year: self.year,
                month: self.month - 1,
            }
        }
    }
}

/// Demand-interval length in minutes. Only 15 or 30.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Minutes(u16);

impl Minutes {
    /// `Minutes::demand_window(n)` is a 15- or 30-minute window. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::InvalidWindow`].
    pub fn demand_window(minutes: u16) -> Result<Self, TariffError> {
        match minutes {
            15 | 30 => Ok(Self(minutes)),
            other => Err(TariffError::InvalidWindow(other)),
        }
    }

    /// `Minutes::positive(n)` is a positive interval length. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::InvalidIntervalMinutes`].
    pub fn positive(minutes: u16) -> Result<Self, TariffError> {
        if minutes == 0 {
            Err(TariffError::InvalidIntervalMinutes(minutes))
        } else {
            Ok(Self(minutes))
        }
    }

    /// The minute count. **Calculation.**
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Unit a [`RateWithSource`] is denominated in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::enum_variant_names,
    reason = "each variant is a dollars-per-unit; the prefix is the unit"
)]
pub enum RateUnit {
    /// Dollars per kilowatt-month. Demand charges.
    UsdPerKwMonth,
    /// Dollars per kilowatt-year. Coincident-peak charges.
    UsdPerKwYear,
    /// Dollars per megawatt-hour. Energy.
    UsdPerMwh,
}

/// A rate plus where it came from and whether anyone has verified it.
///
/// Estimates are labelled in `source`. `verified` is false until a human says
/// otherwise.
#[derive(Debug, Clone, PartialEq)]
pub struct RateWithSource {
    value: f64,
    unit: RateUnit,
    source: String,
    dated: CalendarDate,
    verified: bool,
}

impl RateWithSource {
    /// `RateWithSource::new(...)` is that labelled rate. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::IllegalRate`] when `value` is negative or not finite.
    pub fn new(
        value: f64,
        unit: RateUnit,
        source: impl Into<String>,
        dated: CalendarDate,
        verified: bool,
    ) -> Result<Self, TariffError> {
        Ok(Self {
            value: require_non_negative_finite(value)?,
            unit,
            source: source.into(),
            dated,
            verified,
        })
    }

    /// Numeric rate. **Calculation.**
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.value
    }

    /// Unit of [`Self::value`]. **Calculation.**
    #[must_use]
    pub const fn unit(&self) -> RateUnit {
        self.unit
    }

    /// Provenance text. ESTIMATE belongs here when the figure is an estimate.
    /// **Calculation.**
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Date the source quotes. **Calculation.**
    #[must_use]
    pub const fn dated(&self) -> CalendarDate {
        self.dated
    }

    /// Whether a human has verified the figure. **Calculation.**
    #[must_use]
    pub const fn verified(&self) -> bool {
        self.verified
    }
}

/// Whether a coincident-peak charge is billed through to the site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassThrough {
    /// The contract says the charge is passed through.
    Confirmed,
    /// The file assumes it is passed through; unverified.
    Assumed,
    /// The charge is not passed through. Exposure is `$0`.
    NotPassedThrough,
}

/// Optional time-of-use window on a demand charge, hours 0..=23 local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfUse {
    start_hour: u8,
    end_hour: u8,
}

impl TimeOfUse {
    /// `TimeOfUse::new(start, end)` is that hour window. **Calculation.**
    ///
    /// Hours are 0..=23. `end_hour` is exclusive unless it equals `start_hour`,
    /// which means the whole day. The caller applies the window to samples
    /// before [`crate::demand_bill`]; this type only stores it.
    ///
    /// # Errors
    /// [`TariffError::InvalidHour`].
    pub fn new(start_hour: u8, end_hour: u8) -> Result<Self, TariffError> {
        require_hour(start_hour)?;
        require_hour(end_hour)?;
        Ok(Self {
            start_hour,
            end_hour,
        })
    }

    /// Inclusive start hour. **Calculation.**
    #[must_use]
    pub const fn start_hour(self) -> u8 {
        self.start_hour
    }

    /// End hour as stored. **Calculation.**
    #[must_use]
    pub const fn end_hour(self) -> u8 {
        self.end_hour
    }
}

fn require_hour(hour: u8) -> Result<u8, TariffError> {
    if hour <= 23 {
        Ok(hour)
    } else {
        Err(TariffError::InvalidHour(hour))
    }
}

/// A reported fixed charge. Present in ledgers; never an optimisation lever.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedCharge {
    amount: Usd,
}

impl FixedCharge {
    /// `FixedCharge::new(amount)` is that dollar amount. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::IllegalRate`] when `amount` is negative or not finite.
    pub fn new(amount: f64) -> Result<Self, TariffError> {
        Ok(Self {
            amount: Usd::new(require_non_negative_finite(amount)?),
        })
    }

    /// Dollar amount. **Calculation.**
    #[must_use]
    pub const fn amount(&self) -> Usd {
        self.amount
    }
}

/// One present charge on a contract.
#[derive(Debug, Clone, PartialEq)]
pub enum Charge {
    /// Energy settlement: DAM-indexed, real-time, or hedged flat.
    Energy(Settlement),
    /// Monthly demand charge, optional ratchet and time-of-use.
    Demand(DemandCharge),
    /// Coincident-peak rule, 4 or 12 intervals per year.
    CoincidentPeak(CoincidentPeakRule),
    /// Fixed amount, reported only.
    Fixed(FixedCharge),
}

/// Name of one ledger line: `{contract}:{kind}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChargeName(String);

impl ChargeName {
    pub(crate) fn new(text: String) -> Self {
        Self(text)
    }

    /// The name as text. **Calculation.**
    #[must_use]
    pub fn get(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChargeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One named contract covering a meter scope for a date range.
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    name: String,
    applies_to: MeterScope,
    period: DateRange,
    charges: Vec<Charge>,
}

impl Contract {
    /// `Contract::new(name, applies_to, period, charges)` is that contract.
    /// **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::EmptyContractName`].
    pub fn new(
        name: impl Into<String>,
        applies_to: MeterScope,
        period: DateRange,
        charges: Vec<Charge>,
    ) -> Result<Self, TariffError> {
        let name = name.into();
        if name.is_empty() {
            return Err(TariffError::EmptyContractName);
        }
        Ok(Self {
            name,
            applies_to,
            period,
            charges,
        })
    }

    /// Contract name. **Calculation.**
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Meter scope. **Calculation.**
    #[must_use]
    pub const fn applies_to(&self) -> &MeterScope {
        &self.applies_to
    }

    /// Inclusive period. **Calculation.**
    #[must_use]
    pub const fn period(&self) -> DateRange {
        self.period
    }

    /// Charges in contract order. **Calculation.**
    #[must_use]
    pub fn charges(&self) -> &[Charge] {
        &self.charges
    }
}

/// A site alias and the contracts that apply there.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteTariff {
    site: SiteAlias,
    contracts: Vec<Contract>,
}

impl SiteTariff {
    /// `SiteTariff::new(site, contracts)` is that tariff. **Calculation.**
    #[must_use]
    pub fn new(site: SiteAlias, contracts: Vec<Contract>) -> Self {
        Self { site, contracts }
    }

    /// Site alias. **Calculation.**
    #[must_use]
    pub const fn site(&self) -> &SiteAlias {
        &self.site
    }

    /// Contracts in file order. **Calculation.**
    #[must_use]
    pub fn contracts(&self) -> &[Contract] {
        &self.contracts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_alias_accepts_hyphenated_lowercase() {
        let alias = SiteAlias::parse("synthetic-4cp").expect("fixture alias");
        assert_eq!(alias.get(), "synthetic-4cp");
    }

    #[test]
    fn site_alias_refuses_empty_and_capitals() {
        assert_eq!(SiteAlias::parse(""), Err(TariffError::EmptySiteAlias));
        assert!(matches!(
            SiteAlias::parse("Acme"),
            Err(TariffError::SiteAliasCharacter { character: 'A', .. })
        ));
    }

    #[test]
    fn date_range_refuses_inverted_bounds() {
        let start = CalendarDate::new(2026, 6, 1).expect("valid");
        let end = CalendarDate::new(2026, 1, 1).expect("valid");
        assert_eq!(
            DateRange::new(start, end),
            Err(TariffError::InvertedDateRange)
        );
    }

    #[test]
    fn calendar_date_refuses_31_april() {
        assert!(CalendarDate::new(2026, 4, 31).is_err());
    }

    #[test]
    fn year_month_pred_crosses_january() {
        let jan = YearMonth::new(2026, 1).expect("valid");
        let dec = jan.pred();
        assert_eq!(dec.year(), 2025);
        assert_eq!(dec.month(), 12);
    }

    #[test]
    fn negative_rate_is_refused() {
        let dated = CalendarDate::new(2026, 1, 1).expect("valid");
        assert!(matches!(
            RateWithSource::new(-1.0, RateUnit::UsdPerKwMonth, "x", dated, false),
            Err(TariffError::IllegalRate { .. })
        ));
    }

    #[test]
    fn time_of_use_refuses_hour_24() {
        assert_eq!(TimeOfUse::new(0, 24), Err(TariffError::InvalidHour(24)));
        let window = TimeOfUse::new(7, 19).expect("valid");
        assert_eq!(window.start_hour(), 7);
        assert_eq!(window.end_hour(), 19);
    }

    #[test]
    fn calendar_date_parses_iso_and_refuses_garbage() {
        let date = CalendarDate::parse("2026-01-01").expect("iso");
        assert_eq!(date.to_iso(), "2026-01-01");
        assert!(CalendarDate::parse("2026/01/01").is_err());
    }

    #[test]
    fn empty_contract_name_is_refused() {
        let period = DateRange::new(
            CalendarDate::new(2026, 1, 1).expect("valid"),
            CalendarDate::new(2026, 12, 31).expect("valid"),
        )
        .expect("valid");
        let scope = MeterScope::new("site").expect("valid");
        assert_eq!(
            Contract::new("", scope, period, vec![]).unwrap_err(),
            TariffError::EmptyContractName
        );
    }

    #[test]
    fn empty_meter_scope_is_refused() {
        assert_eq!(MeterScope::new(""), Err(TariffError::EmptyMeterScope));
    }

    #[test]
    fn year_month_refuses_month_zero_and_thirteen() {
        assert_eq!(YearMonth::new(2026, 0), Err(TariffError::InvalidMonth(0)));
        assert_eq!(YearMonth::new(2026, 13), Err(TariffError::InvalidMonth(13)));
    }

    #[test]
    fn minutes_positive_refuses_zero() {
        assert_eq!(
            Minutes::positive(0),
            Err(TariffError::InvalidIntervalMinutes(0))
        );
    }

    #[test]
    fn fixed_charge_refuses_a_negative_amount() {
        assert!(matches!(
            FixedCharge::new(-1.0),
            Err(TariffError::IllegalRate { .. })
        ));
    }

    #[test]
    fn rate_with_source_refuses_nan_and_infinity() {
        let dated = CalendarDate::new(2026, 1, 1).expect("valid");
        assert!(matches!(
            RateWithSource::new(f64::NAN, RateUnit::UsdPerKwMonth, "x", dated, false),
            Err(TariffError::IllegalRate { .. })
        ));
        assert!(matches!(
            RateWithSource::new(f64::INFINITY, RateUnit::UsdPerKwMonth, "x", dated, false),
            Err(TariffError::IllegalRate { .. })
        ));
    }

    #[test]
    fn rate_with_source_preserves_labels() {
        let dated = CalendarDate::new(2026, 1, 1).expect("valid");
        let rate = RateWithSource::new(
            1.0,
            RateUnit::UsdPerKwMonth,
            "SYNTHETIC ESTIMATE",
            dated,
            false,
        )
        .expect("valid");
        assert_eq!(rate.value().to_string(), "1");
        assert_eq!(rate.unit(), RateUnit::UsdPerKwMonth);
        assert_eq!(rate.source(), "SYNTHETIC ESTIMATE");
        assert_eq!(rate.dated(), dated);
        assert!(!rate.verified());
    }

    #[test]
    fn site_tariff_holds_the_site_and_contracts() {
        let start = CalendarDate::new(2026, 1, 1).expect("valid");
        let end = CalendarDate::new(2026, 12, 31).expect("valid");
        let period = DateRange::new(start, end).expect("valid");
        let contract = Contract::new(
            "primary",
            MeterScope::new("site").expect("valid"),
            period,
            vec![],
        )
        .expect("valid");
        assert_eq!(contract.name(), "primary");
        assert_eq!(contract.applies_to().get(), "site");
        assert_eq!(contract.period().start(), start);
        assert_eq!(contract.period().end(), end);
        assert!(contract.charges().is_empty());
        let tariff = SiteTariff::new(
            SiteAlias::parse("synthetic-hold").expect("alias"),
            vec![contract],
        );
        assert_eq!(tariff.site().get(), "synthetic-hold");
        assert_eq!(tariff.contracts().len(), 1);
    }
}
