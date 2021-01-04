#[macro_use]
extern crate criterion;

use criterion::{black_box, Criterion};
use nash_mpc::common::{dh_init_secp256k1, dh_init_secp256r1, publickey_from_secretkey, Curve};
use nash_mpc::server::{
    complete_sig, compute_rpool_secp256k1, compute_rpool_secp256r1, generate_paillier_proof,
};
use paillier_common::{DecryptionKey, MinimalDecryptionKey};
use rust_bigint::traits::Converter;
use rust_bigint::BigInt;

fn criterion_benchmark(c: &mut Criterion) {
    let paillier_sk = DecryptionKey::from(MinimalDecryptionKey{p: BigInt::from_hex("d3542d07cda6034cf8568b68d69f07b716c98dcc466d7fb89d2a40db4addfe1402ac6007b609734c80fa4dd24f005cc2d404651f724561391fd2c714c054c5ecb98c0d367d5d99cddbd788489151daa247feef546ba173db02576793f2386c89a78e1cf0b1b5e3882efb709663c8fb50f3b973e87447bc0a473b792eeb9720ef").unwrap(), q: BigInt::from_hex("bf9f1abdcfd5f3e30a609ad469637eeadf068f67735c319cd0bfe3cb7ed915d93c33c77078762c3deab68fd66a46903a3241f84ccf827ac27faa339d12f4cf818732220b2a899660765a8554d8bc6490bc7490b7874fe1651dccd25b74bcdb5481e1d09bfe3ec6143c2f9bb2cf3658d514fc8c1e48a8e095b8a0f9fe94891f67").unwrap()});
    c.bench_function("generate_paillier_proof", |b| {
        b.iter(|| {
            generate_paillier_proof(black_box(&paillier_sk));
        })
    });

    let (dh_secrets_k1, dh_publics_k1) = dh_init_secp256k1(10).unwrap();
    c.bench_function("compute_rpool_secp256k1", |b| {
        b.iter(|| {
            compute_rpool_secp256k1(black_box(&dh_secrets_k1), black_box(&dh_publics_k1)).unwrap();
        })
    });

    let (dh_secrets_r1, dh_publics_r1) = dh_init_secp256r1(10).unwrap();
    c.bench_function("compute_rpool_secp256r1", |b| {
        b.iter(|| {
            compute_rpool_secp256r1(black_box(&dh_secrets_r1), black_box(&dh_publics_r1)).unwrap();
        })
    });

    let presig_k1 = BigInt::from_hex("5a955a53b4598601890b70d1c0cba4e4bcf446623cfe529e7e52451932125f880b4139683434ff25f589e7f09441499e97a29227d8fbb484f2be4e9c602f92d411193c7b1c016290524cdbdf94f260959c1d9aacba3955d66ee759335738099902f68201a3a919358f4a2a99b1e61d63a839ad75d681e62b5258f18a4415f40709c8faac80082340cd2b96a8210eb5b1a31f9b0e498a01d985131923ce0b3ac2e874ba1089782db6c667a90c4fb1f5d1b98e133196e533efe8ad2d025806498921d1f89007e6cf013d1ed7683c41d4b07f6b3ff293b9a783043051ef1eaa0d195e706321c2ea6349d61dbf8e053dbe76e8f65d44c96c8b8ede0e69d0bba00def739123e5e5e2adb640d603defd1aa8204df00d0db82155e687e8127e9fcdbfabd2449fb48e11f85903ac9d08b32296085d114e024e677b6ec507fbaad262f4646248e0222588627fda9e20087eec30b1d94cffe9a254678821f7515afd89f5db7801886355cd3bb07493fff73bbf2256dab6b4f79dcbb4ed14adc0a731e2ce781b77728356e62277ce21fe1f4f4190a4b56499738f02bb65df7c71ed9e47fc81e2a23ee8686e921d47e11f3dc3f26eb35faffc41a9e870ab43474d4dfe0a0c1db1ec65837ee7babad54bbbb05b6648aa336f7749a8e0677415d3491431ed58ad922c71cb18c3683a0480eca1e39414ce200d6799d4f17332d647dd7f69c53637").unwrap();
    let r_k1 =
        BigInt::from_hex("02ec71e402771be8e826da22beb05f4eb0a3fb9eefcd06ebd0cb03010c942845ed")
            .unwrap();
    let k_k1 = BigInt::from_hex("b95d4e79d09b35bdfc863cdeb8bbfd85d557546e75fe2582961fbe0497525f6e")
        .unwrap();
    let pk = publickey_from_secretkey(
        &BigInt::from_hex("4794853ce9e44b4c7a69c6a3b87db077f8f910f244bb6b966ba5fed83c9756f1")
            .unwrap(),
        Curve::Secp256k1,
    )
    .unwrap();
    let msg_hash =
        BigInt::from_hex("000000000000000fffffffffffffffffff00000000000000ffffffffff000000")
            .unwrap();
    c.bench_function("complete_sig_k1", |b| {
        b.iter(|| {
            complete_sig(
                black_box(&paillier_sk),
                black_box(&presig_k1),
                black_box(&r_k1),
                black_box(&k_k1),
                black_box(Curve::Secp256k1),
                black_box(&pk),
                black_box(&msg_hash),
            )
            .unwrap();
        })
    });

    let pk = publickey_from_secretkey(
        &BigInt::from_hex("4794853ce9e44b4c7a69c6a3b87db077f8f910f244bb6b966ba5fed83c9756f1")
            .unwrap(),
        Curve::Secp256r1,
    )
    .unwrap();
    let paillier_sk = DecryptionKey::from(MinimalDecryptionKey{p: BigInt::from_hex("b666e18f83bd0f642a3a3831f7f87c81b56fe751ecaa5e8f6b31c8fdb97e757a1e0bb9ec44d66775af88289501f9ab5c27aba18561096593270033561c2dd9e00407981a721fe5fd982693f13777759c7469ec72ce181da61ea07be1a754982869929a7033e3647254d9044556d4045b412ab60fa290aa8f7e731045539cad1f").unwrap(), q: BigInt::from_hex("bb1704bebe7b50a1dc2e8b3cb868b7aebde79f603ae753e5e1a9685c2950f3502509886c26edbc4641f1544a97059299677c4dc3ec760b1472c212b5547cd32b1134bf2d3e8cfa801f9f637e8121dec460d99851398651253a5243465e02a77583b48d1463ee594c0d626f9d9ae78bd56052a31c0aab3dbbd8da96eadf0c6bff").unwrap()});
    let presig_r1 = BigInt::from_hex("112164aec0e4e1be041e43537ce94d75969d7bfd59579355456690f0d593d5b697c4be5f2a13929c5d9b6dd64bd53186c1bd869f1321ba32427e884ee81bacdb49654d6d481eeb14198b3d61a013ffea5bbd058799e7ee828eb15b3541003e67d23c5e1af87021a86b38b75468ee7df7ff595d0e138b361599371698b90fa59a3695574fb56d08fdcc32e1a35c7708ec7e12574d3c68863106c84b8053bd2e0218fcdb31fbcc6351674f274f2aa356c7b7556abb1a55fdf0e17bdd8a0003cb8221850115933aa74b68c84ebcf426636194d6717fc96ca071f3387f2d711637a9fea995a668e1064e0d6c39898ea10c655351f53801ba939327565da4bc960a20481c9f86c2344888b36df9189b58038b3822f70a431fbe82c584a4f4657d742e6a2976f6f956f722b6cc781d8f7367134b2b883a93614af92764af715bb6fcc6d14e490c7f942f6678e4f3a2ebe361c52b95f6056f1d6e20f8260b101a494f9e6fc1fdbf9d2b563be7a6791ceeb8e035a6f07c989f63e313662ef96ec663529896639e461c20ba65a1729a95b09e85ce67704b3d4ef0098b669b1bb57ea216dc60013f2c721f0ba1ce472c3cba24d88b6707bdf0f4a34b6dd75b9b6e10b4c3a7ec60380179dba76e45d472224bc7e8c452873ffa49d7c9e0eab712403a468024a509748da45d0218384d58acf8239614f7e4332cc34a7e68552f738bf50c78c6").unwrap();
    let r_r1 =
        BigInt::from_hex("3c8c36274f450152eeac61fed4d81ca470fe5b4659e64bc3b343eaee9a5b37b55")
            .unwrap();
    let k_r1 = BigInt::from_hex("581b8e054da08a656d3cb821ae1989ac1978cda5052b89f08b13d43a414f4c4a")
        .unwrap();
    c.bench_function("complete_sig_r1", |b| {
        b.iter(|| {
            complete_sig(
                black_box(&paillier_sk),
                black_box(&presig_r1),
                black_box(&r_r1),
                black_box(&k_r1),
                black_box(Curve::Secp256r1),
                black_box(&pk),
                black_box(&msg_hash),
            )
            .unwrap();
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark
}

criterion_main!(benches);
