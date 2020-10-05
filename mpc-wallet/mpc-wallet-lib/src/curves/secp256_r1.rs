// NIST P-256/secp256r1 elliptic curve utility functions.

use super::traits::{ECPoint, ECScalar};
use amcl::nist256::big::{BIG, MODBYTES};
use amcl::nist256::ecp::ECP;
use amcl::nist256::fp::FP;
use amcl::nist256::rom::CURVE_ORDER;
use rust_bigint::traits::Converter;
use rust_bigint::BigInt;
use getrandom::getrandom;
#[cfg(feature = "num_bigint")]
use num_traits::Num;
use serde::de;
use serde::de::Visitor;
use serde::ser::{Serialize, Serializer};
use serde::{Deserialize, Deserializer};
use std::ops::{Add, Mul, Sub};
use std::sync::atomic;
use std::{fmt, ptr};
use zeroize::Zeroize;

#[derive(Clone, Debug, PartialEq)]
pub struct Secp256r1Scalar {
    purpose: &'static str,
    pub(crate) fe: FP,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Secp256r1Point {
    purpose: &'static str,
    pub(crate) ge: ECP,
}

impl Zeroize for Secp256r1Scalar {
    fn zeroize(&mut self) {
        let zero = Secp256r1Scalar {
            purpose: "zero",
            fe: FP::new(),
        };
        unsafe { ptr::write_volatile(self, zero) };
        atomic::fence(atomic::Ordering::SeqCst);
        atomic::compiler_fence(atomic::Ordering::SeqCst);
    }
}

impl ECScalar<FP> for Secp256r1Scalar {
    fn new_random() -> Result<Secp256r1Scalar, ()> {
        let mut rand_arr = [0u8; 32];
        match getrandom(&mut rand_arr) {
            Ok(_) => (),
            Err(_) => return Err(()),
        };
        let mut fp = FP::new();
        fp.x = BIG::frombytes(&rand_arr);
        Ok(Secp256r1Scalar {
            purpose: "random",
            fe: fp,
        })
    }

    fn from(n: &BigInt) -> Result<Secp256r1Scalar, ()> {
        Ok(Secp256r1Scalar {
            purpose: "from_big_int",
            fe: FP::from_hex(format!("1 {}", n.to_hex())),
        })
    }

    fn to_bigint(&self) -> BigInt {
        BigInt::from_hex(&self.fe.clone().x.tostring()).unwrap()
    }

    fn q() -> BigInt {
        let q = BIG::new_ints(&CURVE_ORDER).to_hex();
        BigInt::from_hex(&q).unwrap()
    }

    fn add(&self, other: &FP) -> Result<Secp256r1Scalar, ()> {
        let mut scalar = self.fe.clone();
        scalar.add(&other);
        Ok(Secp256r1Scalar {
            purpose: "add",
            fe: scalar,
        })
    }

    fn mul(&self, other: &FP) -> Result<Secp256r1Scalar, ()> {
        let mut scalar = FP::new();
        scalar.x = BIG::modmul(
            &self.fe.x,
            &other.x,
            &BIG::new_ints(&CURVE_ORDER),
        );
        Ok(Secp256r1Scalar {
            purpose: "mul",
            fe: scalar,
        })
    }

    fn sub(&self, other: &FP) -> Result<Secp256r1Scalar, ()> {
        let mut scalar = self.fe.clone();
        scalar.sub(&other);
        Ok(Secp256r1Scalar {
            purpose: "sub",
            fe: scalar,
        })
    }

    fn invert(&self) -> Result<Secp256r1Scalar, ()> {
        let mut big = self.fe.clone().x;
        big.invmodp(&BIG::new_ints(&CURVE_ORDER));
        let mut fp = FP::new();
        fp.x = big;
        Ok(Secp256r1Scalar {
            purpose: "invert",
            fe: fp,
        })
    }

