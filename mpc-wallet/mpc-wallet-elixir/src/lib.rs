/*
 * Elixir NIFs to MPC-based API keys
 */

#[cfg(feature = "secp256k1")]
use nash_mpc::curves::secp256_k1::{Secp256k1Point, Secp256k1Scalar};
#[cfg(feature = "k256")]
use nash_mpc::curves::secp256_k1_rust::{Secp256k1Point, Secp256k1Scalar};
use nash_mpc::curves::secp256_r1::{Secp256r1Point, Secp256r1Scalar};
use nash_mpc::paillier_common::{DecryptionKey, EncryptionKey};
use nash_mpc::rust_bigint::traits::Converter;
use nash_mpc::rust_bigint::BigInt;
use nash_mpc::{client, common, server};
use rustler::{Encoder, Env, Error, Term};

rustler::init!(
    "Elixir.Server.MPCwallet",
    [generate_paillier_keypair_and_proof, dh_rpool, complete_sig, verify, compute_presig, fill_rpool, dh_init, init_api_childkey_creator, init_api_childkey_creator_with_verified_paillier, verify_paillier, create_api_childkey, publickey_from_secretkey]
);

mod atoms {
    rustler::atoms! {
        ok,
        error,
        __true__ = "true",
        __false__ = "false",
    }
}

/// generate paillier keypair
/// input: none
/// output: paillier secret key, paillier public key, proof that paillier key was generated correctly
#[rustler::nif]
fn generate_paillier_keypair_and_proof<'a>(
    env: Env<'a>,
) -> Result<Term<'a>, Error> {
    let (paillier_pk, paillier_sk) = server::generate_paillier_keypair();
    let correct_key_proof = server::generate_paillier_proof(&paillier_sk);
    let paillier_pk_json = serde_json::to_string(&paillier_pk).unwrap();
    let paillier_sk_json = serde_json::to_string(&paillier_sk).unwrap();
    let correct_key_proof_json = serde_json::to_string(&correct_key_proof).unwrap();
    Ok((
        atoms::ok(),
        &paillier_sk_json,
        &paillier_pk_json,
        &correct_key_proof_json,
    )
        .encode(env))
}

/// Diffie-Hellman, compute random and nonce values to be added to the pool as well as a set of public values.
/// input: client_dh_publics, curve (Secp256r1 or Secp256k1)
/// output: rpool_new, server_dh_publics
#[rustler::nif]
fn dh_rpool<'a>(env: Env<'a>, client_dh_publics_str: String, curve_str: String) -> Result<Term<'a>, Error> {
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };

    let rpool_new_json: String;
    let server_dh_publics_json: String;
    if curve == common::Curve::Secp256k1 {
        let client_dh_publics: Vec<Secp256k1Point> =
            match serde_json::from_str(&client_dh_publics_str) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(
                        (atoms::error(), &"error deserializing client_dh_publics").encode(env)
                    )
                }
            };
        let (server_dh_secrets, server_dh_publics) =
            match common::dh_init_secp256k1(client_dh_publics.len()) {
                Ok(v) => v,
                Err(_) => return Ok((atoms::error(), &"error: n too big").encode(env)),
            };
        let rpool_new =
            match server::compute_rpool_secp256k1(&server_dh_secrets, &client_dh_publics) {
                Ok(v) => v,
                Err(_) => {
                    return Ok((
                        atoms::error(),
                        &"error: server_dh_secrets and client_dh_publics have different lengths",
                    )
                        .encode(env))
                }
            };
        server_dh_publics_json = serde_json::to_string(&server_dh_publics).unwrap();
        rpool_new_json = serde_json::to_string(&rpool_new).unwrap();
    } else if curve == common::Curve::Secp256r1 {
        let client_dh_publics: Vec<Secp256r1Point> =
            match serde_json::from_str(&client_dh_publics_str) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(
                        (atoms::error(), &"error deserializing client_dh_publics").encode(env)
                    )
                }
            };
        let (server_dh_secrets, server_dh_publics) =
            match common::dh_init_secp256r1(client_dh_publics.len()) {
                Ok(v) => v,
                Err(_) => return Ok((atoms::error(), &"error: n too big").encode(env)),
            };
        let rpool_new =
            match server::compute_rpool_secp256r1(&server_dh_secrets, &client_dh_publics) {
                Ok(v) => v,
                Err(_) => {
                    return Ok((
                        atoms::error(),
                        &"error: server_dh_secrets and client_dh_publics have different lengths",
                    )
                        .encode(env))
                }
            };
        server_dh_publics_json = serde_json::to_string(&server_dh_publics).unwrap();
        rpool_new_json = serde_json::to_string(&rpool_new).unwrap();
    } else {
        return Ok((atoms::error(), &"error: invalid curve").encode(env));
    }
    Ok((atoms::ok(), &rpool_new_json, &server_dh_publics_json).encode(env))
}

