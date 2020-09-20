// secp256k1 elliptic curve utility functions.
// based on MIT/Apache-licensed https://github.com/KZen-networks/curv/blob/master/src/elliptic/curves/secp256_k1.rs

use super::traits::{ECPoint, ECScalar};
use crate::ErrorKey;
use bigints::traits::{Converter, Modulo};
use bigints::BigInt;
use generic_array::typenum::U32;
use generic_array::GenericArray;
use getrandom::getrandom;
use k256::ecdsa::VerifyKey;
use k256::elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint};
use k256::{AffinePoint, EncodedPoint, NonZeroScalar, ProjectivePoint, Scalar, SecretKey};
#[cfg(feature = "num_bigint")]
use num_traits::Num;
use serde::de;
use serde::de::Visitor;
use serde::ser::{Serialize, Serializer};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::ops::{Add, Mul};
use std::ptr;
use std::sync::atomic;
use zeroize::Zeroize;

pub type SK = SecretKey;
pub type PK = VerifyKey;

#[derive(Clone, Debug)]
pub struct Secp256k1Scalar {
    purpose: &'static str,
    fe: SK,
}

#[derive(Clone, Debug)]
pub struct Secp256k1Point {
    purpose: &'static str,
    ge: PK,
}

pub type GE = Secp256k1Point;
pub type FE = Secp256k1Scalar;

impl Secp256k1Point {
    pub fn random_point() -> Secp256k1Point {
        let random_scalar: Secp256k1Scalar = Secp256k1Scalar::new_random();
        let base_point = Secp256k1Point::generator();
        let pk = base_point.scalar_mul(&random_scalar.get_element());
        Secp256k1Point {
            purpose: "random_point",
            ge: pk.get_element(),
        }
    }
}

impl Zeroize for Secp256k1Scalar {
    fn zeroize(&mut self) {
        unsafe { ptr::write_volatile(self, FE::zero()) };
        atomic::fence(atomic::Ordering::SeqCst);
        atomic::compiler_fence(atomic::Ordering::SeqCst);
    }
}

impl ECScalar<SK> for Secp256k1Scalar {
    fn new_random() -> Secp256k1Scalar {
        let mut arr = [0u8; 32];
        getrandom(&mut arr).unwrap();
        Secp256k1Scalar {
            purpose: "random",
            fe: SK::from_bytes(&arr).unwrap(),
        }
    }

    fn zero() -> Secp256k1Scalar {
        let zero_arr = [0u8; 32];
        let zero = unsafe { std::mem::transmute::<[u8; 32], SecretKey>(zero_arr) };
        Secp256k1Scalar {
            purpose: "zero",
            fe: zero,
        }
    }

    fn get_element(&self) -> SK {
        self.fe.clone()
    }

    fn set_element(&mut self, element: SK) {
        self.fe = element
    }

    fn from(n: &BigInt) -> Secp256k1Scalar {
        let curve_order = FE::q();
        let n_reduced = BigInt::mod_add(n, &BigInt::from(0), &curve_order);
        let mut v = BigInt::to_vec(&n_reduced);
        const SECRET_KEY_SIZE: usize = 32;

        if v.len() < SECRET_KEY_SIZE {
            let mut template = vec![0; SECRET_KEY_SIZE - v.len()];
            template.extend_from_slice(&v);
            v = template;
        }

        Secp256k1Scalar {
            purpose: "from_big_int",
            fe: SK::from_bytes(&v).unwrap(),
        }
    }

    fn to_big_int(&self) -> BigInt {
        BigInt::from_bytes(&self.fe.to_bytes())
    }

    fn q() -> BigInt {
        /// The order of the secp256k1 curve
        const CURVE_ORDER: [u8; 32] = [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xfe, 0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c,
            0xd0, 0x36, 0x41, 0x41,
        ];
        BigInt::from_bytes(&CURVE_ORDER.as_ref())
    }

    fn add(&self, other: &SK) -> Secp256k1Scalar {
        let scalar1 = Scalar::from_bytes_reduced(&self.get_element().to_bytes());
        let scalar2 = Scalar::from_bytes_reduced(&other.to_bytes());
        Secp256k1Scalar {
            purpose: "add",
            fe: SK::new(NonZeroScalar::new(scalar1 + scalar2).unwrap()),
        }
    }