    /// convert to vector and pad with zeros if necessary
    fn to_vec(&self) -> Vec<u8> {
        let vec = BigInt::to_vec(&self.to_bigint());
        let mut v = vec![0; MODBYTES - vec.len()];
        v.extend(&vec);
        v
    }
}

impl Mul<Secp256r1Scalar> for Secp256r1Scalar {
    type Output = Result<Secp256r1Scalar, ()>;
    fn mul(self, other: Secp256r1Scalar) -> Result<Secp256r1Scalar, ()> {
        (&self).mul(&other.fe)
    }
}

impl<'o> Mul<&'o Secp256r1Scalar> for Secp256r1Scalar {
    type Output = Result<Secp256r1Scalar, ()>;
    fn mul(self, other: &'o Secp256r1Scalar) -> Result<Secp256r1Scalar, ()> {
        (&self).mul(&other.fe)
    }
}

impl Add<Secp256r1Scalar> for Secp256r1Scalar {
    type Output = Result<Secp256r1Scalar, ()>;
    fn add(self, other: Secp256r1Scalar) -> Result<Secp256r1Scalar, ()> {
        (&self).add(&other.fe)
    }
}

impl<'o> Add<&'o Secp256r1Scalar> for Secp256r1Scalar {
    type Output = Result<Secp256r1Scalar, ()>;
    fn add(self, other: &'o Secp256r1Scalar) -> Result<Secp256r1Scalar, ()> {
        (&self).add(&other.fe)
    }
}

impl Sub<Secp256r1Scalar> for Secp256r1Scalar {
    type Output = Result<Secp256r1Scalar, ()>;
    fn sub(self, other: Secp256r1Scalar) -> Result<Secp256r1Scalar, ()> {
        (&self).sub(&other.fe)
    }
}

impl<'o> Sub<&'o Secp256r1Scalar> for Secp256r1Scalar {
    type Output = Result<Secp256r1Scalar, ()>;
    fn sub(self, other: &'o Secp256r1Scalar) -> Result<Secp256r1Scalar, ()> {
        (&self).sub(&other.fe)
    }
}

impl Serialize for Secp256r1Scalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:0>64}", self.to_bigint().to_hex()))
    }
}

impl<'de> Deserialize<'de> for Secp256r1Scalar {
    fn deserialize<D>(deserializer: D) -> Result<Secp256r1Scalar, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Secp256r1ScalarVisitor)
    }
}

struct Secp256r1ScalarVisitor;

impl<'de> Visitor<'de> for Secp256r1ScalarVisitor {
    type Value = Secp256r1Scalar;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Secp256r1Scalar")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Secp256r1Scalar, E> {
        let v = match BigInt::from_hex(&s) {
            Ok(v) => v,
            Err(_) => return Err(de::Error::custom(format!("Invalid hex string: {}", s))),
        };
        match ECScalar::from(&v) {
            Ok(v) => Ok(v),
            Err(_) => return Err(de::Error::custom(format!("Invalid Secp256r1Scalar: {}", s))),
        }
    }
}

impl ECPoint<ECP, FP> for Secp256r1Point {
    fn generator() -> Secp256r1Point {
        Secp256r1Point {
            purpose: "base_fe",
            ge: ECP::generator(),
        }
    }

    fn to_bigint(&self) -> BigInt {
        let mut b: [u8; MODBYTES as usize + 1] = [0; MODBYTES as usize + 1];
        self.ge.tobytes(&mut b, true);
        BigInt::from_bytes(&b)
    }

    fn x_coor(&self) -> BigInt {
        BigInt::from_hex(&self.ge.getx().tostring()).unwrap()
    }

