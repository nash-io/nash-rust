pub mod btc;
pub mod eth;
pub mod neo;

use crate::errors::{ProtocolError, Result};
use crate::graphql::place_limit_order;

use super::super::signer::Signer;

use bigdecimal::{BigDecimal, ToPrimitive};

/// Generic representation of FillOrder payloads across blockchains. These enable
/// Nash to settle active orders directly with the smart contract if necessary
#[derive(Clone, Debug, PartialEq)]
pub enum FillOrder {
    Ethereum(eth::FillOrder),
    Bitcoin(btc::FillOrder),
    NEO(neo::FillOrder),
}

impl FillOrder {
    pub fn to_hex(&self) -> Result<String> {
        match self {
            Self::Ethereum(fill_order) => fill_order.to_hex(),
            Self::Bitcoin(_) => Ok("".to_string()),
            Self::NEO(fill_order) => fill_order.to_hex(),
        }
    }

    pub fn to_blockchain_signature(
        &self,
        signer: &mut Signer,
    ) -> Result<place_limit_order::BlockchainSignature> {
        match self {
            Self::Ethereum(fill_order) => fill_order.to_blockchain_signature(signer),
            Self::Bitcoin(fill_order) => fill_order.to_blockchain_signature(signer),
            Self::NEO(fill_order) => fill_order.to_blockchain_signature(signer),
        }
    }
}

/// Convert a bigdecimal `num` to `u64` for serialization in the protocol using the
/// precision scheme defined by the Nash ME
pub fn bigdecimal_to_nash_u64(num: &BigDecimal, precision: u32) -> Result<u64> {
    let num = bigdecimal_to_nash_prec(num, precision);
    let multiplier = BigDecimal::from(u64::pow(10, precision));
    (num * multiplier)
        .with_scale(0)
        .to_u64()
        .ok_or(ProtocolError("Result does not fit into u64."))
}

pub fn nash_u64_to_bigdecimal(num: u64, precision: u32) -> BigDecimal {
    let num = BigDecimal::from(num);
    let divider = BigDecimal::from(u64::pow(10, precision));
    num / divider
}

/// Convert a bigdecimal to precision expected by the Nash ME
/// Nash ME defines precision as only digits right of decimal point
pub fn bigdecimal_to_nash_prec(num: &BigDecimal, precision: u32) -> BigDecimal {
    let scale = BigDecimal::from(u64::pow(10, precision));
    let scaled = (num * &scale).with_scale(0);
    &scaled / &scale
}

#[cfg(test)]
mod tests {
    use super::{bigdecimal_to_nash_prec, bigdecimal_to_nash_u64};
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    #[test]
    fn nash_precision() {
        let num = BigDecimal::from_str("1.55555").unwrap();
        let prec_num = bigdecimal_to_nash_prec(&num, 4);
        assert_eq!(prec_num.to_string(), "1.5555");
    }

    #[test]
    fn bd_test() {
        let bd_1 = BigDecimal::from_str("0.03451").unwrap();
        let bd_2 = BigDecimal::from_str("0.03454").unwrap();
        println!("Good: {}", bd_1.inverse());
        println!("Bad: {}", bd_2.inverse());
        let step1_1 = bigdecimal_to_nash_prec(&bd_1.inverse(), 8);
        let step1_2 = bigdecimal_to_nash_prec(&bd_2.inverse(), 8);
        println!("Good: {}", step1_1);
        println!("Bad: {}", step1_2);
        let converted_1 = bigdecimal_to_nash_u64(&bd_1.inverse(), 8).unwrap();
        let converted_2 = bigdecimal_to_nash_u64(&bd_2.inverse(), 8).unwrap();
        println!("Good: {}", converted_1);
        println!("Bad: {}", converted_2);
    }
}