    fn mul(&self, other: &SK) -> Secp256k1Scalar {
        let scalar1 = Scalar::from_bytes_reduced(&self.get_element().to_bytes());
        let scalar2 = Scalar::from_bytes_reduced(&other.to_bytes());
        Secp256k1Scalar {
            purpose: "mul",
            fe: SK::new(NonZeroScalar::new(scalar1 * scalar2).unwrap()),
        }
    }

    fn sub(&self, other: &SK) -> Secp256k1Scalar {
        let scalar1 = Scalar::from_bytes_reduced(&self.get_element().to_bytes());
        let scalar2 = Scalar::from_bytes_reduced(&other.to_bytes());
        Secp256k1Scalar {
            purpose: "sub",
            fe: SK::new(NonZeroScalar::new(scalar1 - scalar2).unwrap()),
        }
    }

    fn invert(&self) -> Secp256k1Scalar {
        let scalar = NonZeroScalar::new(
            Scalar::from_bytes_reduced(&self.get_element().to_bytes())
                .invert()
                .unwrap(),
        )
        .unwrap();
        Secp256k1Scalar {
            purpose: "invert",
            fe: SK::new(scalar),
        }
    }
}
impl Mul<Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn mul(self, other: Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).mul(&other.get_element())
    }
}

impl<'o> Mul<&'o Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn mul(self, other: &'o Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).mul(&other.get_element())
    }
}

impl Add<Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn add(self, other: Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).add(&other.get_element())
    }
}

impl<'o> Add<&'o Secp256k1Scalar> for Secp256k1Scalar {
    type Output = Secp256k1Scalar;
    fn add(self, other: &'o Secp256k1Scalar) -> Secp256k1Scalar {
        (&self).add(&other.get_element())
    }
}

impl Serialize for Secp256k1Scalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:0>64}", self.to_big_int().to_hex()))
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
        let v = BigInt::from_str_radix(s, 16).expect("Failed in serde");
        Ok(ECScalar::from(&v))
    }
}

impl PartialEq for Secp256k1Scalar {
    fn eq(&self, other: &Secp256k1Scalar) -> bool {
        self.get_element().to_bytes() == other.get_element().to_bytes()
    }
}

impl PartialEq for Secp256k1Point {
    fn eq(&self, other: &Secp256k1Point) -> bool {
        self.get_element() == other.get_element()
    }
}

impl Zeroize for Secp256k1Point {
    fn zeroize(&mut self) {
        unsafe { ptr::write_volatile(self, GE::generator()) };
        atomic::fence(atomic::Ordering::SeqCst);
        atomic::compiler_fence(atomic::Ordering::SeqCst);
    }
}

impl ECPoint<PK, SK> for Secp256k1Point {
    fn generator() -> Secp256k1Point {
        Secp256k1Point {
            purpose: "base_fe",
            ge: VerifyKey::from_encoded_point(&AffinePoint::generator().to_encoded_point(true))
                .unwrap(),
        }
    }

    fn get_element(&self) -> PK {
        self.ge.clone()
    }

    // to return from BigInt use from_bigint()
    fn bytes_compressed_to_big_int(&self) -> BigInt {
        BigInt::from_bytes(&self.ge.to_bytes())
    }

    fn x_coor(&self) -> Option<BigInt> {
        Some(BigInt::from_bytes(&EncodedPoint::from(&self.ge).x()))
    }

    fn y_coor(&self) -> Option<BigInt> {
        // need this back and forth conversion to get an uncompressed point
        let tmp = AffinePoint::from_encoded_point(&EncodedPoint::from(&self.ge)).unwrap();
        Some(BigInt::from_bytes(
            &tmp.to_encoded_point(false).y().unwrap(),
        ))
    }

    fn from_bytes(bytes: &[u8]) -> Result<Secp256k1Point, ErrorKey> {
        let result = PK::new(&bytes);
        let test = result.map(|pk| Secp256k1Point {
            purpose: "random",
            ge: pk,
        });
        test.map_err(|_err| ErrorKey::InvalidPublicKey)
    }

    fn pk_to_key_slice(&self) -> Vec<u8> {
        let tmp = AffinePoint::from_encoded_point(&EncodedPoint::from(&self.ge)).unwrap();
        tmp.to_encoded_point(false).as_ref().to_vec()
    }