    fn y_coor(&self) -> BigInt {
        BigInt::from_hex(&self.ge.gety().tostring()).unwrap()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Secp256r1Point, ()> {
        if bytes.len() != MODBYTES as usize + 1 && bytes.len() != 2 * MODBYTES as usize + 1 {
            return Err(());
        }
        let point = Secp256r1Point {
            purpose: "random",
            ge: ECP::frombytes(&bytes),
        };
        // verify that public key is valid
        if point.ge == ECP::new() {
            return Err(());
        }
        Ok(point)
    }

    fn to_vec(&self) -> Vec<u8> {
        let mut b: [u8; 2 * MODBYTES as usize + 1] = [0; 2 * MODBYTES as usize + 1];
        self.ge.tobytes(&mut b, false);
        b.to_vec()
    }

    fn scalar_mul(&self, fe: &FP) -> Result<Secp256r1Point, ()> {
        Ok(Secp256r1Point {
            purpose: "mul",
            ge: self.ge.mul(&fe.x),
        })
    }

    fn add_point(&self, other: &ECP) -> Result<Secp256r1Point, ()> {
        let mut point = self.ge.clone();
        point.add(other);
        Ok(Secp256r1Point {
            purpose: "combine",
            ge: point,
        })
    }

    fn sub_point(&self, other: &ECP) -> Result<Secp256r1Point, ()> {
        let mut point = self.ge.clone();
        point.sub(other);
        Ok(Secp256r1Point {
            purpose: "sub",
            ge: point,
        })
    }

    fn from_coor(x: &BigInt, y: &BigInt) -> Result<Secp256r1Point, ()> {
        let ix = BIG::from_hex(x.to_hex());
        let iy = BIG::from_hex(y.to_hex());
        Ok(Secp256r1Point {
            purpose: "base_fe",
            ge: ECP::new_bigs(&ix, &iy),
        })
    }

    fn to_hex(&self) -> String {
        format!("{:0>66}", self.to_bigint().to_hex())
    }

    fn from_hex(s: &str) -> Result<Secp256r1Point, ()> {
        let v = match BigInt::from_hex(s) {
            Ok(v) => v,
            Err(_) => return Err(()),
        };
        let point = match Secp256r1Point::from_bigint(&v) {
            Ok(v) => v,
            Err(_) => return Err(()),
        };
        Ok(point)
    }
}

impl Secp256r1Point {
    // derive point from BigInt
    pub fn from_bigint(i: &BigInt) -> Result<Secp256r1Point, ()> {
        let vec = BigInt::to_vec(i);
        let point = ECP::frombytes(&vec);
        // check if point is valid
        if point == ECP::new() {
            return Err(());
        }
        Ok(Secp256r1Point {
            purpose: "from_bigint",
            ge: point,
        })
    }
}

impl Mul<Secp256r1Scalar> for Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn mul(self, other: Secp256r1Scalar) -> Result<Secp256r1Point, ()> {
        self.scalar_mul(&other.fe)
    }
}

impl<'o> Mul<&'o Secp256r1Scalar> for Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn mul(self, other: &'o Secp256r1Scalar) -> Result<Secp256r1Point, ()> {
        self.scalar_mul(&other.fe)
    }
}

impl<'o> Mul<&'o Secp256r1Scalar> for &'o Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn mul(self, other: &'o Secp256r1Scalar) -> Result<Secp256r1Point, ()> {
        self.scalar_mul(&other.fe)
    }
}

impl Add<Secp256r1Point> for Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn add(self, other: Secp256r1Point) -> Result<Secp256r1Point, ()> {
        self.add_point(&other.ge)
    }
}

impl<'o> Add<&'o Secp256r1Point> for Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn add(self, other: &'o Secp256r1Point) -> Result<Secp256r1Point, ()> {
        self.add_point(&other.ge)
    }
}

impl<'o> Add<&'o Secp256r1Point> for &'o Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn add(self, other: &'o Secp256r1Point) -> Result<Secp256r1Point, ()> {
        self.add_point(&other.ge)
    }
}

impl Sub<Secp256r1Point> for Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn sub(self, other: Secp256r1Point) -> Result<Secp256r1Point, ()> {
        self.sub_point(&other.ge)
    }
}

