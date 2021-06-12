// based on MIT/Apache-licensed https://github.com/KZen-networks/curv/blob/master/src/elliptic/curves/traits.rs

use crate::NashMPCError;
use rust_bigint::BigInt;

pub trait ECScalar<SK>
where
    Self: Sized,
{
    fn new_random() -> Result<Self, NashMPCError>;
    fn from(n: &BigInt) -> Result<Self, NashMPCError>;
    fn to_bigint(&self) -> BigInt;
    fn q() -> BigInt;
    fn add(&self, other: &SK) -> Result<Self, NashMPCError>;
    fn mul(&self, other: &SK) -> Result<Self, NashMPCError>;
    fn sub(&self, other: &SK) -> Result<Self, NashMPCError>;
    fn invert(&self) -> Result<Self, NashMPCError>;
    fn to_vec(&self) -> Vec<u8>;
}

pub trait ECPoint<PK, SK>
where
    Self: Sized,
{
    fn generator() -> Self;
    fn x_coor(&self) -> BigInt;
    fn y_coor(&self) -> BigInt;
    fn to_bigint(&self) -> BigInt;
    fn from_bytes(bytes: &[u8]) -> Result<Self, NashMPCError>;
    fn to_vec(&self) -> Vec<u8>;
    fn scalar_mul(&self, fe: &SK) -> Result<Self, NashMPCError>;
    fn add_point(&self, other: &PK) -> Result<Self, NashMPCError>;
    fn sub_point(&self, other: &PK) -> Result<Self, NashMPCError>;
    fn from_coor(x: &BigInt, y: &BigInt) -> Result<Self, NashMPCError>;
    fn to_hex(&self) -> String;
    fn from_hex(s: &str) -> Result<Self, NashMPCError>;
}
