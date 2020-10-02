// secp256k1 elliptic curve utility functions.
// based on MIT/Apache-licensed https://github.com/KZen-networks/curv/blob/master/src/elliptic/curves/secp256_k1.rs

use super::traits::{ECPoint, ECScalar};
use getrandom::getrandom;
use rust_bigint::traits::{Converter, Modulo};
use rust_bigint::BigInt;
use secp256k1::constants::{
    COMPACT_SIGNATURE_SIZE, CURVE_ORDER, GENERATOR_X, GENERATOR_Y, MESSAGE_SIZE, SECRET_KEY_SIZE,
    UNCOMPRESSED_PUBLIC_KEY_SIZE,
};
use secp256k1::{All, Message, PublicKey, Secp256k1, SecretKey};
use serde::de;
use serde::de::Visitor;
use serde::ser::{Serialize, Serializer};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::ops::{Add, Mul};
use std::ptr;
use std::sync::{atomic, Once};
use zeroize::Zeroize;

#[derive(Clone, Debug)]
pub struct Secp256k1Scalar {
    purpose: &'static str,
    pub(crate) fe: SecretKey,
}

#[derive(Clone, Debug, Copy)]
pub struct Secp256k1Point {
    purpose: &'static str,
    pub(crate) ge: PublicKey,
}

impl Secp256k1Scalar {
    /// sign() is basically a textbook ECDSA sign function. In contrast to MPC, sign() makes use of RFC6979 (deterministic but still cryptographically secure nonce generation) and produces the same signature given the same secret key and message.
    /// It is not needed for MPC but used as a faster replacement for the JS implementation
    pub fn sign(self, msg_hash: &BigInt) -> (BigInt, BigInt) {
        let msg_bytes = msg_hash.to_bytes();
        // add leading zeroes if necessary
        let mut msg_vec = vec![0; MESSAGE_SIZE - msg_bytes.len()];
        msg_vec.extend_from_slice(&msg_bytes);
        let msg = Message::from_slice(&msg_vec).unwrap();
        let signature = get_context()
            .sign(&msg, &self.fe)
            .serialize_compact();
        let r = BigInt::from_bytes(&signature[0..COMPACT_SIGNATURE_SIZE / 2]);
        let s = BigInt::from_bytes(&signature[COMPACT_SIGNATURE_SIZE / 2..COMPACT_SIGNATURE_SIZE]);
        (r, s)
    }
}

impl Secp256k1Point {
    pub fn random_point() -> Secp256k1Point {
        let random_scalar: Secp256k1Scalar = Secp256k1Scalar::new_random();
        let base_point = Secp256k1Point::generator();
        let pk = base_point.scalar_mul(&random_scalar.fe);
        Secp256k1Point {
            purpose: "random_point",
            ge: pk.ge,
        }
    }
}

impl Zeroize for Secp256k1Scalar {
    fn zeroize(&mut self) {
        let zero = unsafe { std::mem::transmute::<[u8; SECRET_KEY_SIZE], SecretKey>([0u8; SECRET_KEY_SIZE]) };
        let zero_scalar = Secp256k1Scalar {
            purpose: "zero",
            fe: zero,
        };
        unsafe { ptr::write_volatile(self, zero_scalar) };
        atomic::fence(atomic::Ordering::SeqCst);
        atomic::compiler_fence(atomic::Ordering::SeqCst);
    }
}

impl ECScalar<SecretKey> for Secp256k1Scalar {
    fn new_random() -> Secp256k1Scalar {
        let mut arr = [0u8; 32];
        getrandom(&mut arr).unwrap();
        Secp256k1Scalar {
            purpose: "random",
            fe: SecretKey::from_slice(&arr[0..arr.len()]).unwrap(),
        }
    }

    fn from(n: &BigInt) -> Secp256k1Scalar {
        let curve_order = Secp256k1Scalar::q();
        let n_reduced = BigInt::mod_add(n, &BigInt::from(0), &curve_order);
        let mut v = BigInt::to_vec(&n_reduced);

        if v.len() < SECRET_KEY_SIZE {
            let mut template = vec![0; SECRET_KEY_SIZE - v.len()];
            template.extend_from_slice(&v);
            v = template;
        }

        Secp256k1Scalar {
            purpose: "from_big_int",
            fe: SecretKey::from_slice(&v).unwrap(),
        }
    }

    fn to_bigint(&self) -> BigInt {
        BigInt::from_bytes(&(self.fe[0..self.fe.len()]))
    }

    fn q() -> BigInt {
        BigInt::from_bytes(&CURVE_ORDER.as_ref())
    }