impl<'o> Sub<&'o Secp256r1Point> for Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn sub(self, other: &'o Secp256r1Point) -> Result<Secp256r1Point, ()> {
        self.sub_point(&other.ge)
    }
}

impl<'o> Sub<&'o Secp256r1Point> for &'o Secp256r1Point {
    type Output = Result<Secp256r1Point, ()>;
    fn sub(self, other: &'o Secp256r1Point) -> Result<Secp256r1Point, ()> {
        self.sub_point(&other.ge)
    }
}

impl Serialize for Secp256r1Point {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Secp256r1Point {
    fn deserialize<D>(deserializer: D) -> Result<Secp256r1Point, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Secp256r1PointVisitor)
    }
}

struct Secp256r1PointVisitor;

impl<'de> Visitor<'de> for Secp256r1PointVisitor {
    type Value = Secp256r1Point;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Secp256r1Point")
    }

    fn visit_str<E>(self, s: &str) -> Result<Secp256r1Point, E>
    where
        E: de::Error,
    {
        match Secp256r1Point::from_hex(&s.to_string()) {
            Ok(v) => Ok(v),
            Err(_) => Err(E::custom(format!(
                "Error deriving Secp256r1Point from string: {}",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BigInt, Secp256r1Point, Secp256r1Scalar};
    use crate::curves::traits::{ECPoint, ECScalar};
    use rust_bigint::traits::{Converter, Modulo, Samplable};

    fn base_point2() -> Secp256r1Point {
        let random_scalar: Secp256r1Scalar = Secp256r1Scalar::new_random().unwrap();
        let base_point = Secp256r1Point::generator();
        let pk = base_point.scalar_mul(&random_scalar.fe).unwrap();
        Secp256r1Point {
            purpose: "base_fe",
            ge: pk.ge,
        }
    }

    fn random_point() -> Secp256r1Point {
        let random_scalar: Secp256r1Scalar = Secp256r1Scalar::new_random().unwrap();
        let pk = Secp256r1Point::generator().scalar_mul(&random_scalar.fe).unwrap();
        Secp256r1Point {
            purpose: "random_point",
            ge: pk.ge,
        }
    }

    #[test]
    fn serialize_sk() {
        let scalar: Secp256r1Scalar = ECScalar::from(&BigInt::from(123456)).unwrap();
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

        Secp256r1Point::from_coor(&vx, &vy).unwrap(); // x and y of size 32

        let x = BigInt::from_hex(
            &"5f6853305467a385b56a5d87f382abb52d10835a365ec265ce510e04b3c3366f".to_string(),
        )
        .unwrap();

        let y = BigInt::from_hex(
            &"b868891567ca1ee8c44706c0dc190dd7779fe6f9b92ced909ad870800451e3".to_string(),
        )
        .unwrap();

        Secp256r1Point::from_coor(&x, &y).unwrap(); // x and y not of size 32 each

        let r = random_point();
        let r_expected = Secp256r1Point::from_coor(&r.x_coor(), &r.y_coor()).unwrap();

        assert_eq!(r.x_coor(), r_expected.x_coor());
        assert_eq!(r.y_coor(), r_expected.y_coor());
    }

    #[test]
    fn deserialize_sk() {
        let s = "\"1e240\"";
        let dummy: Secp256r1Scalar = serde_json::from_str(s).expect("Failed in serialization");

        let sk: Secp256r1Scalar = ECScalar::from(&BigInt::from(123456)).unwrap();

        assert_eq!(dummy, sk);
    }

    #[test]
    fn serialize_pk() {
        let pk = Secp256r1Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        let expected = pk.to_bigint().to_hex();
        assert_eq!(
            s,
            serde_json::to_string(&("0".to_string() + &expected)).unwrap()
        );
        let des_pk: Secp256r1Point = serde_json::from_str(&s).expect("Failed in serialization");
        assert_eq!(des_pk.ge, pk.ge);
    }

    #[test]
    fn bincode_pk() {
        let pk = Secp256r1Point::generator();
        let bin = bincode::serialize(&pk).unwrap();
        let decoded: Secp256r1Point = bincode::deserialize(bin.as_slice()).unwrap();
        assert_eq!(decoded.ge, pk.ge);
    }

    #[test]
    fn test_serdes_pk() {
        let pk = Secp256r1Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        let des_pk: Secp256r1Point = serde_json::from_str(&s).expect("Failed in deserialization");
        assert_eq!(des_pk.ge, pk.ge);

        let pk = base_point2();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        let des_pk: Secp256r1Point = serde_json::from_str(&s).expect("Failed in deserialization");
        assert_eq!(des_pk.ge, pk.ge);
    }

    #[test]
    #[should_panic]
    fn test_serdes_bad_pk() {
        let pk = Secp256r1Point::generator();
        let mut s = serde_json::to_string(&pk).expect("Failed in serialization");
        // we make sure that the string encodes invalid point:
        s = s.replace("2770", "2780");
        let des_pk: Secp256r1Point = serde_json::from_str(&s).expect("Failed in deserialization");
        assert_eq!(des_pk, pk);
    }

    #[test]
    #[should_panic]
    fn test_from_bytes() {
        let vec = BigInt::to_vec(&BigInt::from(1337));
        Secp256r1Point::from_bytes(&vec).unwrap();
    }

    #[test]
    fn test_from_bytes_3() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 1, 2, 3, 4, 5, 6,
        ];
        let result = Secp256r1Point::from_bytes(&test_vec);
        assert!(result.is_ok() | result.is_err())
    }

    #[test]
    fn test_from_bytes_4() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6,
        ];
        let result = Secp256r1Point::from_bytes(&test_vec);
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
        let result = Secp256r1Point::from_bytes(&test_vec);
        assert!(result.is_ok() | result.is_err())
    }

    #[test]
    fn test_add_sub() {
        let q = Secp256r1Scalar::q();
        let start: Secp256r1Scalar = ECScalar::new_random().unwrap();
        let b: Secp256r1Scalar = ECScalar::new_random().unwrap();
        let tmp = BigInt::mod_add(&start.to_bigint(), &b.to_bigint(), &q);
        let end = BigInt::mod_sub(&tmp, &b.to_bigint(), &q);
        assert_eq!(start.to_bigint(), end);
    }

    #[test]
    fn test_minus_point() {
        let a: Secp256r1Scalar = ECScalar::new_random().unwrap();
        let b: Secp256r1Scalar = ECScalar::new_random().unwrap();
        let b_bn = b.to_bigint();
        let q = Secp256r1Scalar::q();
        let minus_b = BigInt::mod_sub(&q, &b_bn, &q);
        let a_minus_b = BigInt::mod_add(&a.to_bigint(), &minus_b, &q);
        let a_minus_b_fe: Secp256r1Scalar = ECScalar::from(&a_minus_b).unwrap();
        let base: Secp256r1Point = ECPoint::generator();
        let point_ab1 = (base.clone() * a_minus_b_fe).unwrap();
        let point_a = (base.clone() * a).unwrap();
        let point_b = (base.clone() * b).unwrap();
        let point_ab2 = point_a.sub_point(&point_b.ge).unwrap();
        assert_eq!(point_ab1.ge, point_ab2.ge);
    }

    #[test]
    fn test_simple_inversion2() {
        let a: Secp256r1Scalar = ECScalar::from(&BigInt::from(2)).unwrap();
        let a_inv = a.invert().unwrap();
        let a_inv_int = a_inv.to_bigint();
        assert_eq!(
            a_inv_int,
            BigInt::from_hex("7fffffff800000007fffffffffffffffde737d56d38bcf4279dce5617e3192a9")
                .unwrap()
        );
    }

    #[test]
    fn test_simple_inversion3() {
        let a: Secp256r1Scalar = ECScalar::from(&BigInt::from(1234567890)).unwrap();
        let a_inv = a.invert().unwrap().to_bigint();
        assert_eq!(
            a_inv,
            BigInt::from_hex("93a24a3b7e3b3a49a5acf862e8360bdd456e4c095dec9b97772bb758f725715a")
                .unwrap()
        );
    }

    #[test]
    fn test_invert() {
        let a_bn = BigInt::sample(256);
        let a: Secp256r1Scalar = ECScalar::from(&a_bn).unwrap();
        let a_inv = a.invert().unwrap();
        let a_inv_bn_1 = BigInt::mod_inv(&a_bn, &Secp256r1Scalar::q());
        let a_inv_bn_2 = a_inv.to_bigint();
        assert_eq!(a_inv_bn_1, a_inv_bn_2);
    }

    #[test]
    fn test_scalar_mul_scalar() {
        let a: Secp256r1Scalar = ECScalar::new_random().unwrap();
        let b: Secp256r1Scalar = ECScalar::new_random().unwrap();
        let c1 = a.mul(&b.fe).unwrap();
        let c2 = (a * b).unwrap();
        assert_eq!(c1.fe, c2.fe);
    }

    #[test]
    fn test_scalar_mul1() {
        let base_point = Secp256r1Point::generator();
        let int: Secp256r1Scalar = ECScalar::from(&BigInt::from(1)).unwrap();
        let test = (base_point * int).unwrap();
        assert_eq!(
            test.ge.getx().to_hex(),
            "6B17D1F2E12C4247F8BCE6E563A440F277037D812DEB33A0F4A13945D898C296"
        );
        assert_eq!(
            test.ge.gety().to_hex(),
            "4FE342E2FE1A7F9B8EE7EB4A7C0F9E162BCE33576B315ECECBB6406837BF51F5"
        );
    }

    #[test]
    fn test_scalar_mul2() {
        let base_point = Secp256r1Point::generator();
        let int: Secp256r1Scalar = ECScalar::from(&BigInt::from(2)).unwrap();
        let test = (base_point * int).unwrap();
        assert_eq!(
            test.ge.getx().to_hex(),
            "7CF27B188D034F7E8A52380304B51AC3C08969E277F21B35A60B48FC47669978"
        );
        assert_eq!(
            test.ge.gety().to_hex(),
            "07775510DB8ED040293D9AC69F7430DBBA7DADE63CE982299E04B79D227873D1"
        );
    }

    #[test]
    fn test_scalar_mul3() {
        let base_point = Secp256r1Point::generator();
        let int: Secp256r1Scalar = ECScalar::from(
            &BigInt::from_hex("7CF27B188D034F7E8A52380304B51AC3C08969E277F21B35A60B48FC47669978")
                .unwrap(),
        ).unwrap();
        let test = (base_point * int).unwrap();
        assert_eq!(
            test.ge.getx().to_hex(),
            "4F6DD42033C0666A04DFC107F4CB4D5D22E33AE178006803D967CB25D95B7DB4"
        );
        assert_eq!(
            test.ge.gety().to_hex(),
            "085DB1B0952D8E081A3E13398A89911A038AAB054AE3E26718A5E582ED9FDD38"
        );
    }

    #[test]
    fn test_pk_to_key_slice() {
        for _ in 1..200 {
            let r = Secp256r1Scalar::new_random().unwrap();
            let rg = (Secp256r1Point::generator() * &r).unwrap();
            let key_slice = rg.to_vec();
            assert!(key_slice.len() == 65);
            assert!(key_slice[0].clone() == 4);
            let rg_prime: Secp256r1Point = ECPoint::from_bytes(&key_slice).unwrap();
            assert_eq!(rg_prime.ge, rg.ge);
        }
    }

    #[test]
    fn scalar_bigint_conversion1() {
        let int = BigInt::sample(256);
        let scalar: Secp256r1Scalar = ECScalar::from(&int).unwrap();
        assert_eq!(scalar.to_bigint(), int);
    }

    #[test]
    fn point_bigint_conversion1() {
        let g = Secp256r1Point::generator();
        let h = g.to_bigint();
        let i = Secp256r1Point::from_bigint(&h).unwrap();
        assert_eq!(i.ge, g.ge);
    }

    #[test]
    fn point_bigint_conversion2() {
        let g = Secp256r1Point::generator();
        let r: Secp256r1Scalar = ECScalar::from(&BigInt::sample(256)).unwrap();
        let point = (g * r).unwrap();
        let point_int = point.to_bigint();
        let point_test = Secp256r1Point::from_bigint(&point_int).unwrap();
        assert_eq!(point.ge, point_test.ge);
    }

    #[test]
    fn scalar_bigint_conversion2() {
        let i = Secp256r1Scalar::new_random().unwrap();
        let int = i.to_bigint();
        let j: Secp256r1Scalar = ECScalar::from(&int).unwrap();
        assert_eq!(i.fe, j.fe);
    }

    #[test]
    fn pk_to_hex() {
        let secret =
            BigInt::from_hex("79196b247effbe3192763a5c37b18f5d89e7d0a8c83d246917add0a842d5af8b")
                .unwrap();
        let sk: Secp256r1Scalar = ECScalar::from(&secret).unwrap();
        let g = Secp256r1Point::generator();
        let h = (g * sk).unwrap();
        assert_eq!(
            h.to_hex(),
            "025c31225f77535b1ceb7f603ef73627bf096a1efb65c1fdf0f7c1c9d64cf167ca"
        );
    }

    #[test]
    fn scalar_from_bigint() {
        let r = Secp256r1Scalar::new_random().unwrap();
        let int = r.to_bigint();
        let s: Secp256r1Scalar = ECScalar::from(&int).unwrap();
        assert_eq!(r.fe, s.fe);
    }

    #[test]
    fn add_sub_point() {
        let g = Secp256r1Point::generator();
        let i: Secp256r1Scalar = ECScalar::from(&BigInt::from(3)).unwrap();
        assert_eq!(((g + g).unwrap() + g).unwrap().ge, (g * i).unwrap().ge);
        assert_eq!((g + g).unwrap().ge, (((g + g).unwrap() - g).unwrap() + g).unwrap().ge);
    }

    #[test]
    fn add_scalar() {
        let i: Secp256r1Scalar = ECScalar::from(&BigInt::from(1)).unwrap();
        let j: Secp256r1Scalar = ECScalar::from(&BigInt::from(2)).unwrap();
        assert_eq!((i.clone() + i.clone()).unwrap().fe, j.fe);
        assert_eq!(
            (((i.clone() + i.clone()).unwrap() + i.clone()).unwrap() + i.clone()).unwrap().fe,
            (j.clone() + j.clone()).unwrap().fe
        );
    }

    #[test]
    fn sub_scalar() {
        let i: Secp256r1Scalar = ECScalar::from(&BigInt::from(1)).unwrap();
        assert_eq!(
            ((i.clone() + i.clone()).unwrap() - i.clone()).unwrap().fe,
            i.fe
        );
        let j: Secp256r1Scalar = ECScalar::from(&BigInt::from(2)).unwrap();
        assert_eq!(
            ((j.clone() + j.clone()).unwrap() - j.clone()).unwrap().fe,
            j.fe
        );
        let k = Secp256r1Scalar::new_random().unwrap();
        assert_eq!(
            ((k.clone() + k.clone()).unwrap() - k.clone()).unwrap().fe,
            k.fe
        );
    }

    #[test]
    fn mul_scalar() {
        let i: Secp256r1Scalar = ECScalar::from(&BigInt::from(1)).unwrap();
        let j: Secp256r1Scalar = ECScalar::from(&BigInt::from(2)).unwrap();
        assert_eq!((j.clone() * i.clone()).unwrap().fe, j.fe);
    }
}
