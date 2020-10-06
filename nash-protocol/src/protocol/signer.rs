use crate::errors::{ProtocolError, Result};
use crate::protocol::RequestPayloadSignature;
use crate::types::ApiKeys;
use crate::types::Blockchain;
use crate::types::PublicKey;
use crate::utils::{der_encode_sig, hash_message};
use nash_mpc::client::APIchildkey;
use nash_mpc::common::Curve;
use nash_mpc::curves::secp256_k1::Secp256k1Scalar;
use nash_mpc::curves::traits::ECScalar;
use nash_mpc::paillier_common;
use nash_mpc::rust_bigint::BigInt;

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
    k1_remaining: u32,
    r1_remaining: u32,
}

impl Signer {
    pub fn new(key_path: &str) -> Result<Self> {
        Ok(Self {
            api_keys: ApiKeys::new(key_path)?,
            k1_remaining: 0,
            r1_remaining: 0,
        })
    }

    pub fn from_data(secret: &str, session: &str) -> Result<Self> {
        Ok(Self {
            api_keys: ApiKeys::from_data(secret, session)?,
            k1_remaining: 0,
            r1_remaining: 0,
        })
    }

    /// Sign GraphQL payload request with secp256k1 via payload signing key
    /// The output is a hex string where signature has been DER encoded
    pub fn sign_canonical_string(&self, request: &str) -> RequestPayloadSignature {
        let message_hash = hash_message(request);
        let signing_key: Secp256k1Scalar = ECScalar::from(&self.api_keys.keys.payload_signing_key);
        let (r, s) = signing_key.sign(&message_hash);
        let sig = der_encode_sig(&r, &s);
        RequestPayloadSignature {
            signed_digest: hex::encode(sig),
            public_key: self.request_payload_public_key(),
        }
    }

    /// Sign data hashed to `BigInt` with the MPC child key for the given `Blockchain`
    pub fn sign_child_key(
        &mut self,
        data: BigInt,
        chain: Blockchain,
    ) -> Result<(BigInt, BigInt, String)> {
        if self.remaining_r_vals(chain) <= 0 {
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
        self.decrement_r_val(chain);
        Ok((sig, r, key.public_key))
    }

    /// Get public key for child key on `chain`
    pub fn child_public_key(&self, chain: Blockchain) -> Result<PublicKey> {
        PublicKey::new(chain, &self.get_child_key(chain).public_key)
    }

    /// Return public key for payload signing in format expected by the Nash backend service
    /// BigInt conversion to hex will stip leading zeros, which Nash backedn doesn't like
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

    /// Call after filling R values to update tracking
    pub fn fill_r_vals(&mut self, chain: Blockchain, n: u32) {
        match chain {
            Blockchain::Ethereum | Blockchain::Bitcoin => self.k1_remaining = n,
            Blockchain::NEO => self.r1_remaining = n,
        }
    }

    fn decrement_r_val(&mut self, chain: Blockchain) {
        match chain {
            Blockchain::Ethereum | Blockchain::Bitcoin => self.k1_remaining -= 1,
            Blockchain::NEO => self.r1_remaining -= 1,
        }
    }

    pub fn remaining_r_vals(&self, chain: Blockchain) -> u32 {
        match chain {
            Blockchain::Ethereum | Blockchain::Bitcoin => self.k1_remaining,
            Blockchain::NEO => self.r1_remaining,
        }
    }
}
