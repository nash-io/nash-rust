/*
 * Functions for MPC-based API keys used by both server and client
 */

use crate::curves::curve25519::{Ed25519Point, Ed25519Scalar, EdwardsPoint};
#[cfg(feature = "secp256k1")]
use crate::curves::secp256_k1::{Secp256k1Point, Secp256k1Scalar};
#[cfg(feature = "k256")]
use crate::curves::secp256_k1_rust::{Secp256k1Point, Secp256k1Scalar};
use crate::curves::secp256_r1::{Secp256r1Point, Secp256r1Scalar};
use crate::curves::traits::{ECPoint, ECScalar};
use crate::NashMPCError;
#[cfg(feature = "k256")]
use k256::elliptic_curve::sec1::{
    FromEncodedPoint as FromEncodedPoint_k256, ToEncodedPoint as ToEncodedPoint_k256,
};
#[cfg(feature = "k256")]
use k256::AffinePoint as AffinePoint_k256;
use lazy_static::__Deref;
#[cfg(feature = "num_bigint")]
use num_integer::Integer;
#[cfg(feature = "num_bigint")]
use num_traits::Zero;
#[cfg(feature = "secp256k1")]
use p256::elliptic_curve::sec1::{
    FromEncodedPoint as FromEncodedPoint_p256, ToEncodedPoint as ToEncodedPoint_p256,
};
use p256::{AffinePoint as AffinePoint_p256, EncodedPoint as EncodedPoint_p256};
use rust_bigint::traits::{Converter, NumberTests};
use rust_bigint::BigInt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::convert::TryInto;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

/// paillier key size is 2048 bit (minimum recommended key length as of 02/2020)
pub const PAILLIER_KEY_SIZE: usize = 2048;