    fn add(&self, other: &SecretKey) -> Secp256k1Scalar {
        let mut plus = other.clone();
        plus.add_assign(&self.to_vec()).unwrap();
        Secp256k1Scalar {
            purpose: "add",
            fe: plus,
        }
    }

    fn mul(&self, other: &SecretKey) -> Secp256k1Scalar {
        let mut mul = other.clone();
        mul.mul_assign(&self.to_vec()).unwrap();
        Secp256k1Scalar {
            purpose: "mul",
            fe: mul,
        }
    }

    fn sub(&self, other: &SecretKey) -> Secp256k1Scalar {
        let mut sub = other.clone();
        sub.negate_assign();
        sub.add_assign(&self.to_vec()).unwrap();
        Secp256k1Scalar {
            purpose: "sub",
            fe: sub,
        }
    }

    fn invert(&self) -> Secp256k1Scalar {
        ECScalar::from(&BigInt::mod_inv(&self.to_bigint(), &Secp256k1Scalar::q()))
    }

    /// convert to vector and pad with zeros if necessary
    fn to_vec(&self) -> Vec<u8> {
        let vec = BigInt::to_vec(&self.to_bigint());
        let mut v = vec![0; SECRET_KEY_SIZE - vec.len()];
        v.extend(&vec);
        v
    }
}

impl Mul<Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn mul(self, other: Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).mul(&other.fe)
    }
}

impl<'o> Mul<&'o Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn mul(self, other: &'o Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).mul(&other.fe)
    }
}

impl Add<Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn add(self, other: Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).add(&other.fe)
    }
}

impl<'o> Add<&'o Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn add(self, other: &'o Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).add(&other.fe)
    }
}

impl Serialize for Secp256k1Scalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:0>64}", self.to_bigint().to_hex()))
    }
}

impl<'de> Deserialize<'de> for Secp256k1Scalar {
    fn deserialize<D>(deserializer: D) -> Result<Secp256k1Scalar, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Secp256k1ScalarVisitor)
    }
}

struct Secp256k1ScalarVisitor;

impl<'de> Visitor<'de> for Secp256k1ScalarVisitor {
    type Value = Secp256k1Scalar;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Secp256k1Scalar")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Secp256k1Scalar, E> {
        let v = BigInt::from_hex(&s).expect("Failed in serde");
        Ok(ECScalar::from(&v))
    }
}

impl PartialEq for Secp256k1Scalar {
    fn eq(&self, other: &Secp256k1Scalar) -> bool {
        self.fe == other.fe
    }
}

impl PartialEq for Secp256k1Point {
    fn eq(&self, other: &Secp256k1Point) -> bool {
        self.ge == other.ge
    }
}

impl Zeroize for Secp256k1Point {
    fn zeroize(&mut self) {
        unsafe { ptr::write_volatile(self, Secp256k1Point::generator()) };
        atomic::fence(atomic::Ordering::SeqCst);
        atomic::compiler_fence(atomic::Ordering::SeqCst);
    }
}

impl ECPoint<PublicKey, SecretKey> for Secp256k1Point {
    fn generator() -> Secp256k1Point {
        let mut v = vec![4 as u8];
        v.extend(GENERATOR_X.as_ref());
        v.extend(GENERATOR_Y.as_ref());
        Secp256k1Point {
            purpose: "base_fe",
            ge: PublicKey::from_slice(&v).unwrap(),
        }
    }

    fn to_bigint(&self) -> BigInt {
        let serial = self.ge.serialize();
        BigInt::from_bytes(&serial[0..33])
    }

    fn x_coor(&self) -> BigInt {
        let serialized_pk = PublicKey::serialize_uncompressed(&self.ge);
        let x = &serialized_pk[1..=serialized_pk.len() / 2];
        BigInt::from_bytes(&x.to_vec()[..])
    }

    fn y_coor(&self) -> BigInt {
        let serialized_pk = PublicKey::serialize_uncompressed(&self.ge);
        let y = &serialized_pk[(serialized_pk.len() - 1) / 2 + 1..serialized_pk.len()];
        BigInt::from_bytes(&y.to_vec()[..])
    }

    fn from_bytes(bytes: &[u8]) -> Result<Secp256k1Point, ()> {
        match PublicKey::from_slice(&bytes) {
            Ok(v) => Ok(Secp256k1Point {
                purpose: "random",
                ge: v,
            }),
            Err(_) => return Err(()),
        }
    }

    fn to_vec(&self) -> Vec<u8> {
        self.ge.serialize_uncompressed().to_vec()
    }

