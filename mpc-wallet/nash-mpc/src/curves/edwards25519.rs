// ed25519 elliptic curve utility functions.

use super::traits::{ECPoint, ECScalar};
use curve25519_dalek::constants::{BASEPOINT_ORDER, ED25519_BASEPOINT_COMPRESSED};
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use getrandom::getrandom;
use num_traits::identities::{One, Zero};
use rust_bigint::traits::{Converter, Modulo};
use rust_bigint::BigInt;
use serde::de;
use serde::de::Visitor;
use serde::ser::{Serialize, Serializer};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::ops::{Add, Mul};
use std::ptr;
use std::sync::atomic;
use zeroize::Zeroize;

#[derive(Clone, Debug)]
pub struct Ed25519Scalar {
    purpose: &'static str,
    pub(crate) fe: Scalar,
}

#[derive(Clone, Debug, Copy)]
pub struct Ed25519Point {
    purpose: &'static str,
    pub(crate) ge: EdwardsPoint,
}

impl Zeroize for Ed25519Scalar {
    fn zeroize(&mut self) {
        self.fe.zeroize();
    }
}

impl ECScalar<Scalar> for Ed25519Scalar {
    fn new_random() -> Result<Ed25519Scalar, ()> {
        let mut arr = [0u8; 64];
        match getrandom(&mut arr) {
            Ok(_) => (),
            Err(_) => return Err(()),
        };
        let scalar = Scalar::from_bytes_mod_order_wide(&arr);
        if scalar == Scalar::zero() {
            return Err(());
        }
        Ok(Ed25519Scalar {
            purpose: "random",
            fe: scalar,
        })
    }

    fn from(n: &BigInt) -> Result<Ed25519Scalar, ()> {
        if n >= &Ed25519Scalar::q() || n < &BigInt::zero() {
            return Err(());
        }
        let mut vec = vec![0; 32 - BigInt::to_vec(n).len()];
        vec.extend(&BigInt::to_vec(n));
        let mut arr = [0u8; 32];
        // curve25519-dalek seems to expect reversed endianness
        let mut vec_rev = vec;
        vec_rev.reverse();
        arr.copy_from_slice(&vec_rev);
        Ok(Ed25519Scalar {
            purpose: "from_big_int",
            fe: Scalar::from_bits(arr),
        })
    }

    fn to_bigint(&self) -> BigInt {
        // curve25519-dalek seems to expect reversed endianness
        let mut bytes = *self.fe.as_bytes();
        bytes.reverse();
        BigInt::from_bytes(&bytes)
    }

    fn q() -> BigInt {
        // curve25519-dalek seems to expect reversed endianness
        let mut bytes = *BASEPOINT_ORDER.as_bytes();
        bytes.reverse();
        BigInt::from_bytes(&bytes)
    }

    fn add(&self, other: &Scalar) -> Result<Ed25519Scalar, ()> {
        let res = self.fe + other;
        if res == Scalar::zero() {
            return Err(());
        }
        Ok(Ed25519Scalar {
            purpose: "add",
            fe: res,
        })
    }

    fn mul(&self, other: &Scalar) -> Result<Ed25519Scalar, ()> {
        let res = self.fe * other;
        if res == Scalar::zero() {
            return Err(());
        }
        Ok(Ed25519Scalar {
            purpose: "add",
            fe: res,
        })
    }

    fn sub(&self, other: &Scalar) -> Result<Ed25519Scalar, ()> {
        let res = self.fe - other;
        if res == Scalar::zero() {
            return Err(());
        }
        Ok(Ed25519Scalar {
            purpose: "add",
            fe: res,
        })
    }

    fn invert(&self) -> Result<Ed25519Scalar, ()> {
        let res = self.fe.invert();
        if self.fe == Scalar::zero() || res == Scalar::zero() {
            return Err(());
        };
        Ok(Ed25519Scalar {
            purpose: "invert",
            fe: res,
        })
    }

    fn to_vec(&self) -> Vec<u8> {
        let vec: Vec<u8> = self.fe.to_bytes().iter().cloned().collect();
        vec
    }
}

impl Mul<Ed25519Scalar> for Ed25519Scalar {
    type Output = Result<Ed25519Scalar, ()>;
    fn mul(self, other: Ed25519Scalar) -> Result<Ed25519Scalar, ()> {
        (&self).mul(&other.fe)
    }
}