/// finalize presignature to normal ECDSA signature
/// input: paillier_sk, presig, r, k, curve, pubkey, msg_hash
/// output: r, s, recid
#[rustler::nif]
fn complete_sig<'a>(env: Env<'a>, paillier_sk_str: String, presig_str: String, r_str: String, k_str: String, curve_str: String, pubkey: String , msg_hash_str: String) -> Result<Term<'a>, Error> {
    let paillier_sk: DecryptionKey = match serde_json::from_str(&paillier_sk_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing paillier_sk").encode(env)),
    };
    let presig = match BigInt::from_hex(&presig_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing presig").encode(env)),
    };
    let r = match BigInt::from_hex(&r_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing r").encode(env)),
    };
    let k = match BigInt::from_hex(&k_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing k").encode(env)),
    };
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };
    let msg_hash = match BigInt::from_hex(&msg_hash_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing msg_hash").encode(env)),
    };

    let (r, s, recid) =
        match server::complete_sig(&paillier_sk, &presig, &r, &k, curve, &pubkey, &msg_hash) {
            Ok(v) => v,
            Err(_) => {
                return Ok((atoms::error(), &"error: completing signature failed").encode(env))
            }
        };
    // add leading zeros if necessary
    Ok((
        atoms::ok(),
        &format!("{:0>64}", r.to_hex()),
        &format!("{:0>64}", s.to_hex()),
        &recid,
    )
        .encode(env))
}

/// verify signature for a message under given public key
/// input: r, s, pubkey, msg_hash, curve
/// output: ok|error
#[rustler::nif]
fn verify<'a>(env: Env<'a>, r_str: String, s_str: String, pubkey: String, msg_hash_str: String, curve_str: String) -> Result<Term<'a>, Error> {
    let r = match BigInt::from_hex(&r_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing r").encode(env)),
    };
    let s = match BigInt::from_hex(&s_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing s").encode(env)),
    };
    let msg_hash = match BigInt::from_hex(&msg_hash_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing msg_hash").encode(env)),
    };
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };

    if common::verify(&r, &s, &pubkey, &msg_hash, curve) {
        Ok(atoms::ok().encode(env))
    } else {
        Ok(atoms::error().encode(env))
    }
}

/// Diffie-Hellman: create a set of secret values and a set of public values.
/// input: n (number of key pairs to generate), curve (Secp256r1 or Secp256k1)
/// output: dh_secrets, dh_publics
#[rustler::nif]
fn dh_init<'a>(env: Env<'a>, n: usize, curve_str: String) -> Result<Term<'a>, Error> {
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };

    let dh_secrets_json: String;
    let dh_publics_json: String;
    if curve == common::Curve::Secp256k1 {
        let (dh_secrets, dh_publics) = match common::dh_init_secp256k1(n) {
            Ok(v) => v,
            Err(_) => return Ok((atoms::error(), &"error: n too big").encode(env)),
        };
        dh_secrets_json = serde_json::to_string(&dh_secrets).unwrap();
        dh_publics_json = serde_json::to_string(&dh_publics).unwrap();
    } else if curve == common::Curve::Secp256r1 {
        let (dh_secrets, dh_publics) = match common::dh_init_secp256r1(n) {
            Ok(v) => v,
            Err(_) => return Ok((atoms::error(), &"error: n too big").encode(env)),
        };
        dh_secrets_json = serde_json::to_string(&dh_secrets).unwrap();
        dh_publics_json = serde_json::to_string(&dh_publics).unwrap();
    } else {
        return Ok((atoms::error(), &"error: invalid curve").encode(env));
    }
    Ok((atoms::ok(), &dh_secrets_json, &dh_publics_json).encode(env))
}

