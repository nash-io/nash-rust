// based on MIT/Apache-licensed https://github.com/KZen-networks/curv/blob/master/src/elliptic/curves/traits.rs

use rust_bigint::BigInt;

pub trait ECScalar<SK> {
    fn new_random() -> Self;
    fn from(n: &BigInt) -> Self;
    fn to_bigint(&self) -> BigInt;
    fn q() -> BigInt;
    fn add(&self, other: &SK) -> Self;
    fn mul(&self, other: &SK) -> Self;
    fn sub(&self, other: &SK) -> Self;
    fn invert(&self) -> Self;
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
    fn from_bytes(bytes: &[u8]) -> Result<Self, ()>;
    fn to_vec(&self) -> Vec<u8>;
    fn scalar_mul(&self, fe: &SK) -> Self;
    fn add_point(&self, other: &PK) -> Self;
    fn sub_point(&self, other: &PK) -> Self;
    fn from_coor(x: &BigInt, y: &BigInt) -> Self;
    fn to_hex(&self) -> String;
    fn from_hex(s: &str) -> Result<Self, ()>;
}