/// supported elliptic curves
#[derive(Copy, Clone, Serialize, Deserialize, PartialEq)]
pub enum Curve {
    Secp256k1,
    Secp256r1,
    Curve25519,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CorrectKeyProof {
    #[serde(with = "rust_bigint::serialize::vecbigint512")]
    pub sigma_vec: Vec<BigInt>,
}

/// m = 11 (i.e., number of messages to be transferred); seems reasonable for our use case (smaller m -> too low security, greater m -> too much bandwidth)
pub(crate) const CORRECT_KEY_M: usize = 11;

/// Diffie-Hellman: create a set of secret values and a set of public values (using curve secp256r1)
pub fn dh_init_secp256r1(
    n: usize,
) -> Result<(Vec<Secp256r1Scalar>, Vec<Secp256r1Point>), NashMPCError> {
    // don't allow creating too many values at once.
    if n > 100 {
        return Err(NashMPCError::IntegerInvalid);
    }
    let base: Secp256r1Point = ECPoint::generator();
    let mut dh_secrets: Vec<Secp256r1Scalar> = Vec::new();
    let mut dh_publics: Vec<Secp256r1Point> = Vec::new();
    for _ in 0..n {
        let dh_secret = Secp256r1Scalar::new_random()?;
        let dh_public = (&base * &dh_secret)?;
        dh_secrets.push(dh_secret);
        dh_publics.push(dh_public);
    }
    Ok((dh_secrets, dh_publics))
}

/// Diffie-Hellman: create a set of secret values and a set of public values (using curve secp256k1)
pub fn dh_init_secp256k1(
    n: usize,
) -> Result<(Vec<Secp256k1Scalar>, Vec<Secp256k1Point>), NashMPCError> {
    // don't allow creating too many values at once.
    if n > 100 {
        return Err(NashMPCError::IntegerInvalid);
    }
    let base: Secp256k1Point = ECPoint::generator();
    let mut dh_secrets: Vec<Secp256k1Scalar> = Vec::new();
    let mut dh_publics: Vec<Secp256k1Point> = Vec::new();
    for _ in 0..n {
        let dh_secret = Secp256k1Scalar::new_random()?;
        let dh_public = (&base * &dh_secret)?;
        dh_secrets.push(dh_secret);
        dh_publics.push(dh_public);
    }
    Ok((dh_secrets, dh_publics))
}

/// Diffie-Hellman: create a set of secret values and a set of public values (using curve25519)
pub fn dh_init_curve25519(
    n: usize,
) -> Result<(Vec<Ed25519Scalar>, Vec<Ed25519Point>), NashMPCError> {
    // don't allow creating too many values at once.
    if n > 100 {
        return Err(NashMPCError::IntegerInvalid);
    }
    let base: Ed25519Point = ECPoint::generator();
    let mut dh_secrets: Vec<Ed25519Scalar> = Vec::new();
    let mut dh_publics: Vec<Ed25519Point> = Vec::new();
    for _ in 0..n {
        let dh_secret = Ed25519Scalar::new_random()?;
        let dh_public = (base * &dh_secret)?;
        dh_secrets.push(dh_secret);
        dh_publics.push(dh_public);
    }
    Ok((dh_secrets, dh_publics))
}

/// verify an ECDSA or EdDSA signature. full message in case of EdDSA/Ed25519 and hash of a message otherwise.
pub fn verify(r: &BigInt, s: &BigInt, pubkey_str: &str, msg: &BigInt, curve: Curve) -> bool {
    // we want to hide the actual error and only return true | false
    verify_wrapper(r, s, pubkey_str, msg, curve).is_ok()
}

fn verify_wrapper(
    r: &BigInt,
    s: &BigInt,
    pubkey_str: &str,
    msg: &BigInt,
    curve: Curve,
) -> Result<(), NashMPCError> {
    if curve == Curve::Secp256k1 {
        // convert pubkey string format as used by ME to bigint
        let pk_int = BigInt::from_hex(pubkey_str)?;
        let pk = Secp256k1Point::from_bytes(&BigInt::to_vec(&pk_int))?;
        let s_fe: Secp256k1Scalar = ECScalar::from(&s)?;
        let rx_fe: Secp256k1Scalar = ECScalar::from(&r)?;
        let s_inv_fe = s_fe.invert()?;
        let e_fe: Secp256k1Scalar = ECScalar::from(&msg.mod_floor(&Secp256k1Scalar::q()))?;
        let u1_ = (Secp256k1Point::generator() * &e_fe)?;
        let u1 = (u1_ * &s_inv_fe)?;
        let u2_ = (pk * &rx_fe)?;
        let u2 = (u2_ * &s_inv_fe)?;
        let u1_plus_u2 = (u1 + u2)?;
        let rx_bytes = &BigInt::to_vec(&r)[..];
        // second condition is against malleability
        if rx_bytes
            .ct_eq(&BigInt::to_vec(&u1_plus_u2.x_coor())[..])
            .unwrap_u8()
            == 1
            && s < &(Secp256k1Scalar::q() - s.clone())
        {
            Ok(())
        } else {
            Err(NashMPCError::SignatureVerification)
        }
    } else if curve == Curve::Secp256r1 {
        // convert pubkey string format as used by ME to bigint
        let pk_int = BigInt::from_hex(pubkey_str)?;
        let pk = Secp256r1Point::from_bytes(&BigInt::to_vec(&pk_int))?;
        let s_fe: Secp256r1Scalar = ECScalar::from(&s)?;
        let rx_fe: Secp256r1Scalar = ECScalar::from(&r)?;
        let s_inv_fe = s_fe.invert()?;
        let e_fe: Secp256r1Scalar = ECScalar::from(&msg.mod_floor(&Secp256r1Scalar::q()))?;
        let u1_ = (Secp256r1Point::generator() * &e_fe)?;
        let u1 = (u1_ * &s_inv_fe)?;
        let u2_ = (pk * &rx_fe)?;
        let u2 = (u2_ * &s_inv_fe)?;
        let u1_plus_u2 = (u1 + u2)?;
        let rx_bytes = &BigInt::to_vec(&r)[..];
        // second condition is against malleability
        if rx_bytes
            .ct_eq(&BigInt::to_vec(&u1_plus_u2.x_coor())[..])
            .unwrap_u8()
            == 1
            && s < &(Secp256r1Scalar::q() - s.clone())
        {
            Ok(())
        } else {
            Err(NashMPCError::SignatureVerification)
        }
    // verify EdDSA signature
    // strict version based on https://docs.rs/ed25519-dalek/1.0.1/src/ed25519_dalek/public.rs.html#283
    } else if curve == Curve::Curve25519 {
        let r_point = Ed25519Point::from_bigint(r)?;
        let s_bytes: [u8; 32] = match s.to_bytes().try_into() {
            Ok(v) => v,
            Err(_) => return Err(NashMPCError::IntegerInvalid),
        };
        let s_ = Ed25519Scalar::from_bytes32(s_bytes)?;
        let pk = Ed25519Point::from_hex(pubkey_str)?;
        if r_point.ge.is_small_order() || pk.ge.is_small_order() {
            return Err(NashMPCError::SignatureVerification);
        };
        let hash: Ed25519Scalar = eddsa_s_hash(&r_point, &pk, msg)?;
        if r_point.ge
            == EdwardsPoint::vartime_double_scalar_mul_basepoint(&hash.fe, &(-&pk).ge, &s_.fe)
        {
            Ok(())
        } else {
            Err(NashMPCError::SignatureVerification)
        }
    } else {
        Err(NashMPCError::CurveInvalid)
    }
}

/// compute the hash-part of S in an EdDSA signature
pub(crate) fn eddsa_s_hash(
    r_point: &Ed25519Point,
    pk: &Ed25519Point,
    msg: &BigInt,
) -> Result<Ed25519Scalar, NashMPCError> {
    let mut h = Sha512::new();
    h.update(&r_point.ge.compress().as_bytes());
    h.update(&pk.ge.compress().as_bytes());
    h.update(&msg.to_bytes());
    // unwrap is safe since sha512 output is always 64 byte
    let hash_bytes: [u8; 64] = h.finalize().as_slice().try_into().unwrap();
    let hash = Ed25519Scalar::from_bytes64(&hash_bytes)?;
    // check for hash == 0 as that might (theoretically) allow a brute-force attack on the secret key by the server
    if hash.to_bigint() == BigInt::zero() {
        return Err(NashMPCError::ScalarInvalid);
    }
    Ok(hash)
}

/// derive EdDSA signing key from secret key.
/// the secret key is hashed with a 512-bit hash function (SHA-512 in case of EdDSA/ed25519)
/// and the output is split in half. The first half is the signing key (after some bit fiddling)
/// and we ignore the second half (which is usually used for generating nonces).
pub(crate) fn eddsa_signingkey_from_secretkey(
    secret_key_int: &BigInt,
) -> Result<Ed25519Scalar, NashMPCError> {
    let mut signing_key_bytes: [u8; 32] = [0u8; 32];
    signing_key_bytes.copy_from_slice(&Sha512::digest(&secret_key_int.to_bytes())[0..32]);
    // bit fiddling
    signing_key_bytes[0] &= 0b11111000;
    signing_key_bytes[31] &= 0b00111111;
    signing_key_bytes[31] |= 0b01000000;
    Ed25519Scalar::from_bytes32(signing_key_bytes)
}

#[cfg(feature = "secp256k1")]
fn publickey_from_secretkey_k1(pk: &Secp256k1Point) -> String {
    "0".to_string() + &BigInt::from_bytes(&pk.ge.serialize_uncompressed()).to_hex()
}
#[cfg(feature = "k256")]
fn publickey_from_secretkey_k1(pk: &Secp256k1Point) -> String {
    // unwrap() is safe because pk has been validated in publickey_from_secretkey()
    let tmp = AffinePoint_k256::from_encoded_point(&pk.ge.to_encoded_point(false)).unwrap();
    "0".to_string() + &BigInt::from_bytes(&tmp.to_encoded_point(false).as_bytes()).to_hex()
}

/// derive public key from secret key, in uncompressed format as expected by ME
pub fn publickey_from_secretkey(
    secret_key_int: &BigInt,
    curve: Curve,
) -> Result<String, NashMPCError> {
    if curve == Curve::Secp256k1 {
        let secret_key = match ECScalar::from(secret_key_int) {
            Ok(v) => Zeroizing::<Secp256k1Scalar>::new(v),
            Err(_) => return Err(NashMPCError::IntegerInvalid),
        };
        let base: Secp256k1Point = ECPoint::generator();
        let pk = (base * secret_key.deref())?;
        Ok(publickey_from_secretkey_k1(&pk))
    } else if curve == Curve::Secp256r1 {
        let secret_key = match ECScalar::from(secret_key_int) {
            Ok(v) => Zeroizing::<Secp256r1Scalar>::new(v),
            Err(_) => return Err(NashMPCError::IntegerInvalid),
        };
        let pk = (Secp256r1Point::generator() * secret_key.deref())?;
        let tmp = AffinePoint_p256::from_encoded_point(&EncodedPoint_p256::from(&pk.ge));
        if tmp.is_none() {
            return Err(NashMPCError::PointInvalid);
        }
        // add leading zeros if necessary
        Ok(format!(
            "{:0>130}",
            BigInt::from_bytes(&tmp.unwrap().to_encoded_point(false).as_bytes()).to_hex()
        ))
    } else if curve == Curve::Curve25519 {
        let signing_key = match eddsa_signingkey_from_secretkey(secret_key_int) {
            Ok(v) => Zeroizing::<Ed25519Scalar>::new(v),
            Err(_) => return Err(NashMPCError::IntegerInvalid),
        };
        let pk = (Ed25519Point::generator() * signing_key.deref())?;
        // add leading zeros if necessary
        Ok(format!("{:0>64}", pk.to_hex()))
    } else {
        Err(NashMPCError::CurveInvalid)
    }
}

/// this is an implementation of the getRho() function from the paper "Efficient Noninteractive Certification of RSA Moduli and Beyond" by Goldberg et al. 2019 (Appendix C.4)
/// parameters alpha = 6370, m = 11, kappa = 139 seem reasonable for our use case.
/// the bigger alpha the more primes need to be checked during verification -> slower computation
/// the smaller alpha the more messages need to be transferred -> higher bandwidth
pub(crate) fn correct_key_proof_rho(n: &BigInt) -> Vec<BigInt> {
    // use sha256 because it is used already in other parts of the code
    const HASH_SIZE: usize = 256;
    // use a 256 bit salt (i.e., "nash" as input to sha256)
    let salt: String = format!(
        "{:0>64}",
        create_hash(&BigInt::from_bytes(b"nash")).to_hex()
    );

    let mut rho_vec: Vec<BigInt> = Vec::new();
    let iterations = NumberTests::bits(n) / HASH_SIZE;
    for i in 0..CORRECT_KEY_M {
        let mut counter: usize = 0;
        loop {
            let rho = create_hash(
                &BigInt::from_hex(
                    &(i2osp(&n.to_hex())
                        + &salt.clone()
                        + &i2osp(&i.to_string())
                        + &i2osp(&counter.to_string())),
                )
                .unwrap(),
            );
            if iterations < 1 {
                rho_vec.push(rho);
                break;
            }

            // extend rho by using a mask generation function (which is similar to a hash function but with variable-sized output)
            let mut rho_full = "".to_string();
            for i in 0..iterations {
                rho_full += &format!(
                    "{:0>64}",
                    create_hash(
                        &BigInt::from_hex(&(i2osp(&rho.to_hex()) + &i2osp(&i.to_string())))
                            .unwrap()
                    )
                    .to_hex()
                );
            }
            let rho_full_int = BigInt::from_hex(&rho_full).unwrap();
            // increase counter and retry until result is in Z_n
            if &rho_full_int < n {
                rho_vec.push(rho_full_int);
                break;
            }
            counter += 1;
        }
    }
    rho_vec
}

/// add one leading zero to string if necessary
/// "Efficient Noninteractive Certification of RSA Moduli and Beyond" by Goldberg et al. 2019 suggests using I2OSP from https://tools.ietf.org/html/rfc8017#section-4.1
/// and our .tostring() and .to_hex() implementations may miss one zero
fn i2osp(input: &str) -> String {
    let width = if input.len() % 2 == 0 {
        input.len()
    } else {
        input.len() + 1
    };
    format!("{:0>width$}", input, width = width)
}

/// sha256 hash
fn create_hash(int: &BigInt) -> BigInt {
    let hash = Sha256::digest(&BigInt::to_vec(int));
    BigInt::from_bytes(&hash[..])
}

#[cfg(test)]
mod tests {
    use crate::common::{
        correct_key_proof_rho, create_hash, dh_init_curve25519, dh_init_secp256k1,
        dh_init_secp256r1, i2osp, publickey_from_secretkey, verify, Curve,
    };
    #[cfg(feature = "secp256k1")]
    use crate::curves::secp256_k1::Secp256k1Point;
    #[cfg(feature = "k256")]
    use crate::curves::secp256_k1_rust::Secp256k1Point;
    use crate::curves::secp256_r1::Secp256r1Point;
    use crate::curves::traits::ECPoint;
    #[cfg(feature = "num_bigint")]
    use num_traits::Num;
    use rust_bigint::traits::Converter;
    use rust_bigint::BigInt;

