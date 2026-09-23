//! Facilities charge: dollars per kilowatt on a contract quantity or the ratcheted peak.

use crate::contract::{Kilowatt, Usd};
use crate::{TariffError, require_non_negative_finite};

/// What the facilities rate multiplies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FacilitiesBasis {
    /// A contracted kilowatt quantity stored on the charge.
    ContractKw {
        /// Contracted kilowatts. Not negative.
        contract_kw: Kilowatt,
    },
    /// The billed demand kilowatts for the same month, ratchet included.
    RatchetedPeak,
}

/// Dollars per kilowatt, applied to [`FacilitiesBasis`].
#[derive(Debug, Clone, PartialEq)]
pub struct FacilitiesCharge {
    rate_per_kw: f64,
    basis: FacilitiesBasis,
}

impl FacilitiesCharge {
    /// `FacilitiesCharge::new(rate_per_kw, basis)` is that charge. **Calculation.**
    ///
    /// # Errors
    /// [`TariffError::IllegalRate`] when the rate or a contract kW is negative
    /// or not finite.
    pub fn new(rate_per_kw: f64, basis: FacilitiesBasis) -> Result<Self, TariffError> {
        let rate_per_kw = require_non_negative_finite(rate_per_kw)?;
        if let FacilitiesBasis::ContractKw { contract_kw } = basis {
            require_non_negative_finite(contract_kw.get())?;
        }
        Ok(Self { rate_per_kw, basis })
    }

    /// Dollars per kilowatt. **Calculation.**
    #[must_use]
    pub const fn rate_per_kw(&self) -> f64 {
        self.rate_per_kw
    }

    /// Kilowatt basis. **Calculation.**
    #[must_use]
    pub const fn basis(&self) -> FacilitiesBasis {
        self.basis
    }
}

/// `facilities_bill(charge, ratcheted_kw)` is `rate_per_kw` times the basis kilowatts.
///
/// `ContractKw` uses the quantity on the charge and ignores `ratcheted_kw`.
/// `RatchetedPeak` uses `ratcheted_kw`, which is the billed demand kilowatts
/// (month peak, or the ratchet floor when that is higher). **Calculation.**
#[must_use]
pub fn facilities_bill(charge: &FacilitiesCharge, ratcheted_kw: Kilowatt) -> Usd {
    let kilowatts = match charge.basis() {
        FacilitiesBasis::ContractKw { contract_kw } => contract_kw.get(),
        FacilitiesBasis::RatchetedPeak => ratcheted_kw.get(),
    };
    Usd::new(kilowatts * charge.rate_per_kw())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_kw_basis_multiplies_the_stored_quantity() {
        let basis = FacilitiesBasis::ContractKw {
            contract_kw: Kilowatt::new(50.0),
        };
        let charge = FacilitiesCharge::new(2.0, basis).expect("valid");
        assert_eq!(FacilitiesCharge::rate_per_kw(&charge).to_string(), "2");
        assert_eq!(FacilitiesCharge::basis(&charge), basis);
        // 50 kW × $2/kW = $100. The ratcheted argument is ignored.
        assert_eq!(
            facilities_bill(&charge, Kilowatt::new(999.0)),
            Usd::new(100.0)
        );
    }

    #[test]
    fn ratcheted_peak_basis_multiplies_the_supplied_kilowatts() {
        let charge = FacilitiesCharge::new(2.0, FacilitiesBasis::RatchetedPeak).expect("valid");
        // 180 kW × $2/kW = $360.
        assert_eq!(
            facilities_bill(&charge, Kilowatt::new(180.0)),
            Usd::new(360.0)
        );
    }

    #[test]
    fn negative_rate_and_negative_contract_kw_are_refused() {
        assert!(matches!(
            FacilitiesCharge::new(-1.0, FacilitiesBasis::RatchetedPeak),
            Err(TariffError::IllegalRate { .. })
        ));
        let basis = FacilitiesBasis::ContractKw {
            contract_kw: Kilowatt::new(-5.0),
        };
        assert!(matches!(
            FacilitiesCharge::new(2.0, basis),
            Err(TariffError::IllegalRate { .. })
        ));
    }
}
