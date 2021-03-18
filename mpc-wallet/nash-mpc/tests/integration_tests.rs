use nash_mpc::client::{
    compute_presig, fill_rpool_curve25519,
    fill_rpool_secp256k1, fill_rpool_secp256r1, APIchildkeyCreator,
};
use nash_mpc::common::{
    dh_init_curve25519, dh_init_secp256k1, dh_init_secp256r1, publickey_from_secretkey,
    verify, Curve,
};
use nash_mpc::curves::traits::ECScalar;
use nash_mpc::server::{
    complete_sig_ecdsa, complete_sig_eddsa, compute_rpool_curve25519, compute_rpool_secp256k1,
    compute_rpool_secp256r1, decrypt, generate_paillier_keypair, generate_paillier_proof,
};
use rust_bigint::traits::Converter;
use rust_bigint::BigInt;

#[test]
fn test_integration_k1() {
    let secret_key =
        BigInt::from_hex("4794853ce9e44b4c7a69c6a3b87db077f8f910f244bb6b966ba5fed83c9756f1")
            .unwrap();
    let mut api_childkey_creator = APIchildkeyCreator::init(&secret_key);
    let (paillier_pk, paillier_sk) = generate_paillier_keypair();
    let correct_key_proof = generate_paillier_proof(&paillier_sk);
    api_childkey_creator = api_childkey_creator
        .verify_paillier(&paillier_pk, &correct_key_proof)
        .unwrap();
    let api_childkey = api_childkey_creator
        .create_api_childkey(Curve::Secp256k1)
        .unwrap();
    let msg_hash =
        BigInt::from_hex("000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
            .unwrap();
    let (client_dh_secrets, client_dh_publics) = dh_init_secp256k1(1).unwrap();
    let (server_dh_secrets, server_dh_publics) = dh_init_secp256k1(1).unwrap();
    fill_rpool_secp256k1(client_dh_secrets, &server_dh_publics, &paillier_pk).unwrap();
    let rpool = compute_rpool_secp256k1(&server_dh_secrets, &client_dh_publics).unwrap();
    let (presig, r) = compute_presig(&api_childkey, &msg_hash, Curve::Secp256k1).unwrap();
    let k = rpool
        .get(&format!("{:0>66}", r.to_hex()))
        .unwrap()
        .to_bigint();
    let pk = publickey_from_secretkey(&secret_key, Curve::Secp256k1).unwrap();
    let (rx, s, _) = complete_sig_ecdsa(
        &paillier_sk,
        &presig,
        &r,
        &k,
        Curve::Secp256k1,
        &pk,
        &msg_hash,
    )
    .unwrap();
    assert!(verify(
        &rx,
        &s,
        &publickey_from_secretkey(&secret_key, Curve::Secp256k1).unwrap(),
        &msg_hash,
        Curve::Secp256k1,
    ));
}

#[test]
fn test_integration_r1() {
    let secret_key =
        BigInt::from_hex("4794853ce9e44b4c7a69c6a3b87db077f8f910f244bb6b966ba5fed83c9756f1")
            .unwrap();
    let mut api_childkey_creator = APIchildkeyCreator::init(&secret_key);
    let (paillier_pk, paillier_sk) = generate_paillier_keypair();
    let correct_key_proof = generate_paillier_proof(&paillier_sk);
    api_childkey_creator = api_childkey_creator
        .verify_paillier(&paillier_pk, &correct_key_proof)
        .unwrap();
    let api_childkey = api_childkey_creator
        .create_api_childkey(Curve::Secp256r1)
        .unwrap();
    let msg_hash =
        BigInt::from_hex("000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
            .unwrap();
    let (client_dh_secrets, client_dh_publics) = dh_init_secp256r1(1).unwrap();
    let (server_dh_secrets, server_dh_publics) = dh_init_secp256r1(1).unwrap();
    fill_rpool_secp256r1(client_dh_secrets, &server_dh_publics, &paillier_pk).unwrap();
    let rpool = compute_rpool_secp256r1(&server_dh_secrets, &client_dh_publics).unwrap();
    let (presig, r) = compute_presig(&api_childkey, &msg_hash, Curve::Secp256r1).unwrap();
    let k = rpool
        .get(&format!("{:0>66}", r.to_hex()))
        .unwrap()
        .to_bigint();
    let pk = publickey_from_secretkey(&secret_key, Curve::Secp256r1).unwrap();
    let (rx, s, _) = complete_sig_ecdsa(
        &paillier_sk,
        &presig,
        &r,
        &k,
        Curve::Secp256r1,
        &pk,
        &msg_hash,
    )
    .unwrap();
    assert!(verify(
        &rx,
        &s,
        &publickey_from_secretkey(&secret_key, Curve::Secp256r1).unwrap(),
        &msg_hash,
        Curve::Secp256r1,
    ));
}

#[test]
fn test_integration_ed() {
    let secret_key =
        BigInt::from_hex("f445c1855a1cd979572dc650d1611d266291daf4c06c8b5ceec98f0cfba3b65f")
            .unwrap();
    let pk = publickey_from_secretkey(&secret_key, Curve::Curve25519).unwrap();
    let (paillier_pk, paillier_sk) = generate_paillier_keypair();
    let api_childkey_creator =
        APIchildkeyCreator::init_with_verified_paillier(&secret_key, &paillier_pk);
    let api_childkey = api_childkey_creator
        .create_api_childkey(Curve::Curve25519)
        .unwrap();
    let server_secret_share = decrypt(&paillier_sk, &api_childkey.server_secret_share_encrypted);
    let msg = BigInt::from_hex("68656c6c6f2c20776f726c6421").unwrap();
    let (client_dh_secrets, client_dh_publics) = dh_init_curve25519(1).unwrap();
    let (server_dh_secrets, server_dh_publics) = dh_init_curve25519(1).unwrap();
    fill_rpool_curve25519(client_dh_secrets, &server_dh_publics).unwrap();
    let rpool = compute_rpool_curve25519(&server_dh_secrets, &client_dh_publics).unwrap();
    let (presig, r) = compute_presig(&api_childkey, &msg, Curve::Curve25519).unwrap();
    let r_server = rpool.get(&r.to_hex()).unwrap();
    let s = complete_sig_eddsa(
        server_secret_share,
        &presig,
        &r,
        r_server.clone(),
        &pk,
        &msg,
    )
    .unwrap();
    assert!(verify(&r, &s.to_bigint_le(), &pk, &msg, Curve::Curve25519));
}