    #[test]
    // Test Vectors taken from:
    // https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/secure-hashing#shavs
    fn vector_sha256_test() {
        // 256 bit message
        let result: BigInt = create_hash(
            &BigInt::from_str_radix(
                "09fc1accc230a205e4a208e64a8f204291f581a12756392da4b8c0cf5ef02b95",
                16,
            )
            .unwrap(),
        );
        assert_eq!(
            result.to_str_radix(16),
            "4f44c1c7fbebb6f9601829f3897bfd650c56fa07844be76489076356ac1886a4"
        );

        // 512 bit message
        let result: BigInt = create_hash(&BigInt::from_str_radix("5a86b737eaea8ee976a0a24da63e7ed7eefad18a101c1211e2b3650c5187c2a8a650547208251f6d4237e661c7bf4c77f335390394c37fa1a9f9be836ac28509", 16).unwrap());
        assert_eq!(
            result.to_str_radix(16),
            "42e61e174fbb3897d6dd6cef3dd2802fe67b331953b06114a65c772859dfc1aa"
        );
    }

    #[test]
    fn test_correct_key_proof_rho() {
        let correct_key_proof = correct_key_proof_rho(&BigInt::from_hex("d3542d07cda6034cf8568b68d69f07b716c98dcc466d7fb89d2a40db4addfe1402ac6007b609734c80fa4dd24f005cc2d404651f724561391fd2c714c054c5ecb98c0d367d5d99cddbd788489151daa247feef546ba173db02576793f2386c89a78e1cf0b1b5e3882efb709663c8fb50f3b973e87447bc0a473b792eeb9720ef").unwrap());
        let correct_key_proof_expected = [BigInt::from_hex("c884ea8da3eeecd7f746280d3d7a41475363ab1a5b6365ee286a0b8feab4ca956f2943bc115b7ae6f6b63671096c8ef29a84c422822797a3a444c95f32804fd6c4a027233a7bd6373734fb3694b1065d371286fd41c7193ec8783eb87f9a28b280af8714523beec16dc50713a3795ec03ac6ca6954b68da88daa166cca8e19b9").unwrap(), BigInt::from_hex("2faa32f93fbfa718fdbf9eb4cf0dce0411bae771fe7b4032af740cc6e30298aadbe1074859e6ec9fe2e3566e6432956edfd33ed7f9e5b7471e74ca5e314d04908e4dc5eba260c004f674cd8a13aad7b027dd0b030f6ca595fe8316881c1b6cc0122848622b16871fb932cb0a62cb91ccd9bd6d1945f4102a5c8ee6aee3b8197d").unwrap(), BigInt::from_hex("3f8133d100e0da7ef31d8df80abcfd0824d04e48d7212de0863cb5b69fe6258ec5cf59b81130207cc0b60b15fc0b8a1051ed2d2f2227bac91cb568a47fc687f5e1f1c1b63067a7e73dc2e1b492d221b62f17d76c9aafd00e342614aac031f034969d2be93c6e24863e29d81df7455c398e06297d67e1e81791006d8b41952174").unwrap(), BigInt::from_hex("38039e49805c8c2fdbe7790ee249713268495904fb6a0c3bc7165ab10395b0d51317bf41aa60fd9eafd371b0233fba993551e89dca4c494d0391c53dc18703b23e09753d1fb04f5cc9a98c82e3f5c463311fa3b307ad721f36b6abd962c7b257f5a14958e0620bf00d456065c99b07cacc0508a779dd8ba8eb890cf1d1ca2048").unwrap(), BigInt::from_hex("b024da2148818d0f51c89ce0e94d012ab44c8f39d1b81d5ae266438b136f443480bdba62a2b12382ff5efc61a68e1678a3c5c5476d07038290f8c219e610593e7c078285915afb7283e9b1ad8be6be11f90de1dba1fd713fbaca8346ec3b2e365f77e118a6f650d749d1a5f16a0f33146f1807013d2d5cd37ded7ab8d426cbf5").unwrap(), BigInt::from_hex("5d9aef465046f0ac0c3e713484db75e5e9f688523f276c7796f8c38aa0b9291c2dbad40d9450d14982f5e32a05e759a55b689ea37e7300b1f220cf66d3e80d08db209ec9ac46833e85136ff52c36745bf34ab4c02b8344914582108371813694ca2a375d9c04d27dad3b98ae49f5fcdd8d920db1381e088d913bde60133436a0").unwrap(), BigInt::from_hex("66dba6ba189b9b729b7dcf39e1ad9e3e8d3f5aac56416a2fd34f2b7dfc8d1b751ec22291f93093d4d5cdc6d69b85ee189e4135a3d57c8349b8cf3a4cf9e953f3da87f3025c4237fdcf5b379fc107d9c0d3ddd70e7d566ab80a7d52602f480a6d163a9d2ee7108e1f2b20d5913746e46b211e2db560764cf7c5f7eef751627e00").unwrap(), BigInt::from_hex("6ea4f72fad257733622614f693da76bfc4716c69e53d02ae46a73a33b961d37d2b36ef71ce1c05fa588919dc0897f05d9564cf7a115c861fc9e0effb5df662d4a71af7896a8085dea2769ffa44b9128c43a13c83279c2c30ebcd0c5aa0326be31537bba7f17ff05a76e1d0287bc8f8590a10977e3b2a4c9acbf14f375ce61d3c").unwrap(), BigInt::from_hex("5ba6a506d2cb8100ee64cd1ec14022c1785ba4d4c3f27c077331e940a435e926f1ca6e3999e1b44969a8e9613338090212ec46bd3425737e64e8ee45268630badecbb83a8415396f7d05061541c1ca04bd0bfa5504f7b09d584c8efca2f80b4eb125016752f88c784bbbf0227d470ec59496d7c4e95bd01f04dc880c7e894c8").unwrap(), BigInt::from_hex("1e3e4d09d5024dfe628d930c45c2bb70cbdb3260f3c32495c5c01c46c92209b75188bcbb43285f5b75f286e1b745d28c62b52696a3aa8c4bc2c03a616f04603c6eca074a4a1b62c87a9941080d14171daa2e8d3b7b845eafb786646a5c851a7a805c020eabdacf7d52724e645425bb0b41db34601e22fd8c2f4a09bbaf2598fc").unwrap(), BigInt::from_hex("986aa4c80d6dd3881d7f9e8d49e9896958ca3feb4d9786d789ac833f359ee8302807d38f353bb0cadfb209607fac5c2b944180fb2b399682b2d7e8b8d229596fa090d8c46dc9734269179604b75d258ce846970eb43f9a67403d9986c86cc4bbff0d0ef234d8b5681ddac681b25612b15a69ef5a73c38a6b4226f1e6ed3fce7d").unwrap()];
        assert_eq!(correct_key_proof, correct_key_proof_expected);
    }