impl<'o> Mul<&'o Ed25519Scalar> for Ed25519Scalar {
    type Output = Result<Ed25519Scalar, ()>;
    fn mul(self, other: &'o Ed25519Scalar) -> Result<Ed25519Scalar, ()> {
        (&self).mul(&other.fe)
    }
}

impl Add<Ed25519Scalar> for Ed25519Scalar {
    type Output = Result<Ed25519Scalar, ()>;
    fn add(self, other: Ed25519Scalar) -> Result<Ed25519Scalar, ()> {
        (&self).add(&other.fe)
    }
}

impl<'o> Add<&'o Ed25519Scalar> for Ed25519Scalar {
    type Output = Result<Ed25519Scalar, ()>;
    fn add(self, other: &'o Ed25519Scalar) -> Result<Ed25519Scalar, ()> {
        (&self).add(&other.fe)
    }
}

impl Serialize for Ed25519Scalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:0>64}", self.to_bigint().to_hex()))
    }
}

impl<'de> Deserialize<'de> for Ed25519Scalar {
    fn deserialize<D>(deserializer: D) -> Result<Ed25519Scalar, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Ed25519ScalarVisitor)
    }
}

struct Ed25519ScalarVisitor;

impl<'de> Visitor<'de> for Ed25519ScalarVisitor {
    type Value = Ed25519Scalar;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Ed25519Scalar")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Ed25519Scalar, E> {
        let v = match BigInt::from_hex(&s) {
            Ok(v) => v,
            Err(_) => return Err(de::Error::custom(format!("Invalid hex string: {}", s))),
        };
        match ECScalar::from(&v) {
            Ok(v) => Ok(v),
            Err(_) => Err(de::Error::custom(format!("Invalid Ed25519Scalar: {}", s))),
        }
    }
}

impl PartialEq for Ed25519Scalar {
    fn eq(&self, other: &Ed25519Scalar) -> bool {
        self.fe == other.fe
    }
}

impl PartialEq for Ed25519Point {
    fn eq(&self, other: &Ed25519Point) -> bool {
        self.ge == other.ge
    }
}

impl Zeroize for Ed25519Point {
    fn zeroize(&mut self) {
        unsafe { ptr::write_volatile(self, Ed25519Point::generator()) };
        atomic::fence(atomic::Ordering::SeqCst);
        atomic::compiler_fence(atomic::Ordering::SeqCst);
    }
}

impl ECPoint<EdwardsPoint, Scalar> for Ed25519Point {
    fn generator() -> Ed25519Point {
        Ed25519Point {
            purpose: "base_fe",
            ge: ED25519_BASEPOINT_COMPRESSED.decompress().unwrap(),
        }
    }

    fn to_bigint(&self) -> BigInt {
        BigInt::from_bytes(self.ge.compress().as_bytes())
    }

    fn x_coor(&self) -> BigInt {
        // based on https://github.com/ZenGo-X/curv/blob/master/src/elliptic/curves/ed25519.rs#L584
        // helper function, based on https://ed25519.cr.yp.to/python/ed25519.py
        fn expmod(b: &BigInt, e: &BigInt, m: &BigInt) -> BigInt {
            if e.clone() == BigInt::zero() {
                return BigInt::one();
            };
            let t_temp = expmod(b, &(e.clone() / BigInt::from(2u32)), m);
            let mut t = BigInt::mod_pow(&t_temp, &BigInt::from(2u32), m);

            if e % &BigInt::from(2) != BigInt::zero() {
                t = BigInt::mod_mul(&t, b, m);
            }
            t
        }

        // based on https://github.com/ZenGo-X/curv/blob/master/src/elliptic/curves/ed25519.rs#L553
        // helper function, based on https://ed25519.cr.yp.to/python/ed25519.py
        let q = BigInt::from(2u32).pow(255u32) - BigInt::from(19u32);
        let d_n = -BigInt::from(121_665i32);
        let d_d = expmod(&BigInt::from(121_666), &(q.clone() - BigInt::from(2)), &q);

        let d_bn = d_n * d_d;
        let y_sqr = self.y_coor() * self.y_coor();
        let u = y_sqr.clone() - BigInt::one();
        let v = y_sqr * d_bn + BigInt::one();
        let v_inv = expmod(&v, &(q.clone() - BigInt::from(2)), &q);

        let x_sqr = u * v_inv;
        let q_plus_3_div_8 = (q.clone() + BigInt::from(3i32)) / BigInt::from(8i32);

        let mut x = expmod(&x_sqr, &q_plus_3_div_8, &q);
        if BigInt::mod_sub(&(x.clone() * x.clone()), &x_sqr, &q) != BigInt::zero() {
            let q_minus_1_div_4 = (q.clone() - BigInt::from(3i32)) / BigInt::from(4i32);
            let i = expmod(&BigInt::from(2i32), &q_minus_1_div_4, &q);
            x = BigInt::mod_mul(&x, &i, &q);
        }
        if &x % &BigInt::from(2i32) != BigInt::zero() {
            x = q - x.clone();
        }
        x
    }

