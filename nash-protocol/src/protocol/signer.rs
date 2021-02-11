#[cfg(feature = "k256")]
use k256::ecdsa::signature::Signer as k256_Signer;
#[cfg(feature = "k256")]
use k256::ecdsa::{Signature, SigningKey};
#[cfg(feature = "secp256k1")]
use rust_bigint::traits::Converter;
#[cfg(feature = "secp256k1")]
use secp256k1::constants::{COMPACT_SIGNATURE_SIZE, MESSAGE_SIZE, SECRET_KEY_SIZE};
#[cfg(feature = "secp256k1")]
use secp256k1::{Message, SecretKey};

use nash_mpc::client::APIchildkey;
use nash_mpc::common::Curve;
#[cfg(feature = "secp256k1")]
use nash_mpc::curves::secp256_k1::get_context;
#[cfg(feature = "k256")]
use nash_mpc::curves::secp256_k1_rust::Secp256k1Scalar;
#[cfg(feature = "k256")]
use nash_mpc::curves::traits::ECScalar;
use nash_mpc::paillier_common;
use nash_mpc::rust_bigint::BigInt;

use crate::errors::{ProtocolError, Result};
use crate::protocol::RequestPayloadSignature;
use crate::types::ApiKeys;
use crate::types::Blockchain;
use crate::types::PublicKey;
#[cfg(feature = "secp256k1")]
use crate::utils::{der_encode_sig, hash_message};
use std::sync::atomic::{AtomicU32, Ordering};

pub fn chain_path(chain: Blockchain) -> &'static str {
    match chain {
        Blockchain::NEO => "m/44'/888'/0'/0/0",
        Blockchain::Ethereum => "m/44'/60'/0'/0/0",
        Blockchain::Bitcoin => "m/44'/0'/0'/0/0",
    }
}

#[derive(Debug)]
pub struct Signer {
    pub api_keys: ApiKeys,
    k1_remaining: AtomicU32,
    r1_remaining: AtomicU32,
}

impl Signer {
    pub fn new(key_path: &str) -> Result<Self> {
        Ok(Self {
            api_keys: ApiKeys::new(key_path)?,
            k1_remaining: AtomicU32::new(0),
            r1_remaining: AtomicU32::new(0),
        })
    }

    pub fn from_data(secret: &str, session: &str) -> Result<Self> {
        Ok(Self {
            api_keys: ApiKeys::from_data(secret, session)?,
            k1_remaining: AtomicU32::new(0),
            r1_remaining: AtomicU32::new(0),
        })
    }

    /// Sign GraphQL payload request via payload signing key
    /// The output is a hex string where signature has been DER encoded
    /// Either implemented with k256 from rustcrypto (pure rust) or secp256k1 (better performance)
    #[cfg(feature = "rustcrypto")]
    pub fn sign_canonical_string(&self, request: &str) -> RequestPayloadSignature {
        let signing_key: Secp256k1Scalar =
            ECScalar::from(&self.api_keys.keys.payload_signing_key).expect("Invalid key");
        let key = SigningKey::new(&signing_key.to_vec()).expect("invalid secret key");
        let sig_pre: Signature = key.try_sign(request.as_bytes()).expect("signing failed");
        let sig = sig_pre.to_asn1();
        RequestPayloadSignature {
            signed_digest: hex::encode(sig),
            public_key: self.request_payload_public_key(),
        }
    }
    #[cfg(feature = "secp256k1")]
    pub fn sign_canonical_string(&self, request: &str) -> RequestPayloadSignature {
        // create message hash
        let message_hash = hash_message(request).to_bytes();
        // add leading zeroes if necessary
        let mut msg_vec = vec![0; MESSAGE_SIZE - message_hash.len()];
        msg_vec.extend_from_slice(&message_hash);
        let msg = Message::from_slice(&msg_vec).unwrap();

        // SecretKey from BigInt
        let vec = BigInt::to_vec(&self.api_keys.keys.payload_signing_key);
        let mut v = vec![0; SECRET_KEY_SIZE - vec.len()];
        v.extend(&vec);
        let key = SecretKey::from_slice(&v).expect("invalid secret key");

        // actual signature generation (and encoding)
        let signature = get_context().sign(&msg, &key).serialize_compact();
        let r = BigInt::from_bytes(&signature[0..COMPACT_SIGNATURE_SIZE / 2]);
        let s = BigInt::from_bytes(&signature[COMPACT_SIGNATURE_SIZE / 2..COMPACT_SIGNATURE_SIZE]);
        let sig = der_encode_sig(&r, &s);

        RequestPayloadSignature {
            signed_digest: hex::encode(sig),
            public_key: self.request_payload_public_key(),
        }
    }

    /// Sign data hashed to `BigInt` with the MPC child key for the given `Blockchain`
    pub fn sign_child_key(
        &self,
        data: BigInt,
        chain: Blockchain,
    ) -> Result<(BigInt, BigInt, String)> {
        if self.get_remaining_r_vals(&chain) <= 0 {
            return Err(ProtocolError("Ran out of R values"));
        }
        let key = self.get_child_key(chain);
        let curve = match chain {
            Blockchain::Ethereum | Blockchain::Bitcoin => Curve::Secp256k1,
            Blockchain::NEO => Curve::Secp256r1,
        };
        // FIX ME: Right now the pools are under a global mutex. Make them managed
        let (sig, r) = nash_mpc::client::compute_presig(&key, &data, curve)
            .map_err(|_| ProtocolError("Error computing presignature"))?;
        // Track the fact that we now have one less R value
        self.decr_r_vals(chain);
        Ok((sig, r, key.public_key))
    }

    /// Get public key for child key on `chain`
    pub fn child_public_key(&self, chain: Blockchain) -> Result<PublicKey> {
        PublicKey::new(chain, &self.get_child_key(chain).public_key)
    }

