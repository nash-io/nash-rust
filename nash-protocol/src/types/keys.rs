//! This module manages the `ApiKeys` type and its general set of functionality,
//! including parsing keys from a JSON file, signing request payloads, and signing
//! blockchain data with MPC child keys.

use crate::errors::{ProtocolError, Result as ProtocolResult};
pub use crate::types::Blockchain;
use nash_mpc::paillier_common;
use nash_mpc::rust_bigint::BigInt;
use serde::de::{Deserializer, Error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Structure of data returned by key creation on nash.io
#[derive(Deserialize, Debug)]
pub struct ApiKeys {
    /// Base64 encoding of key data json object
    #[serde(rename = "secret")]
    #[serde(deserialize_with = "secrets_from_base64")]
    pub keys: KeyMap,
    /// Link to session associated with API key
    #[serde(rename = "apiKey")]
    pub session_id: String,
}

impl ApiKeys {
    pub fn new(path: &str) -> ProtocolResult<Self> {
        let key_str = std::fs::read_to_string(path)
            .map_err(|_| ProtocolError("Could not read API key file"))?;
        let keys: Self = serde_json::from_str(&key_str)
            .map_err(|_| ProtocolError("Could not serialize API key file to keys"))?;
        Ok(keys)
    }

    pub fn from_data(secret: &str, session: &str) -> ProtocolResult<Self> {
        let decoded = base64::decode(secret)
            .map_err(|_| ProtocolError("Could not decode secret string as Base64"))?;
        let decoded_str = std::str::from_utf8(&decoded)
            .map_err(|_| ProtocolError("Could not encode decoded base64 as string (impossible)"))?;
        println!("Decoded: {}", decoded_str);
        let keys: KeyMap = serde_json::from_str(decoded_str)
            .map_err(|_| ProtocolError("Could not convert secret string value to KeyMap"))?;
        Ok(ApiKeys {
            keys,
            session_id: session.to_string(),
        })
    }

    pub fn version(&self) -> u32 {
        self.keys.version
    }
}

fn secrets_from_base64<'de, D>(deserializer: D) -> Result<KeyMap, D::Error>
where
    D: Deserializer<'de>,
{
    let secret: &str = Deserialize::deserialize(deserializer)?;
    let decoded_secret = base64::decode(secret).map_err(D::Error::custom)?;
    let decoded_string = std::str::from_utf8(&decoded_secret).map_err(D::Error::custom)?;
    serde_json::from_str(&decoded_string).map_err(D::Error::custom)
}

/// Child key representation for API key file
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyfileChildKey {
    pub address: String,
    pub public_key: String,
    #[serde(with = "nash_mpc::rust_bigint::serialize::bigint")]
    pub client_secret_share: BigInt,
    #[serde(with = "nash_mpc::rust_bigint::serialize::bigint")]
    pub server_secret_share_encrypted: BigInt,
}

/// Decoding of key list base64 data
#[derive(Deserialize, Debug)]
pub struct KeyMap {
    pub child_keys: HashMap<String, KeyfileChildKey>,
    pub paillier_pk: paillier_common::EncryptionKey,
    #[serde(with = "nash_mpc::rust_bigint::serialize::bigint")]
    pub payload_public_key: BigInt,
    #[serde(with = "nash_mpc::rust_bigint::serialize::bigint")]
    pub payload_signing_key: BigInt,
    pub version: u32,
}

#[cfg(test)]
mod tests {
    use super::ApiKeys;
    use serde_json::Result;
    use std::fs;

    fn get_keyfile() -> String {
        fs::read_to_string(&"../nash-native-client/test_data/keyfile.json".to_string()).unwrap()
    }

    #[test]
    fn keyfile_parse() {
        let keyfile: Result<ApiKeys> = serde_json::from_str(&get_keyfile());
        assert!(keyfile.is_ok());
    }

    #[test]
    fn from_data() {
        let secret = "eyJjaGlsZF9rZXlzIjp7Im0vNDQnLzAnLzAnLzAvMCI6eyJhZGRyZXNzIjoiMk41ZEVvZ3JvYWpLS28xQzM0ck5KUHZWNWZSV2tVUzRMa3ciLCJjbGllbnRfc2VjcmV0X3NoYXJlIjoiNjQwYzc4MmEzNDIxNzdiMTE1Y2I2MzAzNzZmNWExYWJkMTYwMjM0OGQ5YmUzNmQ1NjcxNTUwMDBiZGM0YjNjZiIsInB1YmxpY19rZXkiOiIwM2Q2ZDIxZGEyYzUyMmVhYWM2MmM2MzIwOWEyODBkNWU3ZWM0ZTgxZDUzODVjYzZlODJhZmVlNDU3MjQyZjAyMmUiLCJzZXJ2ZXJfc2VjcmV0X3NoYXJlX2VuY3J5cHRlZCI6ImE1Y2JmNDRhODQ2YzI3NzQzODNkZTJmOTMxOTVmYjFjOTcwNGRjNWYxMmI4NTEwNmU3Y2QyYmMyMGVlMzE3YzkxN2NjN2E2MTZmODRlMDA1YWZiNDhhMjkwNjFjNzA3MzUwZDQwZjE3NjcxNDJmOGRiOTYzMGVmNTY3OGQ3NWEzYWVlNDYxZDA3NDliZGZkYWFhOTAwZjA5NGVmOTZkODMxMTgyZjVjYmRmZjQ4YWExMjlkMWU2ODY3OWZkYzg0OGJiY2VhNmNmZjBmMjZiMmRiYWMyYzI5NjUxODcxZGIzMDNjZWM5MmU5MmIxNTM3NTE1ZDBmMjQ2ODJhOWQ5YjliYmE3YWJkMDg1OGZmNDU1MTkwMTAyMzczMGU3YmE5YTQxZTc3ZDBhMmRmZTZiOTdmN2NlNDAxOGVlMzMyOWIyNzg1M2NmZWFjZDI3M2VlNTNiMTU0Y2UzZWMxMmY4ODAzNDQ3YjM5ODg3MjgyNmY0MTg4NmU4YWZjZTJmYzFjNjQzYTY1NjE0ZTYzNDg1ZGZkOTU2NmU2OGVhNjQyNjAwMjJmYzdkYzYxMjEzMDRiNzRmNDgxOGQyODIyM2YyNTA2MzQwNzJiOWVkODY5YTc0YmJjMGQwMjgyMDY0YWQ0MzZjZWFjM2QxMTY2NWQ4NDRhM2Q1NzU0OTY3MjY3ZWM2ZDllOGMzNTNjNjg2ZjFkMDE1NjM3N2M2ZWY1YWJkN2Q5Y2Y2MmNmMTIzZmUwNTFhYzExOTliMjUyMjI3MTg2MGI2MzEzNzUyNWQ0NzE0NDZiMzQzMmQwMmQ2YzU4MDhmZGNiMzRiNDM2NzJjZGEzZGU2Y2E0MjljZDJkZDUxMDg5ZjU1ZDk2NDczNzQzNGRjMmIxNGI3NDI0ODY0MmE1ZjdkMWJjNzQ1ZWExZWQzOTAwZjZmYzk0ZThjNGFiMDY2ZDNiMDcxZjk4YmI0NGEwMTAwYzAwODU5YzQ5NmE2ZTdjZjliYTk1YWYyOWVhYzdkMGE0ODNjMmU5ZGI4MjQzM2NlMDQ3Y2U1MjJlMTNlM2RiOTUzODg1MzExNTYwOGQ0OTgzODBiMGFmZThmMjg0NTY0NjYxMTZkY2MyM2FlNDBmOWVhMGNmNjRkNTZkODE1OGEyNWI1OTMwMDdkNzA0NzM0NzE2Njk1MmFkMzk2ZmQyYTFiOTQ5Y2RjYzZkOWU2OTBhNDg5NjNjM2FkYTI2NTQ2M2JjNTQ0ZDQ0ZWJmZmM0MzQ1YmVhYWI0OWU1ZWFjM2I4MjQzMzM3ZTI4NjMxNjYwZGVjZGE1NDZkOTgyNDU2OGM2YTQxNmRlNGU0NjJkY2E1YTA1MzE4NTYxODc1OTM2Y2FmNGViM2I0NmQ0NyJ9LCJtLzQ0Jy82MCcvMCcvMC8wIjp7ImFkZHJlc3MiOiJlY2Y5OWYyZTY5Y2UzMjQ0ZDFmZTRjYWRhMzRiZjU0Njg0YWNmYWZlIiwiY2xpZW50X3NlY3JldF9zaGFyZSI6ImQ4Yzk1MGNkY2E0OGEwNjgwZTE4Y2JlNmU0Njc5YzgzNWNlMmNhM2RlYzBhZjZmNWFmYmVmMjNmN2QyMWVhYmEiLCJwdWJsaWNfa2V5IjoiMDQ4ZWI5YTI5YzI1MzM5Y2FjODM5OGYyM2E4MTg2MDg5ODM4MDljMTg5OTBkMjhjNTUwMmNiYjNhN2E1ZjU1NThiNTQ3NjZhNDY1NzczNzQ5Yjg3NDVhNjE1YTQ0MDc3NWFmOTE3MWM5N2NjMTIwN2IzYWMxNDVlNTBhY2E5MDRjOSIsInNlcnZlcl9zZWNyZXRfc2hhcmVfZW5jcnlwdGVkIjoiMWI2NWY4ODY2NmRkMTU5ZTZmMWJiMTkxNTg3MTQ2Mjg4YzlkZTJlZTRiZjA5MDYzYWNhMTdlOGQwMWVlYmU3YzM3NDJiNTM0OTIxY2I3MTgwOGFjYjk3ZWFiMjRiOTgxZmJjODA5Y2I4YTNhN2M4OGJkNWU1NmQwYTYxM2Q3MDYzM2ZjOGQ5ZmE5MzZiYWIwOWUyYWNmYzg0NzYwNTBiYTU0NTY5NDc2NWE0ZmQ5Mjg5MWU4ZGFkMDZhMWFkYTcyYjczNDc3MmQxZjQzZjFhYmE2ODA4YzQyODIwYjEwMjRiYmM2YThiMDE0YjNhYzUxMDBkZDAzZDNhYjRhMTY4N2IyNjQ2OGU1MDU2MTFmZDMwMDA2ZmQwMjFkNTIzOTBkYzdjMjNkYmZlYmFkNjMxNzIxNTZlZjRjZjJjMjQzZjc5ZWYyNjJkNjU2ZWIyYWEyY2YwYTlmM2QzYWE1MjAyMzRjYjhmYmI1MGM1NmUzMzE2ZmE0NTZmN2FiY2FhOTk3ZTZiYTM4NzM5ZjJiZGFmYWQzNzllNTVlYTU1YmEyZGRiY2VlMmQ1MDIxZWY3MzUzY2I1NTk3ZmNhMmEzODQwNzdiMWFhYjYwZjIxMjY1NjIzMzA1NTQ4MWYwMDlmNmJmMmY4ZTEwMGViNGQ0YzQ5ODMwNTZmOGRjNGZkMTVmNzVjZWE4NDlkNjY1M2M2NjdkMjhjYzExZThjZWRhZmU2OTQwNTkyZGIwN2NhOWU2MTVkMDJjYjgxNTBlMmFiZjVhNGIyY2UwOWFmODhmYTQ1MGNjYTM1OWZjYzY0MzVjYzBlNGY1YTcyMDA5MjdmMGU0ZGNhZjM3NzUzOWZlNzgyMDk4N2NkNGY4ZjFlNzExZGUwN2VmOTYxNGI3MTU1MWVkYzJhMTJiMmQ4M2M5M2NhZmQ0YThiNTMxODQwODY1MTk0NDQ3ZGIwNGM3NzU2NzQ0YjU0NGFlMGEwYTA2OTYyMmY4MDY1MGEzY2RhZWNmOWQ0Mzc5NWJkNTU3MjMxNTM3M2QyYTg1Y2JmYjRlYjNiMzIzZDlhZTgxOGRkNWZjMTE0MjljNzhlNTIwM2I2MjhhYjI2MTQ4NWNlMjQ3NzJiNWU4Y2MxZDY0ODU4Zjg3MzhhOTQ0ZDg1ZmNjZDA2YTI4NThkMWQ3MmU1YWU4OTBkNDBjZmYwMWQ5OTk1NzAxY2VkNGJiZTEwOGFlNjAwMTE3MWNlOWJkYjg2NTAyZGU2YWVlZjdiNDcxNmMyOTY5ZDBlMzgyNjRiMTU0YzYyMWRkN2NiZGQxYmQzYjllYzlmNWMzMDJiN2ZiZDcyNDAzYThiNGE3N2NjNGI0NjhmZjk3MjA3ZGY3YzllNmRkMTU1OCJ9LCJtLzQ0Jy84ODgnLzAnLzAvMCI6eyJhZGRyZXNzIjoiQVZLSlFUdG9TZVlHVkVIaU1pTHZnOHZUeWpVZnBudjU3cSIsImNsaWVudF9zZWNyZXRfc2hhcmUiOiJmM2U5MTdkMTViNjE4ZmVkYWU3ZWZhYmE2MmVmMGViZmJjNTFkODU2OTEzMDAwNTk4YWNjOTJiMDRlY2M5N2Y2IiwicHVibGljX2tleSI6IjAyYzBlZWI3NWFjZmM4NTYzYTUzNjYwODJhZWRmZGQzY2YwZjUyMmM0ZmYwM2EyMTFjYmE3MzgyNzUyNTg3ZTI2OSIsInNlcnZlcl9zZWNyZXRfc2hhcmVfZW5jcnlwdGVkIjoiN2M4NTcxOWJiNzRhNGJiZjMwYjk5YTQ0ZTJiYzViNjYwNzUxODkwZjA4ZWMxYmQzMDVjODkxNDU1ZTRkYTgzZGY1NGE5NDQ3ZWM5MGViN2RmNTk1NmY4OTAzMTNjYWExOGI4ODA5YTVlY2E0ODcyNDA3Y2RlZDMzMWUxZDU4NmY0YWM4ZGY1OGRiNjMyMDA3YWU3ODQ2M2E2OTJlOTJhYzI5YjgzZmNmODJjZDZjMDliNzQ5OTNkZjQ1YTlmMjAzZTc3ZjA2MjIxMzU5OGY2MGUzNDA5MjlkYTRkNmJiMGI4ZDNlNzQ5YjM3YzljNGEyYzdlMWU2MGFmZGY4MzVmOTZmMTAwZmE0NjZiMzE2Y2YyMGI5ZDE3ZmM4NTZkYTY1ZGE4OGQ1YmExMjk1ZTY0NDg0MzUxMjNiNjIyMTQ5OTE4Y2E4ZGE0ODhjMGU4MDdjYjQzNGQ3YzQzZjQ5MTMzYjgwOGNhMzc5YzVlNzEwNWEyZjg5OTQyNjlhNThmMzUyMmVlOTIxOTI3YjBmNWE0OWRmNTcyOTk0M2ZhNzMxMzJjMWYyODY2MWUyN2M2ZmIwNDgwZDdmMTYwMzc4MDQxMzcxNDU5OGJmNmFmYzg2NzRlYzE0Y2YzMjg0NDc1NTkxYTY1YjZjOGU5M2VmNzUwMTFhYTcxMDYwODg2MmM1YmE4ZTg4YzFiYjlhNGZhNTc5YmJmM2I5MDIyNzgwMDkxY2RmN2I5ZGI4ZTQyODE0MDA3YjVkZGU1YzdjYmQ1OGFjYmVmOTJjMThhZWY1OTY4ZmUyODUxNjJjNDQ1MGRmN2FjMjIyMGMxMzYwMTA1YzU5ZWFhOWUyNTE0ZDkxNDk1OTlhOTVhNDFjOTViNTczYmY3NzhjOGIzMTBhOTE3ZmExZmQ5NGI5MmVjZDIzNTQ5MTE4ZDgzYWQ0MjcwNzAyZjhmNjk5ZjgyNjUzMTk0NTlmZWM5NDAwYTE0Mjg0NzkzODU0ZjUwNTNkMmNlMWU5M2FjNjc1ZjVlYmQzM2NkZjdlMGIzYjJkMTM0ZGVlYzViOWEyYjhkZmNmZDhkOTFhMjdkNzhhMDdhMmFlYTEyZGIzZGU5MTI4OTE4ZjRkMGRkODBkN2E2ZGM2MzM2YmI0MWJiZTkxYjhhZDM1NTMyYWY4MjU3ODEwMTgxYWI4NDYxNTZmMjRjN2MwZjM3NWM0YWViMWYyNmJjN2E2YTliMjRjZjNmNTI0ODk2ZTM4ZmMzN2UwOGE3OWQ4ZjRkYzdhYmJiN2QzNmFmNDBlODg5OGE1NjA4ZmNmY2QyNmVkN2ExYjRlOWZiZWE4YzYxOGEzZTc3NTg5MDQ3NDEzMjNlOWYzMjBjYTM0MzI4NzkxMDE3In19LCJwYWlsbGllcl9wayI6eyJuIjoiNTk4N2U2MjIyNjFjYWY5NmUyNTgyNmM3MGNmMzIzYjI2MTk0ZmY5Y2ZlNjllM2Y0MGYzMGQzMDY1NzE2NDJjYWVhOGEzMTQzZDExZmY5NGMxMzg4MzYwNDg2NzM1N2FlOGMwZTY2M2JmMDNkMDA5MzAxNmQ3ZjRkNzkwYWUyNGUyOTE3ODAzZDgxMmI2NDFhZjJkNmMwOTU3M2QxMTJlYjc2ODY0NjUyOTFjZDE0NmZkNjYyZjc3ZjU5NWVmODMyNzdiZTE2ODBkMDQwYjFmM2M0OTljODE5MTc1NzIwNmU1MTBhZTU0NzI0ZDY2N2ZjMDQxYTJjN2MyZmYzZDNiNjZjMzcyOWRjMjVlMDJjNDAxOWVkM2EwMTJmZDc1ZWMwZTAzOTQ4Y2Y3ODNhZDM5MDJjZTVlNWU3MjIyOWMzZGQzYTE0YjkzNGQ2MDI2OWFjYjdiYTBiZDQxNWQyZGUxMjhlZjE4NzIyNDAwYmFlYTJlODUwZTZkMWZkODc4N2EwMTMwZDUxNjIwNmQ3MThhNDlkN2EyMWQ0MjhiMGZhMzc3MzA3OWI2NDg2MTgxMTE5MWI1NTAwMWQ0YzJjMjlmNjMwMzE0YmUxOTFjZjNjYTNmMGY4ZTA5ZWUwOTU0M2ZmZGRhM2Y5N2NmMTY5ZDUyZTA2N2NmZDQwY2IzMDM5NDEifSwicGF5bG9hZF9wdWJsaWNfa2V5IjoiMDMxN2E2ODkyMWFmMzkxZWI3YTk3YzVmYTI3MzMwZTUzOTA2MzY4ZWNmZTFjZTVjZTcyMGYwODcxYzQyNzc0NGNhIiwicGF5bG9hZF9zaWduaW5nX2tleSI6IjQ2NjkxN2FhOGRkMWFlMjJjMzUwYTJlOWZlOGYzY2Q5MmMwMTdlODc5YmI1NDEyN2RkMzM0ZWZjNzU2OGEyODAiLCJ2ZXJzaW9uIjowfQ";
        let session = "9a490222-89b6-4403-9539-ae77e4c5c08e";
        let keys = ApiKeys::from_data(secret, session).unwrap();
        println!("{:?}", keys);
    }
}