    #[test]
    fn test_i2osp() {
        assert_eq!(BigInt::from(1).to_hex().len(), 1);
        assert_eq!(i2osp(&BigInt::from(1).to_hex()).len(), 2);
        assert_eq!(i2osp(&BigInt::from(255).to_hex()).len(), 2);
        assert_eq!(i2osp(&BigInt::from(256).to_hex()).len(), 4);
    }

    #[test]
    fn test_dh_init_k1() {
        let (secret1, public1) = dh_init_secp256k1(1).unwrap();
        let (secret2, public2) = dh_init_secp256k1(1).unwrap();
        assert_ne!(secret1, secret2);
        assert_ne!(public1, public2);
    }

    #[test]
    fn test_dh_init_r1() {
        let (secret1, public1) = dh_init_secp256r1(1).unwrap();
        let (secret2, public2) = dh_init_secp256r1(1).unwrap();
        assert_ne!(secret1, secret2);
        assert_ne!(public1, public2);
    }

    #[test]
    fn test_dh_init_ed() {
        let (secret1, public1) = dh_init_curve25519(1).unwrap();
        let (secret2, public2) = dh_init_curve25519(1).unwrap();
        assert_ne!(secret1, secret2);
        assert_ne!(public1, public2);
    }

    #[test]
    #[should_panic]
    fn test_dh_init_k1_fail() {
        dh_init_secp256k1(101).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_dh_init_r1_fail() {
        dh_init_secp256r1(101).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_dh_init_ed_fail() {
        dh_init_curve25519(101).unwrap();
    }

    #[test]
    fn test_verify_k1_ok() {
        let r =
            BigInt::from_hex("23d42030c7a114b26e020de776265b7260b8b4606779f1930d938080c804d3e2")
                .unwrap();
        let s =
            BigInt::from_hex("7a99d4297ec6ea427dae9fdf326a5fb8b2fdc5a0c98e7be4c784a22b7aa9807b")
                .unwrap();
        let pubkey = Secp256k1Point::from_coor(
            &BigInt::from_hex("697bc1ff06e124e7ca8b72c554ffa5b1950aa7f2e7e7ab23c0da63dee5c90a72")
                .unwrap(),
            &BigInt::from_hex("721c7dde8df4a6d06c2bea91dc6e9c075c3c35926d73f891788b9ae681b7eed5")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("0000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256k1
        ));
    }

    #[test]
    fn test_verify_k1_wrong_sig_r() {
        let r =
            BigInt::from_hex("33d42030c7a114b26e020de776265b7260b8b4606779f1930d938080c804d3e2")
                .unwrap();
        let s =
            BigInt::from_hex("7a99d4297ec6ea427dae9fdf326a5fb8b2fdc5a0c98e7be4c784a22b7aa9807b")
                .unwrap();
        let pubkey = Secp256k1Point::from_coor(
            &BigInt::from_hex("697bc1ff06e124e7ca8b72c554ffa5b1950aa7f2e7e7ab23c0da63dee5c90a72")
                .unwrap(),
            &BigInt::from_hex("721c7dde8df4a6d06c2bea91dc6e9c075c3c35926d73f891788b9ae681b7eed5")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("0000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256k1
        ));
    }

    #[test]
    fn test_verify_k1_wrong_sig_s() {
        let r =
            BigInt::from_hex("23d42030c7a114b26e020de776265b7260b8b4606779f1930d938080c804d3e2")
                .unwrap();
        let s =
            BigInt::from_hex("8a99d4297ec6ea427dae9fdf326a5fb8b2fdc5a0c98e7be4c784a22b7aa9807b")
                .unwrap();
        let pubkey = Secp256k1Point::from_coor(
            &BigInt::from_hex("697bc1ff06e124e7ca8b72c554ffa5b1950aa7f2e7e7ab23c0da63dee5c90a72")
                .unwrap(),
            &BigInt::from_hex("721c7dde8df4a6d06c2bea91dc6e9c075c3c35926d73f891788b9ae681b7eed5")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("0000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256k1
        ));
    }

    #[test]
    fn test_verify_k1_wrong_pk() {
        let r =
            BigInt::from_hex("23d42030c7a114b26e020de776265b7260b8b4606779f1930d938080c804d3e2")
                .unwrap();
        let s =
            BigInt::from_hex("7a99d4297ec6ea427dae9fdf326a5fb8b2fdc5a0c98e7be4c784a22b7aa9807b")
                .unwrap();
        let pubkey = Secp256k1Point::from_coor(
            &BigInt::from_hex("61646fdc4544f10294e20e994ce56d8c0ff852596eb6b3aa0ba9d4b2079d86d4")
                .unwrap(),
            &BigInt::from_hex("2b3b5e8491a48ff6e1620732579807916eeb07beb6f9970dc5952bd444404f74")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("0000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256k1
        ));
    }

    #[test]
    #[should_panic]
    fn test_verify_k1_invalid_pk() {
        let r =
            BigInt::from_hex("23d42030c7a114b26e020de776265b7260b8b4606779f1930d938080c804d3e2")
                .unwrap();
        let s =
            BigInt::from_hex("7a99d4297ec6ea427dae9fdf326a5fb8b2fdc5a0c98e7be4c784a22b7aa9807b")
                .unwrap();
        let msg_hash =
            BigInt::from_hex("0000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(verify(
            &r,
            &s,
            &"1234567890".to_string(),
            &msg_hash,
            Curve::Secp256k1
        ));
    }

    #[test]
    fn test_verify_k1_wrong_msg() {
        let r =
            BigInt::from_hex("23d42030c7a114b26e020de776265b7260b8b4606779f1930d938080c804d3e2")
                .unwrap();
        let s =
            BigInt::from_hex("7a99d4297ec6ea427dae9fdf326a5fb8b2fdc5a0c98e7be4c784a22b7aa9807b")
                .unwrap();
        let pubkey = Secp256k1Point::from_coor(
            &BigInt::from_hex("697bc1ff06e124e7ca8b72c554ffa5b1950aa7f2e7e7ab23c0da63dee5c90a72")
                .unwrap(),
            &BigInt::from_hex("721c7dde8df4a6d06c2bea91dc6e9c075c3c35926d73f891788b9ae681b7eed5")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("1000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256k1
        ));
    }

    #[test]
    fn test_verify_r1_ok() {
        let r =
            BigInt::from_hex("899b9326a056734c9c1b663124b9accb00453f13ed4cf4fdef1ebd05688464cd")
                .unwrap();
        let s =
            BigInt::from_hex("38da4184c1176c73eb1e89b694054679b47cf1cad512d731894f299828c71929")
                .unwrap();
        let pubkey = Secp256r1Point::from_coor(
            &BigInt::from_hex("edb74abcc30629455eccbe8d3a61a8694999656de8b8f0615ad50c4c3ef238e5")
                .unwrap(),
            &BigInt::from_hex("dcf1956f7877ffb5c927e5d3e479fe913e10a0caa7a34866fe44f8bddf4b0a04")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256r1
        ));
    }