    fn scalar_mul(&self, fe: &SecretKey) -> Secp256k1Point {
        let mut new_point = *self;
        new_point
            .ge
            .mul_assign(get_context(), &fe[..])
            .expect("Assignment expected");
        new_point
    }

    fn add_point(&self, other: &PublicKey) -> Secp256k1Point {
        Secp256k1Point {
            purpose: "combine",
            ge: self.ge.combine(other).unwrap(),
        }
    }

    fn sub_point(&self, other: &PublicKey) -> Secp256k1Point {
        let mut minus = other.clone();
        minus.negate_assign(get_context());
        self.add_point(&minus)
    }

    fn from_coor(x: &BigInt, y: &BigInt) -> Secp256k1Point {
        const COOR_SIZE: usize = (UNCOMPRESSED_PUBLIC_KEY_SIZE - 1) / 2;
        let mut v = vec![4 as u8];
        let vec_x = BigInt::to_vec(x);
        // pad with zeros if necessary
        v.extend_from_slice(&vec![0; COOR_SIZE - vec_x.len()]);
        v.extend(vec_x);
        let vec_y = BigInt::to_vec(y);
        // pad with zeros if necessary
        v.extend_from_slice(&vec![0; COOR_SIZE - vec_y.len()]);
        v.extend(vec_y);
        Secp256k1Point {
            purpose: "base_fe",
            ge: PublicKey::from_slice(&v).unwrap(),
        }
    }

    fn to_hex(&self) -> String {
        format!("{:0>66}", self.to_bigint().to_hex())
    }

    fn from_hex(s: &str) -> Result<Secp256k1Point, ()> {
        let v = match BigInt::from_hex(s) {
            Ok(v) => v,
            Err(_) => return Err(()),
        };
        let point = match Secp256k1Point::from_bigint(&v) {
            Ok(v) => v,
            Err(_) => return Err(()),
        };
        Ok(point)
    }
}

impl Secp256k1Point {
    /// derive point from BigInt
    pub fn from_bigint(i: &BigInt) -> Result<Secp256k1Point, ()> {
        let vec = BigInt::to_vec(i);
        let point = match Secp256k1Point::from_bytes(&vec) {
            Ok(v) => v,
            Err(_) => return Err(()),
        };
        Ok(point)
    }
}

static mut CONTEXT: Option<Secp256k1<All>> = None;
pub fn get_context() -> &'static Secp256k1<All> {
    static INIT_CONTEXT: Once = Once::new();
    INIT_CONTEXT.call_once(|| unsafe {
        CONTEXT = Some(Secp256k1::new());
    });
    unsafe { CONTEXT.as_ref().unwrap() }
}

impl Mul<Secp256k1Scalar> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn mul(self, other: Secp256k1Scalar) -> Self::Output {
        self.scalar_mul(&other.fe)
    }
}

impl<'o> Mul<&'o Secp256k1Scalar> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn mul(self, other: &'o Secp256k1Scalar) -> Self::Output {
        self.scalar_mul(&other.fe)
    }
}

impl<'o> Mul<&'o Secp256k1Scalar> for &'o Secp256k1Point {
    type Output = Secp256k1Point;
    fn mul(self, other: &'o Secp256k1Scalar) -> Self::Output {
        self.scalar_mul(&other.fe)
    }
}

impl Add<Secp256k1Point> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn add(self, other: Secp256k1Point) -> Self::Output {
        self.add_point(&other.ge)
    }
}

impl<'o> Add<&'o Secp256k1Point> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn add(self, other: &'o Secp256k1Point) -> Self::Output {
        self.add_point(&other.ge)
    }
}

impl<'o> Add<&'o Secp256k1Point> for &'o Secp256k1Point {
    type Output = Secp256k1Point;
    fn add(self, other: &'o Secp256k1Point) -> Self::Output {
        self.add_point(&other.ge)
    }
}

impl Serialize for Secp256k1Point {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Secp256k1Point {
    fn deserialize<D>(deserializer: D) -> Result<Secp256k1Point, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Secp256k1PointVisitor)
    }
}

struct Secp256k1PointVisitor;