/// compute presignature
/// input: api_childkey, message_hash, curve
/// output: presignature, r
#[rustler::nif]
fn compute_presig<'a>(env: Env<'a>, api_childkey_str: String, msg_hash_str: String, curve_str: String) -> Result<Term<'a>, Error> {
    let api_childkey: client::APIchildkey = match serde_json::from_str(&api_childkey_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing api_childkey").encode(env)),
    };
    let msg_hash = match BigInt::from_hex(&msg_hash_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing msg_hash").encode(env)),
    };
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };

    let (presig, r) = match client::compute_presig(&api_childkey, &msg_hash, curve) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error getting value from rpool").encode(env)),
    };
    // add leading zeros if necessary
    Ok((
        atoms::ok(),
        &format!("{:0>1024}", presig.to_hex()),
        &format!("{:0>66}", r.to_hex()),
    )
        .encode(env))
}

/// fill pool of r-values from dh secret and public values
/// input: server_dh_secrets, client_dh_publics, curve, paillier_pk
/// output: ok|error
#[rustler::nif]
fn fill_rpool<'a>(env: Env<'a>, server_dh_secrets_str: String, client_dh_publics_str: String, curve_str: String, paillier_pk_str: String) -> Result<Term<'a>, Error> {
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };
    let paillier_pk: EncryptionKey = match serde_json::from_str(&paillier_pk_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing paillier_pk").encode(env)),
    };

    if curve == common::Curve::Secp256k1 {
        let server_dh_secrets: Vec<Secp256k1Scalar> =
            match serde_json::from_str(&server_dh_secrets_str) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(
                        (atoms::error(), &"error deserializing server_dh_secrets").encode(env)
                    )
                }
            };
        let client_dh_publics: Vec<Secp256k1Point> =
            match serde_json::from_str(&client_dh_publics_str) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(
                        (atoms::error(), &"error deserializing client_dh_publics").encode(env)
                    )
                }
            };
        match client::fill_rpool_secp256k1(server_dh_secrets, &client_dh_publics, &paillier_pk) {
            Ok(v) => v,
            Err(_) => return Ok((atoms::error(), &"error filling rpool").encode(env)),
        };
    } else if curve == common::Curve::Secp256r1 {
        let server_dh_secrets: Vec<Secp256r1Scalar> =
            match serde_json::from_str(&server_dh_secrets_str) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(
                        (atoms::error(), &"error deserializing server_dh_secrets").encode(env)
                    )
                }
            };
        let client_dh_publics: Vec<Secp256r1Point> =
            match serde_json::from_str(&client_dh_publics_str) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(
                        (atoms::error(), &"error deserializing client_dh_publics").encode(env)
                    )
                }
            };
        match client::fill_rpool_secp256r1(server_dh_secrets, &client_dh_publics, &paillier_pk) {
            Ok(v) => v,
            Err(_) => return Ok((atoms::error(), &"error filling rpool").encode(env)),
        };
    } else {
        return Ok((atoms::error(), &"error: invalid curve").encode(env));
    }
    Ok((atoms::ok()).encode(env))
}

/// initialize API child key creation by setting the full secret key
/// input: secret_key
/// output: api_childkey_creator
#[rustler::nif]
fn init_api_childkey_creator<'a>(env: Env<'a>, secret_key_str: String) -> Result<Term<'a>, Error> {
    let secret_key = match BigInt::from_hex(&secret_key_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing secret_key").encode(env)),
    };

    let api_childkey_creator = client::APIchildkeyCreator::init(&secret_key);
    let api_childkey_creator_json = serde_json::to_string(&api_childkey_creator).unwrap();
    Ok((atoms::ok(), &api_childkey_creator_json).encode(env))
}