    #[test]
    fn test_verify_r1_wrong_r() {
        let r =
            BigInt::from_hex("999b9326a056734c9c1b663124b9accb00453f13ed4cf4fdef1ebd05688464cd")
                .unwrap();
        let s =
            BigInt::from_hex("38da4184c1176c73eb1e89b694054679b47cf1cad512d731894f299828c71929")
                .unwrap();
        let pubkey = Secp256r1Point::from_coor(
            &BigInt::from_hex("edb74abcc30629455eccbe8d3a61a8694999656de8b8f0615ad50c4c3ef238e5")
                .unwrap(),
            &BigInt::from_hex("dcf1956f7877ffb5c927e5d3e479fe913e10a0caa7a34866fe44f8bddf4b0a04")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256r1
        ));
    }

    #[test]
    fn test_verify_r1_wrong_s() {
        let r =
            BigInt::from_hex("899b9326a056734c9c1b663124b9accb00453f13ed4cf4fdef1ebd05688464cd")
                .unwrap();
        let s =
            BigInt::from_hex("48da4184c1176c73eb1e89b694054679b47cf1cad512d731894f299828c71929")
                .unwrap();
        let pubkey = Secp256r1Point::from_coor(
            &BigInt::from_hex("edb74abcc30629455eccbe8d3a61a8694999656de8b8f0615ad50c4c3ef238e5")
                .unwrap(),
            &BigInt::from_hex("dcf1956f7877ffb5c927e5d3e479fe913e10a0caa7a34866fe44f8bddf4b0a04")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256r1
        ));
    }

    #[test]
    fn test_verify_r1_wrong_pk() {
        let r =
            BigInt::from_hex("899b9326a056734c9c1b663124b9accb00453f13ed4cf4fdef1ebd05688464cd")
                .unwrap();
        let s =
            BigInt::from_hex("38da4184c1176c73eb1e89b694054679b47cf1cad512d731894f299828c71929")
                .unwrap();
        let pubkey = Secp256r1Point::from_coor(
            &BigInt::from_hex("c6a9361d39ebc4324027de99a953de94fcc79150a023a7e10cbc9e36926cc3ba")
                .unwrap(),
            &BigInt::from_hex("a8b5559fa5b697360dc7633f7782fd9f1ec4ba090dc362fd79ee7e6313d755a4")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256r1
        ));
    }

    #[test]
    #[should_panic]
    fn test_verify_r1_invalid_pk() {
        let r =
            BigInt::from_hex("899b9326a056734c9c1b663124b9accb00453f13ed4cf4fdef1ebd05688464cd")
                .unwrap();
        let s =
            BigInt::from_hex("38da4184c1176c73eb1e89b694054679b47cf1cad512d731894f299828c71929")
                .unwrap();
        let msg_hash =
            BigInt::from_hex("0000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(verify(
            &r,
            &s,
            &"1234567890".to_string(),
            &msg_hash,
            Curve::Secp256k1
        ));
    }

    #[test]
    fn test_verify_r1_wrong_hash() {
        let r =
            BigInt::from_hex("899b9326a056734c9c1b663124b9accb00453f13ed4cf4fdef1ebd05688464cd")
                .unwrap();
        let s =
            BigInt::from_hex("38da4184c1176c73eb1e89b694054679b47cf1cad512d731894f299828c71929")
                .unwrap();
        let pubkey = Secp256r1Point::from_coor(
            &BigInt::from_hex("edb74abcc30629455eccbe8d3a61a8694999656de8b8f0615ad50c4c3ef238e5")
                .unwrap(),
            &BigInt::from_hex("dcf1956f7877ffb5c927e5d3e479fe913e10a0caa7a34866fe44f8bddf4b0a04")
                .unwrap(),
        )
        .unwrap();
        let msg_hash =
            BigInt::from_hex("100000000000000fffffffffffffffffff00000000000000ffffffffff000000")
                .unwrap();
        assert!(!verify(
            &r,
            &s,
            &pubkey.to_bigint().to_hex(),
            &msg_hash,
            Curve::Secp256r1
        ));
    }

    // test vectors from https://tools.ietf.org/html/rfc8032#page-24 (https://ed25519.cr.yp.to/python/sign.input)
    #[test]
    fn test_verify_ed_ok() {
        let r =
            BigInt::from_hex("92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da")
                .unwrap();
        let s =
            BigInt::from_hex("085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00")
                .unwrap();
        let pk_str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c".to_string();
        let msg = BigInt::from_hex("72").unwrap();
        assert!(verify(&r, &s, &pk_str, &msg, Curve::Curve25519));

        let r =
            BigInt::from_hex("6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac")
                .unwrap();
        let s =
            BigInt::from_hex("18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a")
                .unwrap();
        let pk_str = "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025".to_string();
        let msg = BigInt::from_hex("af82").unwrap();
        assert!(verify(&r, &s, &pk_str, &msg, Curve::Curve25519));

        let r =
            BigInt::from_hex("0aab4c900501b3e24d7cdf4663326a3a87df5e4843b2cbdb67cbf6e460fec350")
                .unwrap();
        let s =
            BigInt::from_hex("aa5371b1508f9f4528ecea23c436d94b5e8fcd4f681e30a6ac00a9704a188a03")
                .unwrap();
        let pk_str = "278117fc144c72340f67d0f2316e8386ceffbf2b2428c9c51fef7c597f1d426e".to_string();
        let msg = BigInt::from_hex("08b8b2b733424243760fe426a4b54908632110a66c2f6591eabd3345e3e4eb98fa6e264bf09efe12ee50f8f54e9f77b1e355f6c50544e23fb1433ddf73be84d879de7c0046dc4996d9e773f4bc9efe5738829adb26c81b37c93a1b270b20329d658675fc6ea534e0810a4432826bf58c941efb65d57a338bbd2e26640f89ffbc1a858efcb8550ee3a5e1998bd177e93a7363c344fe6b199ee5d02e82d522c4feba15452f80288a821a579116ec6dad2b3b310da903401aa62100ab5d1a36553e06203b33890cc9b832f79ef80560ccb9a39ce767967ed628c6ad573cb116dbefefd75499da96bd68a8a97b928a8bbc103b6621fcde2beca1231d206be6cd9ec7aff6f6c94fcd7204ed3455c68c83f4a41da4af2b74ef5c53f1d8ac70bdcb7ed185ce81bd84359d44254d95629e9855a94a7c1958d1f8ada5d0532ed8a5aa3fb2d17ba70eb6248e594e1a2297acbbb39d502f1a8c6eb6f1ce22b3de1a1f40cc24554119a831a9aad6079cad88425de6bde1a9187ebb6092cf67bf2b13fd65f27088d78b7e883c8759d2c4f5c65adb7553878ad575f9fad878e80a0c9ba63bcbcc2732e69485bbc9c90bfbd62481d9089beccf80cfe2df16a2cf65bd92dd597b0707e0917af48bbb75fed413d238f5555a7a569d80c3414a8d0859dc65a46128bab27af87a71314f318c782b23ebfe808b82b0ce26401d2e22f04d83d1255dc51addd3b75a2b1ae0784504df543af8969be3ea7082ff7fc9888c144da2af58429ec96031dbcad3dad9af0dcbaaaf268cb8fcffead94f3c7ca495e056a9b47acdb751fb73e666c6c655ade8297297d07ad1ba5e43f1bca32301651339e22904cc8c42f58c30c04aafdb038dda0847dd988dcda6f3bfd15c4b4c4525004aa06eeff8ca61783aacec57fb3d1f92b0fe2fd1a85f6724517b65e614ad6808d6f6ee34dff7310fdc82aebfd904b01e1dc54b2927094b2db68d6f903b68401adebf5a7e08d78ff4ef5d63653a65040cf9bfd4aca7984a74d37145986780fc0b16ac451649de6188a7dbdf191f64b5fc5e2ab47b57f7f7276cd419c17a3ca8e1b939ae49e488acba6b965610b5480109c8b17b80e1b7b750dfc7598d5d5011fd2dcc5600a32ef5b52a1ecc820e308aa342721aac0943bf6686b64b2579376504ccc493d97e6aed3fb0f9cd71a43dd497f01f17c0e2cb3797aa2a2f256656168e6c496afc5fb93246f6b1116398a346f1a641f3b041e989f7914f90cc2c7fff357876e506b50d334ba77c225bc307ba537152f3f1610e4eafe595f6d9d90d11faa933a15ef1369546868a7f3a45a96768d40fd9d03412c091c6315cf4fde7cb68606937380db2eaaa707b4c4185c32eddcdd306705e4dc1ffc872eeee475a64dfac86aba41c0618983f8741c5ef68d3a101e8a3b8cac60c905c15fc910840b94c00a0b9d0").unwrap();
        assert!(verify(&r, &s, &pk_str, &msg, Curve::Curve25519));
    }

    #[test]
    fn test_verify_ed_wrong_r() {
        let r =
            BigInt::from_hex("6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac")
                .unwrap();
        let s =
            BigInt::from_hex("085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00")
                .unwrap();
        let pk_str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c".to_string();
        let msg = BigInt::from_hex("72").unwrap();
        assert!(!verify(&r, &s, &pk_str, &msg, Curve::Curve25519));
    }

    #[test]
    fn test_verify_ed_wrong_s() {
        let r =
            BigInt::from_hex("92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da")
                .unwrap();
        let s =
            BigInt::from_hex("18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a")
                .unwrap();
        let pk_str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c".to_string();
        let msg = BigInt::from_hex("72").unwrap();
        assert!(!verify(&r, &s, &pk_str, &msg, Curve::Curve25519));
    }

    #[test]
    fn test_verify_ed_wrong_pk() {
        let r =
            BigInt::from_hex("92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da")
                .unwrap();
        let s =
            BigInt::from_hex("085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00")
                .unwrap();
        let pk_str = "4d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c".to_string();
        let msg = BigInt::from_hex("72").unwrap();
        assert!(!verify(&r, &s, &pk_str, &msg, Curve::Curve25519));
    }

    #[test]
    fn test_verify_ed_wrong_msg() {
        let r =
            BigInt::from_hex("92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da")
                .unwrap();
        let s =
            BigInt::from_hex("085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00")
                .unwrap();
        let pk_str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c".to_string();
        let msg = BigInt::from_hex("82").unwrap();
        assert!(!verify(&r, &s, &pk_str, &msg, Curve::Curve25519));
    }

    #[test]
    fn test_pk_from_sk_k1_ok() {
        let sk =
            BigInt::from_hex("bb8bcf52a5f944f351c5bc856b7a4c41a5f370f5ce99dce0c8d6f1d491cd34bf")
                .unwrap();
        let pk = publickey_from_secretkey(&sk, Curve::Secp256k1).unwrap();
        assert_eq!(pk, "0461646fdc4544f10294e20e994ce56d8c0ff852596eb6b3aa0ba9d4b2079d86d42b3b5e8491a48ff6e1620732579807916eeb07beb6f9970dc5952bd444404f74".to_string());
    }

    #[test]
    fn test_pk_from_sk_k1_wrong() {
        let sk =
            BigInt::from_hex("cb8bcf52a5f944f351c5bc856b7a4c41a5f370f5ce99dce0c8d6f1d491cd34bf")
                .unwrap();
        let pk = publickey_from_secretkey(&sk, Curve::Secp256k1).unwrap();
        assert_ne!(pk, "0461646fdc4544f10294e20e994ce56d8c0ff852596eb6b3aa0ba9d4b2079d86d42b3b5e8491a48ff6e1620732579807916eeb07beb6f9970dc5952bd444404f74".to_string());
    }

    #[test]
    fn test_pk_from_sk_r1_ok() {
        let sk =
            BigInt::from_hex("bb8bcf52a5f944f351c5bc856b7a4c41a5f370f5ce99dce0c8d6f1d491cd34bf")
                .unwrap();
        let pk = publickey_from_secretkey(&sk, Curve::Secp256r1).unwrap();
        assert_eq!(pk, "04b0c36f4c3c2ee418e73bae21518226efbbdb526a6f87c2ed4d5d271c7bcd397e4c4a71a6e72c775c22ab5a356c9186717f3f90f0821d9abe427ed1462e74427e".to_string());
    }

    #[test]
    fn test_pk_from_sk_r1_wrong() {
        let sk =
            BigInt::from_hex("cb8bcf52a5f944f351c5bc856b7a4c41a5f370f5ce99dce0c8d6f1d491cd34bf")
                .unwrap();
        let pk = publickey_from_secretkey(&sk, Curve::Secp256r1).unwrap();
        assert_ne!(pk, "04b0c36f4c3c2ee418e73bae21518226efbbdb526a6f87c2ed4d5d271c7bcd397e4c4a71a6e72c775c22ab5a356c9186717f3f90f0821d9abe427ed1462e74427e".to_string());
    }

    #[test]
    fn test_pk_from_sk_ed_ok() {
        assert_eq!(
            publickey_from_secretkey(
                &BigInt::from_hex(
                    "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"
                )
                .unwrap(),
                Curve::Curve25519
            )
            .unwrap(),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a".to_string()
        );
        assert_eq!(
            publickey_from_secretkey(
                &BigInt::from_hex(
                    "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb"
                )
                .unwrap(),
                Curve::Curve25519
            )
            .unwrap(),
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c".to_string()
        );
        assert_eq!(
            publickey_from_secretkey(
                &BigInt::from_hex(
                    "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7"
                )
                .unwrap(),
                Curve::Curve25519
            )
            .unwrap(),
            "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025".to_string()
        );
        assert_eq!(
            publickey_from_secretkey(
                &BigInt::from_hex(
                    "f5e5767cf153319517630f226876b86c8160cc583bc013744c6bf255f5cc0ee5"
                )
                .unwrap(),
                Curve::Curve25519
            )
            .unwrap(),
            "278117fc144c72340f67d0f2316e8386ceffbf2b2428c9c51fef7c597f1d426e".to_string()
        );
    }

    #[test]
    fn test_pk_from_sk_ed_wrong() {
        let sk =
            BigInt::from_hex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
                .unwrap();
        let pk = publickey_from_secretkey(&sk, Curve::Curve25519).unwrap();
        assert_ne!(
            pk,
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c".to_string()
        );
    }
}
