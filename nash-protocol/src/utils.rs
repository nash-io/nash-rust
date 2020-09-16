//! Miscellaneous helper functions

use crate::errors::{ProtocolError, Result};
use bigdecimal::BigDecimal;
use mpc_wallet_lib::bigints::traits::Converter;
use mpc_wallet_lib::bigints::BigInt;
use sha3::{Digest, Keccak256};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "num_bigint")]
use num_traits::Num;

/// Format BigInt signature values as hex in the way Nash ME expects
pub fn bigint_to_nash_sig(num: BigInt) -> String {
    format!("{:0>1024}", num.to_hex())
}

/// Format Nash MPC r value as hex in the way Nash ME expects
pub fn bigint_to_nash_r(r: BigInt) -> String {
    format!("{:0>66}", r.to_hex())
}

/// Hash ethereum data with keccak
pub fn hash_eth_message(msg: &[u8]) -> BigInt {
    // For signing in Ethereum, the message is first prefixed with the header \x19Ethereum Signed Message:\n followed by the length of the message.
    // The length of the message is always 32 bytes, since the "message" to be used is a (keccak256) hash of the actual message.
    const ETH_PREFIX: &[u8] = b"\x19Ethereum Signed Message:\n32";
    // append the hash of the message to the prefix and hash the result all over again
    let msg_hash =
        Keccak256::digest(&[&ETH_PREFIX[..], &Keccak256::digest(&msg).to_vec()].concat());
    BigInt::from_bytes(&msg_hash)
}

/// Hash NEO message (FIXME: move)
pub fn hash_neo_message(msg: &[u8]) -> BigInt {
    let msg_hash_once = sha2::Sha256::digest(msg);
    let msg_hash_twice = sha2::Sha256::digest(&msg_hash_once[..]);
    BigInt::from_str_radix(&hex::encode(msg_hash_twice), 16).unwrap()
}

// SHA256 hash for ME request payloads
pub fn hash_message(message: &str) -> BigInt {
    let hash = sha2::Sha256::digest(message.as_bytes());
    BigInt::from_str_radix(&hex::encode(hash), 16).unwrap()
}

/// Produce DER encoding in bytes for secp256k1 signature
pub fn der_encode_sig(r: &BigInt, s: &BigInt) -> Vec<u8> {
    let r_bytes = der_encode_sig_value(r);
    let s_bytes = der_encode_sig_value(s);
    let mut der_bytes = Vec::new();
    // beginning of DER encoding, indicates compound value
    der_bytes.push(0x30);
    // length descriptor for everything that follows
    der_bytes.push((r_bytes.len() + s_bytes.len()) as u8);
    // append r and s values
    for byte in r_bytes {
        der_bytes.push(byte);
    }
    for byte in s_bytes {
        der_bytes.push(byte)
    }
    der_bytes
}

fn der_encode_sig_value(r_or_s: &BigInt) -> Vec<u8> {
    let mut bytes = r_or_s.to_bytes();
    // if highest bit of first byte is 1, prepend 0x00
    if bytes[0] >= 0b1000_0000 {
        bytes.insert(0, 0x00);
    }
    // length descriptor for r or s data
    bytes.insert(0, bytes.len() as u8);
    // indicates integer value follows
    bytes.insert(0, 0x02);
    bytes
}

/// Get current time in millis as an `i64` for GraphQL timestamps
pub fn current_time_as_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

pub fn decode_hexstr(hex_str: &str) -> Result<Vec<u8>> {
    Ok(hex::decode(hex_str).map_err(|_| ProtocolError("Could not decode hex string"))?)
}

// FIXME: this is super ugly
/// Pad numerical string with zeros to the desired precision. Required for Nash Me backend
pub fn pad_zeros(str_num: &str, precision: u32) -> Result<String> {
    let components: Vec<&str> = str_num.split('.').collect();
    match components.len() {
        1 => {
            let mut zeros = ".".to_string();
            for _ in 0..precision {
                zeros += "0"
            }
            Ok(str_num.to_string() + &zeros)
        }
        2 => {
            let mut zeros = "".to_string();
            let existing_count = components[1].len();
            if existing_count as u32 > precision {
                // If precision too high, lower it
                let num_zeros_to_subtract = existing_count as u32 - precision;
                let reduced_prec = &str_num[..(str_num.len() - (num_zeros_to_subtract as usize))];
                Ok(reduced_prec.to_string())
            } else {
                for _ in 0..(precision - existing_count as u32) {
                    zeros += "0"
                }
                Ok(str_num.to_string() + &zeros)
            }
        }
        _ => Err(ProtocolError(
            "String was not a valid number for zero padding",
        )),
    }
}

pub fn convert_at_price(amount: &str, price: &str) -> Result<String> {
    let amount_bd = BigDecimal::from_str(amount)
        .map_err(|_| ProtocolError("Could not convert amount to bigdecimal"))?;
    let price_bd = BigDecimal::from_str(price)
        .map_err(|_| ProtocolError("Could not convert price to bigdecimal"))?;
    let convert = amount_bd * price_bd.inverse();
    Ok(convert.to_string())
}