    fn scalar_mul(&self, fe: &SK) -> Secp256k1Point {
        let point = ProjectivePoint::from(
            AffinePoint::from_encoded_point(&EncodedPoint::from(&self.ge)).unwrap(),
        );
        let scalar = Scalar::from_bytes_reduced(&fe.to_bytes());
        Secp256k1Point {
            purpose: "mul",
            ge: VerifyKey::from_encoded_point(&(point * scalar).to_affine().to_encoded_point(true))
                .unwrap(),
        }
    }

    fn add_point(&self, other: &PK) -> Secp256k1Point {
        let point1 = ProjectivePoint::from(
            AffinePoint::from_encoded_point(&EncodedPoint::from(&self.ge)).unwrap(),
        );
        let point2 = ProjectivePoint::from(
            AffinePoint::from_encoded_point(&EncodedPoint::from(other)).unwrap(),
        );
        Secp256k1Point {
            purpose: "combine",
            ge: VerifyKey::from_encoded_point(
                &(point1 + point2).to_affine().to_encoded_point(true),
            )
            .unwrap(),
        }
    }

    fn sub_point(&self, other: &PK) -> Secp256k1Point {
        let point1 = ProjectivePoint::from(
            AffinePoint::from_encoded_point(&EncodedPoint::from(&self.ge)).unwrap(),
        );
        let point2 = ProjectivePoint::from(
            AffinePoint::from_encoded_point(&EncodedPoint::from(other)).unwrap(),
        );
        Secp256k1Point {
            purpose: "sub",
            ge: VerifyKey::from_encoded_point(
                &(point1 - point2).to_affine().to_encoded_point(true),
            )
            .unwrap(),
        }
    }

    fn from_coor(x: &BigInt, y: &BigInt) -> Secp256k1Point {
        let mut vec_x = BigInt::to_vec(x);
        let mut vec_y = BigInt::to_vec(y);
        const COORDINATE_SIZE: usize = 32;
        assert!(vec_x.len() <= COORDINATE_SIZE, "x coordinate is too big.");
        assert!(vec_x.len() <= COORDINATE_SIZE, "y coordinate is too big.");
        if vec_x.len() < COORDINATE_SIZE {
            // pad
            let mut x_buffer = vec![0; COORDINATE_SIZE - vec_x.len()];
            x_buffer.extend_from_slice(&vec_x);
            vec_x = x_buffer
        }
        if vec_y.len() < COORDINATE_SIZE {
            // pad
            let mut y_buffer = vec![0; COORDINATE_SIZE - vec_y.len()];
            y_buffer.extend_from_slice(&vec_y);
            vec_y = y_buffer
        }

        let x_arr: GenericArray<u8, U32> = *GenericArray::from_slice(&vec_x);
        let y_arr: GenericArray<u8, U32> = *GenericArray::from_slice(&vec_y);
        Secp256k1Point {
            purpose: "base_fe",
            ge: VerifyKey::from_encoded_point(&EncodedPoint::from_affine_coordinates(
                &x_arr, &y_arr, false,
            ))
            .unwrap(),
        }
    }

    fn to_hex(&self) -> String {
        format!("{:0>66}", self.bytes_compressed_to_big_int().to_hex())
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

impl From<String> for Secp256k1Point {
    fn from(str_point: String) -> Self {
        let bigint = BigInt::from_str_radix(&str_point, 16).unwrap();
        Secp256k1Point::from_bigint(&bigint).unwrap()
    }
}

impl Into<String> for &Secp256k1Point {
    fn into(self) -> String {
        let bigint = self.bytes_compressed_to_big_int();
        let hex = bigint.to_str_radix(16);
        if hex.len() % 2 == 0 {
            hex
        } else {
            format!("0{}", &hex)
        }
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

impl Mul<Secp256k1Scalar> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn mul(self, other: Secp256k1Scalar) -> Self::Output {
        self.scalar_mul(&other.get_element())
    }
}

impl<'o> Mul<&'o Secp256k1Scalar> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn mul(self, other: &'o Secp256k1Scalar) -> Self::Output {
        self.scalar_mul(&other.get_element())
    }
}

impl<'o> Mul<&'o Secp256k1Scalar> for &'o Secp256k1Point {
    type Output = Secp256k1Point;
    fn mul(self, other: &'o Secp256k1Scalar) -> Self::Output {
        self.scalar_mul(&other.get_element())
    }
}

impl Add<Secp256k1Point> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn add(self, other: Secp256k1Point) -> Self::Output {
        self.add_point(&other.get_element())
    }
}

