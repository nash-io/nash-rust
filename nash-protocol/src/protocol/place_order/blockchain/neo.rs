use crate::errors::Result;
use crate::graphql::place_limit_order;
use crate::types::neo::{Address, PublicKey};
use crate::types::{Amount, Blockchain, Nonce, Rate};
use crate::types::{AssetOrCrosschain, Prefix};
use crate::utils::{bigint_to_nash_r, bigint_to_nash_sig, hash_neo_message};
use nash_mpc::curves::secp256_r1::Secp256r1Point;
use nash_mpc::curves::traits::ECPoint;
use nash_mpc::rust_bigint::traits::Converter;
use nash_mpc::rust_bigint::BigInt;

use super::super::super::signer::Signer;

/// FillOrder data allows an NEO smart contract to execute an order that
/// has been placed for an NEO blockchain asset. In practice, the Nash ME
/// does not tend to directly execute these orders and instead relies on clients
/// syncing aggregate state periodically via `syncState`.

#[derive(Clone, Debug, PartialEq)]
pub struct FillOrder {
    pub prefix: Prefix,                // 1 byte
    pub address: Address,              // 20 bytes
    pub asset_from: AssetOrCrosschain, // 32 bytes (or 20 bytes in case of NEP5 or cross-chain)
    pub asset_to: AssetOrCrosschain,   // 32 bytes (or 20 bytes in case of NEP5 or cross-chain)
    pub nonce_from: Nonce,             // 8 bytes
    pub nonce_to: Nonce,               // 8 bytes
    pub amount: Amount,                // 8 bytes
    pub min_order: Rate,               // 8 bytes
    pub max_order: Rate,               // 8 bytes
    pub fee_rate: Rate,                // 8 bytes
    pub order_nonce: Nonce,            // 8 bytes
    pub public_key: Secp256r1Point,    // 33 bytes
}

impl FillOrder {
    /// Constructor for FillOrder payload
    pub fn new(
        public_key: PublicKey,
        asset_from: AssetOrCrosschain,
        asset_to: AssetOrCrosschain,
        nonce_from: Nonce,
        nonce_to: Nonce,
        amount: Amount,
        min_order: Rate,
        max_order: Rate,
        fee_rate: Rate,
        order_nonce: Nonce,
    ) -> Self {
        Self {
            prefix: Prefix::FillOrder,
            address: public_key.to_address(),
            asset_from,
            asset_to,
            nonce_from,
            nonce_to,
            amount,
            min_order,
            max_order,
            fee_rate,
            order_nonce,
            public_key: public_key.inner,
        }
    }

    /// Serialize FillOrder payload to bytes interpretable by the NEO smart contract
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok([
            &self.prefix.to_bytes()[..],
            &self.address.to_bytes()[..],
            &self.asset_from.to_neo_bytes()[..],
            &self.asset_to.to_neo_bytes()[..],
            &self.nonce_from.to_le_bytes()[..],
            &self.nonce_to.to_le_bytes()[..],
            &self.amount.to_le_bytes()?[..],
            &self.min_order.to_le_bytes()?[..],
            &self.max_order.to_le_bytes()?[..],
            &self.fee_rate.to_le_bytes()?[..],
            &self.order_nonce.to_le_bytes()[..],
            &self.public_key.bytes_compressed_to_big_int().to_bytes()[..],
        ]
        .concat())
    }

    /// Serialize FillOrder payload to bytes as a hex string, interpretable by a smart contract
    pub fn to_hex(&self) -> Result<String> {
        Ok(hex::encode(self.to_bytes()?).to_uppercase())
    }

    /// Hash a FillOrder for signing with an NEO private key or Nash MPC protocol
    pub fn hash(&self) -> Result<BigInt> {
        let bytes = self.to_bytes()?;
        Ok(hash_neo_message(&bytes))
    }

    /// Construct GraphQL object corresponding to a blockchain signature on ETH fillorder data.
    pub fn to_blockchain_signature(
        &self,
        signer: &mut Signer,
    ) -> Result<place_limit_order::BlockchainSignature> {
        let payload_hash = self.hash()?;
        let (sig, r, pub_key) = signer.sign_child_key(payload_hash, Blockchain::NEO)?;
        let graphql_output = place_limit_order::BlockchainSignature {
            blockchain: place_limit_order::Blockchain::NEO,
            nonce_from: Some(self.nonce_from.into()),
            nonce_to: Some(self.nonce_to.into()),
            public_key: Some(pub_key),
            signature: bigint_to_nash_sig(sig),
            r: Some(bigint_to_nash_r(r)),
        };
        Ok(graphql_output)
    }
}