    /// Return public key for payload signing in format expected by the Nash backend service
    /// BigInt conversion to hex will strip leading zeros, which Nash backend doesn't like
    pub fn request_payload_public_key(&self) -> String {
        let mut key_str = self.api_keys.keys.payload_public_key.to_str_radix(16);
        if key_str.len() % 2 != 0 {
            key_str = format!("0{}", &key_str);
        }
        key_str
    }

    pub fn paillier_pk(&self) -> &paillier_common::EncryptionKey {
        &self.api_keys.keys.paillier_pk
    }

    pub fn get_address(&self, chain: Blockchain) -> &str {
        &self.api_keys.keys.child_keys[chain_path(chain)].address
    }

    pub fn get_child_key(&self, chain: Blockchain) -> APIchildkey {
        let key = &self.api_keys.keys.child_keys[chain_path(chain)];
        // TODO: these should be unified! it was more convenient to parse the paillier_pk
        // once for all the key data from deserialization, which is why I need this atm.
        // is on list to fix once things are verified to be working
        APIchildkey {
            client_secret_share: key.client_secret_share.clone(),
            paillier_pk: self.paillier_pk().clone(),
            public_key: key.public_key.clone(),
            server_secret_share_encrypted: key.server_secret_share_encrypted.clone(),
        }
    }

    /// Get the current number of available R values for the given chain
    pub fn get_remaining_r_vals(&self, chain: &Blockchain) -> u32 {
        match chain {
            Blockchain::Ethereum | Blockchain::Bitcoin => self.k1_remaining.load(Ordering::Acquire),
            Blockchain::NEO => self.r1_remaining.load(Ordering::Acquire),
        }
    }

    /// Call after filling R values to update tracking
    pub fn fill_r_vals(&self, chain: Blockchain, n: u32) {
        match chain {
            Blockchain::Ethereum | Blockchain::Bitcoin => self.k1_remaining.fetch_add(n, Ordering::Release),
            Blockchain::NEO => self.r1_remaining.fetch_add(n, Ordering::Release),
        };
        tracing::info!("filled {:?}: +{}", chain, n);
    }

    fn decr_r_vals(&self, chain: Blockchain) {
        match chain {
            Blockchain::Ethereum | Blockchain::Bitcoin => self.k1_remaining.fetch_sub(1, Ordering::Release),
            Blockchain::NEO => self.r1_remaining.fetch_sub(1, Ordering::Release),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::Signer;

    #[test]
    fn test_signing() {
        let base64_key = "eyJjaGlsZF9rZXlzIjp7fSwKICAgICAgICAicGFpbGxpZXJfcGsiOnsibiI6IjU5ODdlNjIyMjYxY2FmOTZlMjU4MjZjNzBjZjMyM2IyNjE5NGZmOWNmZTY5ZTNmNDBmMzBkMzA2NTcxNjQyY2FlYThhMzE0M2QxMWZmOTRjMTM4ODM2MDQ4NjczNTdhZThjMGU2NjNiZjAzZDAwOTMwMTZkN2Y0ZDc5MGFlMjRlMjkxNzgwM2Q4MTJiNjQxYWYyZDZjMDk1NzNkMTEyZWI3Njg2NDY1MjkxY2QxNDZmZDY2MmY3N2Y1OTVlZjgzMjc3YmUxNjgwZDA0MGIxZjNjNDk5YzgxOTE3NTcyMDZlNTEwYWU1NDcyNGQ2NjdmYzA0MWEyYzdjMmZmM2QzYjY2YzM3MjlkYzI1ZTAyYzQwMTllZDNhMDEyZmQ3NWVjMGUwMzk0OGNmNzgzYWQzOTAyY2U1ZTVlNzIyMjljM2RkM2ExNGI5MzRkNjAyNjlhY2I3YmEwYmQ0MTVkMmRlMTI4ZWYxODcyMjQwMGJhZWEyZTg1MGU2ZDFmZDg3ODdhMDEzMGQ1MTYyMDZkNzE4YTQ5ZDdhMjFkNDI4YjBmYTM3NzMwNzliNjQ4NjE4MTExOTFiNTUwMDFkNGMyYzI5ZjYzMDMxNGJlMTkxY2YzY2EzZjBmOGUwOWVlMDk1NDNmZmRkYTNmOTdjZjE2OWQ1MmUwNjdjZmQ0MGNiMzAzOTQxIn0sCiAgICAgICAgInBheWxvYWRfcHVibGljX2tleSI6IjA0NjE2NDZmZGM0NTQ0ZjEwMjk0ZTIwZTk5NGNlNTZkOGMwZmY4NTI1OTZlYjZiM2FhMGJhOWQ0YjIwNzlkODZkNDJiM2I1ZTg0OTFhNDhmZjZlMTYyMDczMjU3OTgwNzkxNmVlYjA3YmViNmY5OTcwZGM1OTUyYmQ0NDQ0MDRmNzQiLAogICAgICAgICJwYXlsb2FkX3NpZ25pbmdfa2V5IjoiYmI4YmNmNTJhNWY5NDRmMzUxYzViYzg1NmI3YTRjNDFhNWYzNzBmNWNlOTlkY2UwYzhkNmYxZDQ5MWNkMzRiZiIsCiAgICAgICAgInZlcnNpb24iOjB9";
        let signer = Signer::from_data(&base64_key, "").unwrap();
        let signature = signer.sign_canonical_string("hello, world!");
        assert_eq!(signature.signed_digest, "30440220135a79b11caa321f1548d4b86e17c9b53525ffcdeab5e559d6cca310623cc45d02205a0bb368cf79e41d4760f48c9d16ccd8351ac5e97e0a6825eac6acbe662007c4");
    }
}