impl<'o> Add<&'o Secp256k1Point> for Secp256k1Point {
    type Output = Secp256k1Point;
    fn add(self, other: &'o Secp256k1Point) -> Self::Output {
        self.add_point(&other.get_element())
    }
}

impl<'o> Add<&'o Secp256k1Point> for &'o Secp256k1Point {
    type Output = Secp256k1Point;
    fn add(self, other: &'o Secp256k1Point) -> Self::Output {
        self.add_point(&other.get_element())
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
    use super::{BigInt, ErrorKey};
    #[cfg(not(feature = "wasm"))]
    use crate::curves::secp256_k1::{Secp256k1Point, Secp256k1Scalar};
    #[cfg(feature = "wasm")]
    use crate::curves::secp256_k1_rust::{Secp256k1Point, Secp256k1Scalar};
    use crate::curves::traits::{ECPoint, ECScalar};
    use bigints::traits::{Converter, Modulo};
    use bincode;
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
        let r_expected = Secp256k1Point::from_coor(&r.x_coor().unwrap(), &r.y_coor().unwrap());

        assert_eq!(r.x_coor().unwrap(), r_expected.x_coor().unwrap());
        assert_eq!(r.y_coor().unwrap(), r_expected.y_coor().unwrap());
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
            serde_json::to_string(&("0".to_string() + &pk.bytes_compressed_to_big_int().to_hex()))
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
        assert_eq!(des_pk.get_element(), pk.get_element());
    }

    #[test]
    fn test_from_bytes() {
        let vec = BigInt::to_vec(&BigInt::from(1337));
        let result = Secp256k1Point::from_bytes(&vec);
        assert_eq!(result.unwrap_err(), ErrorKey::InvalidPublicKey)
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
        let b_bn = b.to_big_int();
        let order = Secp256k1Scalar::q();
        let minus_b = BigInt::mod_sub(&order, &b_bn, &order);
        let a_minus_b = BigInt::mod_add(&a.to_big_int(), &minus_b, &order);
        let a_minus_b_fe: Secp256k1Scalar = ECScalar::from(&a_minus_b);
        let base: Secp256k1Point = ECPoint::generator();
        let point_ab1 = base.clone() * a_minus_b_fe;

        let point_a = base.clone() * a;
        let point_b = base.clone() * b;
        let point_ab2 = point_a.sub_point(&point_b.get_element());
        assert_eq!(point_ab1.get_element(), point_ab2.get_element());
    }

    #[test]
    fn test_invert() {
        let a: Secp256k1Scalar = ECScalar::new_random();
        let a_bn = a.to_big_int();
        let a_inv = a.invert();
        let a_inv_bn_1 = BigInt::mod_inv(&a_bn, &Secp256k1Scalar::q());
        let a_inv_bn_2 = a_inv.to_big_int();
        assert_eq!(a_inv_bn_1, a_inv_bn_2);
    }

    #[test]
    fn test_scalar_mul_scalar() {
        let a: Secp256k1Scalar = ECScalar::new_random();
        let b: Secp256k1Scalar = ECScalar::new_random();
        let c1 = a.mul(&b.get_element());
        let c2 = a * b;
        assert_eq!(c1.get_element().to_bytes(), c2.get_element().to_bytes());
    }

    #[test]
    fn test_pk_to_key_slice() {
        for _ in 1..200 {
            let r = Secp256k1Scalar::new_random();
            let rg = Secp256k1Point::generator() * &r;
            let key_slice = rg.pk_to_key_slice();

            assert!(key_slice.len() == 65);
            assert!(key_slice[0].clone() == 4);

            let rg_prime: Secp256k1Point = ECPoint::from_bytes(&key_slice).unwrap();
            assert_eq!(rg_prime.get_element(), rg.get_element());
        }
    }
}