    fn y_coor(&self) -> BigInt {
        // curve25519-dalek seems to expect reversed endianness
        let mut bytes = *self.ge.compress().as_bytes();
        bytes.reverse();
        BigInt::from_bytes(&bytes)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Ed25519Point, ()> {
        let point = CompressedEdwardsY::from_slice(&bytes);
        match point.decompress() {
            Some(v) => Ok(Ed25519Point {
                purpose: "random",
                ge: v,
            }),
            None => Err(()),
        }
    }

    fn to_vec(&self) -> Vec<u8> {
        self.ge.compress().as_bytes().to_vec()
    }

    fn scalar_mul(&self, fe: &Scalar) -> Result<Ed25519Point, ()> {
        Ok(Ed25519Point {
            purpose: "mul",
            ge: self.ge * fe,
        })
    }

    fn add_point(&self, other: &EdwardsPoint) -> Result<Ed25519Point, ()> {
        Ok(Ed25519Point {
            purpose: "combine",
            ge: self.ge + other,
        })
    }

    fn sub_point(&self, other: &EdwardsPoint) -> Result<Ed25519Point, ()> {
        Ok(Ed25519Point {
            purpose: "combine",
            ge: self.ge - other,
        })
    }

    fn from_coor(x: &BigInt, y: &BigInt) -> Result<Ed25519Point, ()> {
        let vec_y = BigInt::to_vec(y);
        let mut vec = vec![0; 32 - vec_y.len()];
        // pad with zeros if necessary
        vec.extend_from_slice(&vec_y);

        // curve25519-dalek seems to expect reversed endianness
        vec.reverse();

        match CompressedEdwardsY::from_slice(&vec).decompress() {
            Some(v) => Ok(Ed25519Point{
                purpose: "base_fe",
                ge: v,
            }),
            None => Err(()),
        }
    }

    fn to_hex(&self) -> String {
        format!("{:0>64}", self.to_bigint().to_hex())
    }

    fn from_hex(s: &str) -> Result<Ed25519Point, ()> {
        let v = match BigInt::from_hex(s) {
            Ok(v) => v,
            Err(_) => return Err(()),
        };
        Ed25519Point::from_bigint(&v)
    }
}

impl Ed25519Point {
    /// derive point from BigInt
    pub fn from_bigint(i: &BigInt) -> Result<Ed25519Point, ()> {
        match Ed25519Point::from_bytes(&BigInt::to_vec(i)) {
            Ok(v) => Ok(v),
            Err(_) => Err(()),
        }
    }
}

impl Mul<Ed25519Scalar> for Ed25519Point {
    type Output = Result<Ed25519Point, ()>;
    fn mul(self, other: Ed25519Scalar) -> Result<Ed25519Point, ()> {
        self.scalar_mul(&other.fe)
    }
}

impl<'o> Mul<&'o Ed25519Scalar> for Ed25519Point {
    type Output = Result<Ed25519Point, ()>;
    fn mul(self, other: &'o Ed25519Scalar) -> Result<Ed25519Point, ()> {
        self.scalar_mul(&other.fe)
    }
}

impl<'o> Mul<&'o Ed25519Scalar> for &'o Ed25519Point {
    type Output = Result<Ed25519Point, ()>;
    fn mul(self, other: &'o Ed25519Scalar) -> Result<Ed25519Point, ()> {
        self.scalar_mul(&other.fe)
    }
}

impl Add<Ed25519Point> for Ed25519Point {
    type Output = Result<Ed25519Point, ()>;
    fn add(self, other: Ed25519Point) -> Result<Ed25519Point, ()> {
        self.add_point(&other.ge)
    }
}