impl<'de> Visitor<'de> for Secp256k1PointVisitor {
    type Value = Secp256k1Point;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Secp256k1Point")
    }

    fn visit_str<E>(self, s: &str) -> Result<Secp256k1Point, E>
    where
        E: de::Error,
    {
        match Secp256k1Point::from_hex(&s.to_string()) {
            Ok(v) => Ok(v),
            Err(_) => Err(E::custom(format!(
                "Error deriving Secp256k1Point from string: {}",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BigInt, Secp256k1Point, Secp256k1Scalar};
    use crate::curves::traits::{ECPoint, ECScalar};
    use bincode;
    use rust_bigint::traits::{Converter, Modulo};
    use serde_json;

    #[test]
    fn serialize_sk() {
        let scalar: Secp256k1Scalar = ECScalar::from(&BigInt::from(123456));
        let s = serde_json::to_string(&scalar).expect("Failed in serialization");
        assert_eq!(
            s,
            "\"000000000000000000000000000000000000000000000000000000000001e240\""
        );
    }

    #[test]
    fn serialize_rand_pk_verify_pad() {
        let vx = BigInt::from_hex(
            &"ccaf75ab7960a01eb421c0e2705f6e84585bd0a094eb6af928c892a4a2912508".to_string(),
        )
        .unwrap();

        let vy = BigInt::from_hex(
            &"e788e294bd64eee6a73d2fc966897a31eb370b7e8e9393b0d8f4f820b48048df".to_string(),
        )
        .unwrap();

        Secp256k1Point::from_coor(&vx, &vy); // x and y of size 32

        let x = BigInt::from_hex(
            &"5f6853305467a385b56a5d87f382abb52d10835a365ec265ce510e04b3c3366f".to_string(),
        )
        .unwrap();

        let y = BigInt::from_hex(
            &"b868891567ca1ee8c44706c0dc190dd7779fe6f9b92ced909ad870800451e3".to_string(),
        )
        .unwrap();

        Secp256k1Point::from_coor(&x, &y); // x and y not of size 32 each

        let r = Secp256k1Point::random_point();
        let r_expected = Secp256k1Point::from_coor(&r.x_coor(), &r.y_coor());

        assert_eq!(r.x_coor(), r_expected.x_coor());
        assert_eq!(r.y_coor(), r_expected.y_coor());
    }

    #[test]
    fn deserialize_sk() {
        let s = "\"1e240\"";
        let dummy: Secp256k1Scalar = serde_json::from_str(s).expect("Failed in serialization");

        let sk: Secp256k1Scalar = ECScalar::from(&BigInt::from(123456));

        assert_eq!(dummy, sk);
    }

    #[test]
    fn serialize_pk() {
        let pk = Secp256k1Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        let expected =
            serde_json::to_string(&("0".to_string() + &pk.to_bigint().to_hex()))
                .expect("Failed in serialization");
        assert_eq!(s, expected);
        let des_pk: Secp256k1Point = serde_json::from_str(&s).expect("Failed in serialization");
        assert_eq!(des_pk.ge, pk.ge);
    }

    #[test]
    fn bincode_pk() {
        let pk = Secp256k1Point::generator();
        let bin = bincode::serialize(&pk).unwrap();
        let decoded: Secp256k1Point = bincode::deserialize(bin.as_slice()).unwrap();
        assert_eq!(decoded, pk);
    }

    #[test]
    fn test_serdes_pk() {
        let pk = Secp256k1Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        let des_pk: Secp256k1Point = serde_json::from_str(&s).expect("Failed in deserialization");
        assert_eq!(des_pk, pk);
    }

    #[test]
    #[should_panic]
    fn test_serdes_bad_pk() {
        let pk = Secp256k1Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        // we make sure that the string encodes invalid point:
        let s: String = s.replace("79be", "79bf");
        let des_pk: Secp256k1Point = serde_json::from_str(&s).expect("Failed in deserialization");
        assert_eq!(des_pk.ge, pk.ge);
    }

    #[test]
    #[should_panic]
    fn test_from_bytes() {
        let vec = BigInt::to_vec(&BigInt::from(1337));
        Secp256k1Point::from_bytes(&vec).unwrap();
    }

    #[test]
    fn test_from_bytes_3() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 1, 2, 3, 4, 5, 6,
        ];
        let result = Secp256k1Point::from_bytes(&test_vec);
        assert!(result.is_ok() | result.is_err())
    }

    #[test]
    fn test_from_bytes_4() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6,
        ];
        let result = Secp256k1Point::from_bytes(&test_vec);
        assert!(result.is_ok() | result.is_err())
    }

    #[test]
    fn test_from_bytes_5() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5,
            6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4,
            5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3,
            4, 5, 6,
        ];
        let result = Secp256k1Point::from_bytes(&test_vec);
        assert!(result.is_ok() | result.is_err())
    }

    #[test]
    fn test_minus_point() {
        let a: Secp256k1Scalar = ECScalar::new_random();
        let b: Secp256k1Scalar = ECScalar::new_random();
        let b_bn = b.to_bigint();
        let order = Secp256k1Scalar::q();
        let minus_b = BigInt::mod_sub(&order, &b_bn, &order);
        let a_minus_b = BigInt::mod_add(&a.to_bigint(), &minus_b, &order);
        let a_minus_b_fe: Secp256k1Scalar = ECScalar::from(&a_minus_b);
        let base: Secp256k1Point = ECPoint::generator();
        let point_ab1 = base.clone() * a_minus_b_fe;

        let point_a = base.clone() * a;
        let point_b = base.clone() * b;
        let point_ab2 = point_a.sub_point(&point_b.ge);
        assert_eq!(point_ab1.ge, point_ab2.ge);
    }

    #[test]
    fn test_invert() {
        let a: Secp256k1Scalar = ECScalar::new_random();
        let a_bn = a.to_bigint();
        let a_inv = a.invert();
        let a_inv_bn_1 = BigInt::mod_inv(&a_bn, &Secp256k1Scalar::q());
        let a_inv_bn_2 = a_inv.to_bigint();
        assert_eq!(a_inv_bn_1, a_inv_bn_2);
    }

    #[test]
    fn test_scalar_mul_scalar() {
        let a: Secp256k1Scalar = ECScalar::new_random();
        let b: Secp256k1Scalar = ECScalar::new_random();
        let c1 = a.mul(&b.fe);
        let c2 = a * b;
        assert_eq!(c1.fe, c2.fe);
    }

    #[test]
    fn test_pk_to_key_slice() {
        for _ in 1..200 {
            let r = Secp256k1Scalar::new_random();
            let rg = Secp256k1Point::generator() * &r;
            let key_slice = rg.to_vec();

            assert!(key_slice.len() == 65);
            assert!(key_slice[0].clone() == 4);

            let rg_prime: Secp256k1Point = ECPoint::from_bytes(&key_slice).unwrap();
            assert_eq!(rg_prime.ge, rg.ge);
        }
    }

    #[test]
    fn test_sign_ok() {
        let sk: Secp256k1Scalar = ECScalar::from(
            &BigInt::from_hex("4794853ce9e44b4c7a69c6a3b87db077f8f910f244bb6b966ba5fed83c9756f1")
                .unwrap(),
        );
        let msg_hash =
            BigInt::from_hex("100000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        let signature = sk.sign(&msg_hash);
        let r = signature.0;
        let s = signature.1;
        assert_eq!(
            r,
            BigInt::from_hex("46018c11152491be5d220ed8ca80a1631b8d12b2abb7a0e8bdc854466e5e1bf0")
                .unwrap()
        );
        assert_eq!(
            s,
            BigInt::from_hex("2c359e61f0a895ba6d922737e2e0d268e792e1cf756118d9377ddc96dd4fc5a9")
                .unwrap()
        );
    }

    #[test]
    fn test_sign_wrong_sk() {
        let sk: Secp256k1Scalar = ECScalar::from(
            &BigInt::from_hex("5794853ce9e44b4c7a69c6a3b87db077f8f910f244bb6b966ba5fed83c9756f1")
                .unwrap(),
        );
        let msg_hash =
            BigInt::from_hex("100000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        let signature = sk.sign(&msg_hash);
        let r = signature.0;
        let s = signature.1;
        assert_ne!(
            r,
            BigInt::from_hex("46018c11152491be5d220ed8ca80a1631b8d12b2abb7a0e8bdc854466e5e1bf0")
                .unwrap()
        );
        assert_ne!(
            s,
            BigInt::from_hex("2c359e61f0a895ba6d922737e2e0d268e792e1cf756118d9377ddc96dd4fc5a9")
                .unwrap()
        );
    }

    #[test]
    fn test_sign_wrong_hash() {
        let sk: Secp256k1Scalar = ECScalar::from(
            &BigInt::from_hex("4794853ce9e44b4c7a69c6a3b87db077f8f910f244bb6b966ba5fed83c9756f1")
                .unwrap(),
        );
        let msg_hash =
            BigInt::from_hex("110000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        let signature = sk.sign(&msg_hash);
        let r = signature.0;
        let s = signature.1;
        assert_ne!(
            r,
            BigInt::from_hex("46018c11152491be5d220ed8ca80a1631b8d12b2abb7a0e8bdc854466e5e1bf0")
                .unwrap()
        );
        assert_ne!(
            s,
            BigInt::from_hex("2c359e61f0a895ba6d922737e2e0d268e792e1cf756118d9377ddc96dd4fc5a9")
                .unwrap()
        );
    }
}