#[cfg(test)]
mod tests {
    use super::{FillOrder, PublicKey};
    use crate::types::Asset;
    use crate::types::{Amount, Nonce, OrderRate, Rate};

    #[test]
    fn test_sell_market() {
        let order_data = FillOrder::new(
            PublicKey::new("0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63")
                .unwrap(),
            Asset::NEO.into(),
            Asset::GAS.into(),
            Nonce::Value(5432876),
            Nonce::Value(5432876),
            // FIXME: why is precision = 3 here!?
            Amount::new("10.000", 3).unwrap(),
            Rate::MinOrderRate,
            Rate::MaxOrderRate,
            Rate::MaxFeeRate,
            Nonce::Value(5432876),
        );
        // "neo_gas": {
        //   "amount": {
        //     "value": "10.000",
        //     "currency": "neo"
        //   },
        //   "buyOrSell": "SELL",
        //   "marketName": "neo_gas",
        //   "timestamp": 12345648,
        //   "nonceFrom": 5432876,
        //   "nonceTo": 5432876,
        //   "nonceOrder": 5432876,
        //
        // 01 // prefix
        // 6F6F85BFFFB412967AF3DD0D71A5E2F8A759006C //address
        // 9B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC5 // NEO
        // E72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C60 // GAS
        // 2CE6520000000000 // nonceFrom
        // 2CE6520000000000 // nonceTo
        // 1027000000000000 // amount
        // 0000000000000000 // minOrderRate
        // FFFFFFFFFFFFFFFF // maxOrderRate
        // 90D0030000000000 // maxFeeRate
        // 2CE6520000000000 // nonceOrder
        // 0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63 // public key
        assert_eq!(
            order_data.to_hex().unwrap(),
            "016F6F85BFFFB412967AF3DD0D71A5E2F8A759006C9B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC5E72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C602CE65200000000002CE652000000000010270000000000000000000000000000FFFFFFFFFFFFFFFF90D00300000000002CE65200000000000292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63"
        );
    }

    #[test]
    fn test_sell_market_crosschain() {
        let order_data = FillOrder::new(
            PublicKey::new("0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63")
                .unwrap(),
            Asset::ETH.into(),
            Asset::NEO.into(),
            Nonce::Value(5432876),
            Nonce::Value(5432876),
            // FIXME: why is precision = 8 here!?
            Amount::new("10.00000000", 8).unwrap(),
            Rate::MinOrderRate,
            Rate::MaxOrderRate,
            Rate::MaxFeeRate,
            Nonce::Value(5432876),
        );
        // ETH NEO
        // "eth_neo": {
        //   "amount": {
        //     "value": "10.00000000",
        //     "currency": "eth"
        //   },
        //   "buyOrSell": "SELL",
        //   "marketName": "eth_neo",
        //   "timestamp": 12345648,
        //   "nonceFrom": 5432876,
        //   "nonceTo": 5432876,
        //   "nonceOrder": 5432876,
        //
        // 01 // prefix
        // 6F6F85BFFFB412967AF3DD0D71A5E2F8A759006C // address
        // FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF // Ethereum
        // 9B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC5 // NEO
        // 2CE6520000000000 // nonceFrom
        // 2CE6520000000000 // nonceTo
        // 00CA9A3B00000000 // amount
        // 0000000000000000 // minOrderRate
        // FFFFFFFFFFFFFFFF // maxOrderRate
        // 90D0030000000000 // maxFeeRate
        // 2CE6520000000000 // nonceOrder
        // 0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63 // public key
        assert_eq!(
            order_data.to_hex().unwrap(),
            "016F6F85BFFFB412967AF3DD0D71A5E2F8A759006CFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF9B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC52CE65200000000002CE652000000000000CA9A3B000000000000000000000000FFFFFFFFFFFFFFFF90D00300000000002CE65200000000000292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63"
        );
    }