/// initialize API child key creation by setting the full secret key and the paillier public key, assuming that the paillier public key has been verified before.
/// input: secret_key, paillier_pk
/// output: apichildkeycreator
#[rustler::nif]
fn init_api_childkey_creator_with_verified_paillier<'a>(
    env: Env<'a>,
    secret_key_str: String,
    paillier_pk_str: String
) -> Result<Term<'a>, Error> {
    let secret_key = match BigInt::from_hex(&secret_key_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing secret_key").encode(env)),
    };
    let paillier_pk: EncryptionKey = match serde_json::from_str(&paillier_pk_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing paillier_pk").encode(env)),
    };

    let api_childkey_creator =
        client::APIchildkeyCreator::init_with_verified_paillier(&secret_key, &paillier_pk);
    let api_childkey_creator_json = serde_json::to_string(&api_childkey_creator).unwrap();
    Ok((atoms::ok(), &api_childkey_creator_json).encode(env))
}

/// verify that the Paillier public key was generated correctly.
/// input: api_childkey_creator, paillier_pk, correct_key_proof
/// output: api_childkey_creator
#[rustler::nif]
fn verify_paillier<'a>(env: Env<'a>, api_childkey_creator_str: String, paillier_pk_str: String, correct_key_proof_str: String) -> Result<Term<'a>, Error> {
    let api_childkey_creator: client::APIchildkeyCreator =
        match serde_json::from_str(&api_childkey_creator_str) {
            Ok(v) => v,
            Err(_) => {
                return Ok(
                    (atoms::error(), &"error deserializing api_childkey_creator").encode(env),
                )
            }
        };
    let paillier_pk: EncryptionKey = match serde_json::from_str(&paillier_pk_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing paillier_pk").encode(env)),
    };
    let correct_key_proof: common::CorrectKeyProof =
        match serde_json::from_str(&correct_key_proof_str) {
            Ok(v) => v,
            Err(_) => {
                return Ok((atoms::error(), &"error deserializing correct_key_proof").encode(env))
            }
        };

    let api_childkey_creator_new =
        match api_childkey_creator.verify_paillier(&paillier_pk, &correct_key_proof) {
            Ok(v) => v,
            Err(_) => return Ok((atoms::error(), &"error verifying paillier_pk").encode(env)),
        };
    let api_childkey_creator_new_json = serde_json::to_string(&api_childkey_creator_new).unwrap();
    Ok((atoms::ok(), &api_childkey_creator_new_json).encode(env))
}

/// create API childkey
/// input: api_childkey_creator, curve
/// output: api_childkey
#[rustler::nif]
fn create_api_childkey<'a>(env: Env<'a>, api_childkey_creator_str: String, curve_str: String) -> Result<Term<'a>, Error> {
    let api_childkey_creator: client::APIchildkeyCreator =
        match serde_json::from_str(&api_childkey_creator_str) {
            Ok(v) => v,
            Err(_) => {
                return Ok(
                    (atoms::error(), &"error deserializing api_childkey_creator").encode(env),
                )
            }
        };
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };

    let api_childkey = match api_childkey_creator.create_api_childkey(curve) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error: paillier_pk not verified yet").encode(env)),
    };
    let api_childkey_json = serde_json::to_string(&api_childkey).unwrap();
    Ok((atoms::ok(), &api_childkey_json).encode(env))
}

/// Derive public key from given secret key.
/// input: (full) secret key, curve
/// output: public_key
#[rustler::nif]
fn publickey_from_secretkey<'a>(env: Env<'a>, secret_key_str: String, curve_str: String) -> Result<Term<'a>, Error> {
    let secret_key = match BigInt::from_hex(&secret_key_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing secret_key").encode(env)),
    };
    let curve: common::Curve = match serde_json::from_str(&curve_str) {
        Ok(v) => v,
        Err(_) => return Ok((atoms::error(), &"error deserializing curve").encode(env)),
    };

    let public_key = common::publickey_from_secretkey(&secret_key, curve);
    let public_key_json = serde_json::to_string(&public_key).unwrap();
    Ok((atoms::ok(), &public_key_json).encode(env))
}
