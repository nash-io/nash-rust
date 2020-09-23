use super::bigdecimal_to_nash_u64;
use crate::errors::{ProtocolError, Result};
use crate::graphql::place_limit_order;
use crate::types::neo::{Address, PublicKey};
use crate::types::{Amount, Asset, Blockchain, Nonce, OrderRate, Rate};
use crate::types::{AssetOrCrosschain, Prefix};
use crate::utils::{bigint_to_nash_r, bigint_to_nash_sig, hash_neo_message};
use mpc_wallet_lib::rust_bigint::traits::Converter;
use mpc_wallet_lib::rust_bigint::BigInt;
use mpc_wallet_lib::curves::secp256_r1::Secp256r1Point;
use mpc_wallet_lib::curves::traits::ECPoint;

use super::super::super::signer::Signer;

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

impl Rate {
    /// Convert any Rate into bytes for encoding in a NEO payload
    pub fn to_le_bytes(&self) -> Result<[u8; 8]> {
        let zero_bytes = (0 as f64).to_le_bytes();
        let bytes = match self {
            Self::OrderRate(rate) | Self::FeeRate(rate) => rate.to_le_bytes()?,
            Self::MinOrderRate | Self::MinFeeRate => zero_bytes,
            Self::MaxOrderRate => [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            // 0.0025 * 10^8 = 250,000
            Self::MaxFeeRate => [0x90, 0xD0, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
        };
        Ok(bytes)
    }
}

impl OrderRate {
    /// Serialize the OrderRate to bytes for payload creation. We always use a
    /// precision of 8 and multiplication factor of 10^8
    pub fn to_le_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_le_bytes();
        Ok(bytes)
    }
}

impl Asset {
    /// This maps assets onto their representation in the NEO SC protocol.
    /// Each asset is represented by two bytes which serve as an identifier
    pub fn to_neo_bytes(&self) -> Vec<u8> {
        match self {
            Self::ETH => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::BAT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::OMG => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::USDC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::USDT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::ZRX => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::LINK => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::QNT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::RLC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::ANT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::TRAC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::GUNTHY => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::BTC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::NEO => {
                BigInt::from_hex("9B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC5")
                    .unwrap()
                    .to_bytes()
            }
            Self::GAS => {
                BigInt::from_hex("E72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C60")
                    .unwrap()
                    .to_bytes()
            },
            Self::NNN => BigInt::from_hex("045fab3389daf5602fa0953b4d7db3ef7b57b753")
                .unwrap()
                .to_bytes(),
        }
    }

    /// Given two bytes asset id, return asset
    pub fn from_neo_bytes(bytes: &[u8; 32]) -> Result<Self> {
        match bytes {
            [0x9B, 0x7C, 0xFF, 0xDA, 0xA6, 0x74, 0xBE, 0xAE, 0x0F, 0x93, 0x0E, 0xBE, 0x60, 0x85, 0xAF, 0x90, 0x93, 0xE5, 0xFE, 0x56, 0xB3, 0x4A, 0x5C, 0x22, 0x0C, 0xCD, 0xCF, 0x6E, 0xFC, 0x33, 0x6F, 0xC5] => {
                Ok(Self::NEO)
            }
            [0xE7, 0x2D, 0x28, 0x69, 0x79, 0xEE, 0x6C, 0xB1, 0xB7, 0xE6, 0x5D, 0xFD, 0xDF, 0xB2, 0xE3, 0x84, 0x10, 0x0B, 0x8D, 0x14, 0x8E, 0x77, 0x58, 0xDE, 0x42, 0xE4, 0x16, 0x8B, 0x71, 0x79, 0x2C, 0x60] => {
                Ok(Self::GAS)
            }
            _ => Err(ProtocolError("Invalid Asset ID in bytes")),
        }
    }
}

impl AssetOrCrosschain {
    /// Convert asset to id in bytes interpretable by the NEO
    /// smart contract, or `0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF` if it is a cross-chain asset
    pub fn to_neo_bytes(&self) -> Vec<u8> {
        match self {
            Self::Crosschain => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::Asset(asset) => asset.to_neo_bytes().to_vec(),
        }
    }
    /// Read asset bytes from a protocol payload and convert into
    /// an Asset or mark as cross-chain
    pub fn from_neo_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() == 20
            && bytes
                == [
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ]
                .to_vec()
        {
            Ok(Self::Crosschain)
        } else if bytes.len() == 32 {
            // convert vector to fixed-size array
            let mut arr = [0; 32];
            let bytes = &bytes[..arr.len()];
            arr.copy_from_slice(bytes);
            Ok(Self::Asset(Asset::from_neo_bytes(&arr)?))
        } else {
            Err(ProtocolError("Invalid Asset ID in bytes"))
        }
    }
}

impl Amount {
    /// Serialize Amount to Little Endian bytes for NEO payload creation.
    pub fn to_le_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_le_bytes();
        Ok(bytes)
    }
}

impl Nonce {
    /// Serialize Nonce for NEO payload as 8 bytes LittleEndian
    pub fn to_le_bytes(&self) -> [u8; 8] {
        match self {
            Self::Value(value) => u64::from(*value).to_le_bytes(),
            Self::Crosschain => u64::from(Nonce::crosschain()).to_le_bytes(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Amount, FillOrder, Nonce, OrderRate, PublicKey, Rate};
    use crate::types::Asset;

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
    fn run(){
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