impl<'o> Add<&'o Ed25519Point> for Ed25519Point {
    type Output = Result<Ed25519Point, ()>;
    fn add(self, other: &'o Ed25519Point) -> Result<Ed25519Point, ()> {
        self.add_point(&other.ge)
    }
}

impl<'o> Add<&'o Ed25519Point> for &'o Ed25519Point {
    type Output = Result<Ed25519Point, ()>;
    fn add(self, other: &'o Ed25519Point) -> Result<Ed25519Point, ()> {
        self.add_point(&other.ge)
    }
}

impl Serialize for Ed25519Point {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Ed25519Point {
    fn deserialize<D>(deserializer: D) -> Result<Ed25519Point, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Ed25519PointVisitor)
    }
}

struct Ed25519PointVisitor;

impl<'de> Visitor<'de> for Ed25519PointVisitor {
    type Value = Ed25519Point;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Ed25519Point")
    }

    fn visit_str<E>(self, s: &str) -> Result<Ed25519Point, E>
    where
        E: de::Error,
    {
        match Ed25519Point::from_hex(&s.to_string()) {
            Ok(v) => Ok(v),
            Err(_) => Err(E::custom(format!(
                "Error deriving Ed25519Point from string: {}",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BigInt, Ed25519Point, Ed25519Scalar};
    use crate::curves::traits::{ECPoint, ECScalar};
    use bincode;
    use rust_bigint::traits::{Converter, Modulo};
    use serde_json;

    fn random_point() -> Ed25519Point {
        let random_scalar: Ed25519Scalar = Ed25519Scalar::new_random().unwrap();
        let base_point = Ed25519Point::generator();
        let pk = base_point.scalar_mul(&random_scalar.fe).unwrap();
        Ed25519Point {
            purpose: "random_point",
            ge: pk.ge,
        }
    }

    #[test]
    fn serialize_sk() {
        let scalar: Ed25519Scalar = ECScalar::from(&BigInt::from(123456)).unwrap();
        let s = serde_json::to_string(&scalar).expect("Failed in serialization");
        assert_eq!(
            s,
            "\"000000000000000000000000000000000000000000000000000000000001e240\""
        );
    }

    #[test]
    fn serialize_rand_pk_verify_pad() {
        let basepoint = Ed25519Point::generator();
        println!("basepoint x: {:?}", basepoint.x_coor().to_hex());
        println!("basepoint y: {:?}", basepoint.y_coor().to_hex());

        let vx = BigInt::from_hex(
            &"216936d3cd6e53fec0a4e231fdd6dc5c692cc7609525a7b2c9562d608f25d51a".to_string(),
        )
        .unwrap();

        let vy = BigInt::from_hex(
            &"6666666666666666666666666666666666666666666666666666666666666658".to_string(),
        )
        .unwrap();

        let point = Ed25519Point::from_coor(&vx, &vy).unwrap(); // x and y of size 32
        assert_eq!(point, Ed25519Point::generator());

        let x = BigInt::from_hex(
            &"5f6853305467a385b56a5d87f382abb52d10835a365ec265ce510e04b3c3366f".to_string(),
        )
        .unwrap();

        let y = BigInt::from_hex(
            &"b868891567ca1ee8c44706c0dc190dd7779fe6f9b92ced909ad870800451e3".to_string(),
        )
        .unwrap();

        Ed25519Point::from_coor(&x, &y).unwrap(); // x and y not of size 32 each

        let r = random_point();
        let r_expected = Ed25519Point::from_coor(&r.x_coor(), &r.y_coor()).unwrap();

        assert_eq!(r.x_coor(), r_expected.x_coor());
        assert_eq!(r.y_coor(), r_expected.y_coor());
    }

    #[test]
    fn deserialize_sk() {
        let s = "\"1e240\"";
        let dummy: Ed25519Scalar = serde_json::from_str(s).expect("Failed in serialization");
        let sk: Ed25519Scalar = ECScalar::from(&BigInt::from(123456)).unwrap();
        assert_eq!(dummy, sk);
    }

    #[test]
    fn serialize_pk() {
        let pk = Ed25519Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        let expected = serde_json::to_string(&(&pk.to_bigint().to_hex()))
            .expect("Failed in serialization");
        assert_eq!(s, expected);
        let des_pk: Ed25519Point = serde_json::from_str(&s).expect("Failed in serialization");
        assert_eq!(des_pk.ge, pk.ge);
    }

    #[test]
    fn bincode_pk() {
        let pk = Ed25519Point::generator();
        let bin = bincode::serialize(&pk).unwrap();
        let decoded: Ed25519Point = bincode::deserialize(bin.as_slice()).unwrap();
        assert_eq!(decoded, pk);
    }

    #[test]
    fn test_serdes_pk() {
        let pk = Ed25519Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        let des_pk: Ed25519Point = serde_json::from_str(&s).expect("Failed in deserialization");
        assert_eq!(des_pk, pk);
    }

    #[test]
    #[should_panic]
    fn test_serdes_bad_pk() {
        let pk = Ed25519Point::generator();
        let s = serde_json::to_string(&pk).expect("Failed in serialization");
        // we make sure that the string encodes invalid point:
        let s: String = s.replace("5866", "6977");
        let des_pk: Ed25519Point = serde_json::from_str(&s).expect("Failed in deserialization");
        assert_eq!(des_pk.ge, pk.ge);
    }

    #[test]
    #[should_panic]
    fn test_from_bytes() {
        let vec = BigInt::to_vec(&BigInt::from(1337));
        Ed25519Point::from_bytes(&vec).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_from_bytes_3() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 1, 2, 3, 4, 5, 6,
        ];
        Ed25519Point::from_bytes(&test_vec).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_from_bytes_4() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6,
        ];
        Ed25519Point::from_bytes(&test_vec).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_from_bytes_5() {
        let test_vec = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5,
            6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4,
            5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3,
            4, 5, 6,
        ];
        Ed25519Point::from_bytes(&test_vec).unwrap();
    }

    #[test]
    fn test_minus_point() {
        let a: Ed25519Scalar = ECScalar::new_random().unwrap();
        let b: Ed25519Scalar = ECScalar::new_random().unwrap();
        let b_bn = b.to_bigint();
        let order = Ed25519Scalar::q();
        let minus_b = BigInt::mod_sub(&order, &b_bn, &order);
        let a_minus_b = BigInt::mod_add(&a.to_bigint(), &minus_b, &order);
        let a_minus_b_fe: Ed25519Scalar = ECScalar::from(&a_minus_b).unwrap();
        let base: Ed25519Point = ECPoint::generator();
        let point_ab1 = (base.clone() * a_minus_b_fe).unwrap();

        let point_a = (base.clone() * a).unwrap();
        let point_b = (base.clone() * b).unwrap();
        let point_ab2 = point_a.sub_point(&point_b.ge).unwrap();
        assert_eq!(point_ab1.ge, point_ab2.ge);
    }

    #[test]
    fn test_invert() {
        let a: Ed25519Scalar = ECScalar::new_random().unwrap();
        let a_bn = a.to_bigint();
        let a_inv = a.invert().unwrap();
        let a_inv_bn_1 = BigInt::mod_inv(&a_bn, &Ed25519Scalar::q());
        let a_inv_bn_2 = a_inv.to_bigint();
        assert_eq!(a_inv_bn_1, a_inv_bn_2);
    }

    #[test]
    fn test_scalar_mul() {
        let g = Ed25519Point::generator();
        let a: Ed25519Scalar = ECScalar::from(&BigInt::from(2)).unwrap();
        let expected = Ed25519Point::from_hex(
            "c9a3f86aae465f0e56513864510f3997561fa2c9e85ea21dc2292309f3cd6022",
        )
        .unwrap();
        assert_eq!((g * a).unwrap(), expected);
    }

    #[test]
    fn test_scalar_mul_scalar() {
        let a: Ed25519Scalar = ECScalar::new_random().unwrap();
        let b: Ed25519Scalar = ECScalar::new_random().unwrap();
        let c1 = a.mul(&b.fe).unwrap();
        let c2 = (a * b).unwrap();
        assert_eq!(c1.fe, c2.fe);
    }

    #[test]
    fn test_pk_to_key_slice() {
        for _ in 1..200 {
            let r = Ed25519Scalar::new_random().unwrap();
            let rg = (Ed25519Point::generator() * &r).unwrap();
            let key_slice = rg.to_vec();
            assert!(key_slice.len() == 32);
            let rg_prime: Ed25519Point = ECPoint::from_bytes(&key_slice).unwrap();
            assert_eq!(rg_prime.ge, rg.ge);
        }
    }
}