    #[test]
    fn test_sell_limit() {
        let order_data = FillOrder::new(
            PublicKey::new("0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63")
                .unwrap(),
            Asset::GAS.into(),
            Asset::NEO.into(),
            Nonce::Value(5432876),
            Nonce::Value(5432876),
            // FIXME: why is precision = 4 here!?
            Amount::new("10.000000", 4).unwrap(),
            Rate::MinOrderRate,
            OrderRate::new("17.000").unwrap().invert_rate(None).into(),
            Rate::MaxFeeRate,
            Nonce::Value(5432876),
        );
        // "amount": {
        //   "value": "10.000000",
        //   "currency": "gas"
        // },
        // "buyOrSell": "SELL",
        // "marketName": "gas_neo",
        // "cancellationPolicy": "immediate_or_cancel",
        // "limitPrice": {
        //   "value": "17.000",
        //   "currency_a": "neo",
        //   "currency_b": "gas"
        // },
        // "timestamp": 12345648,
        // "nonceFrom": 5432876,
        // "nonceTo": 5432876,
        // "nonceOrder": 5432876,
        //
        // 01 // prefix
        // 6F6F85BFFFB412967AF3DD0D71A5E2F8A759006C // address
        // E72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C60 // GAS
        // 9B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC5 // NEO
        // 2CE6520000000000 // nonceFrom
        // 2CE6520000000000 // nonceTo
        // A086010000000000 // amount
        // 0000000000000000 // minOrderRate
        // F0C1590000000000 // maxOrderRate
        // 90D0030000000000 // maxFeeRate
        // 2CE6520000000000 // nonceOrder
        // 0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63 // public key
        assert_eq!(
            order_data.to_hex().unwrap(),
            "016F6F85BFFFB412967AF3DD0D71A5E2F8A759006CE72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C609B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC52CE65200000000002CE6520000000000A0860100000000000000000000000000F0C159000000000090D00300000000002CE65200000000000292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63"
        );
    }

    #[test]
    fn run() {
        let bytes = hex::decode("53b7577befb37d4d3b95a02f60f5da8933ab5f04").unwrap();
        let rev_bytes: Vec<u8> = bytes.iter().rev().map(|x| x.clone()).collect();
        println!("{}", hex::encode(&rev_bytes));
    }

    #[test]
    fn test_sell_limit_crosschain() {
        let order_data = FillOrder::new(
            PublicKey::new("0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63")
                .unwrap(),
            Asset::ETH.into(),
            Asset::GAS.into(),
            Nonce::Value(5432876),
            Nonce::Value(5432876),
            // FIXME: why is precision = 8 here!?
            Amount::new("10.00000000", 8).unwrap(),
            Rate::MinOrderRate,
            OrderRate::new("0.0024").unwrap().invert_rate(None).into(),
            Rate::MaxFeeRate,
            Nonce::Value(5432876),
        );
        // "amount": {
        //   "value": "10.00000000",
        //   "currency": "eth"
        // },
        // "buyOrSell": "SELL",
        // "marketName": "eth_gas",
        // "cancellationPolicy": "immediate_or_cancel",
        // "limitPrice": {
        //   "value": "0.0024",
        //   "currency_a": "gas",
        //   "currency_b": "eth"
        // },
        // "timestamp": 1565361133707,
        // "nonceFrom": 5432876,
        // "nonceTo": 5432876,
        // "nonceOrder": 5432876,
        //
        //01 // prefix
        //6F6F85BFFFB412967AF3DD0D71A5E2F8A759006C // address
        //FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF // Ethereum
        //E72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C60 // GAS
        //2CE6520000000000 // nonceFrom
        //2CE6520000000000 // nonceTo
        //00CA9A3B00000000 // amount
        //0000000000000000 // minOrderRate
        //ABE086B309000000 // maxOrderRate
        //90D0030000000000 // maxFeeRate
        //2CE6520000000000 // nonceOrder
        //0292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63 // public key
        // 016F6F85BFFFB412967AF3DD0D71A5E2F8A759006CFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C602CE65200000000002CE652000000000000CA9A3B000000000000000000000000AAE086B30900000090D00300000000002CE65200000000000292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63
        // 016F6F85BFFFB412967AF3DD0D71A5E2F8A759006CFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C602CE65200000000002CE652000000000000CA9A3B000000000000000000000000ABE086B30900000090D00300000000002CE65200000000000292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63

        let serialized = order_data.to_hex().unwrap();
        let test = "016F6F85BFFFB412967AF3DD0D71A5E2F8A759006CFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C602CE65200000000002CE652000000000000CA9A3B000000000000000000000000AAE086B30900000090D00300000000002CE65200000000000292CBF3790801CEF47C5CDC9ABF4B010EC50AAD117F595350D77ECD385D286E63".to_string();

        assert_eq!(serialized, test);
    }
}
