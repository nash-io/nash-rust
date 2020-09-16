//! Retrieve current asset nonces from Nash server. Asset nonces
//! allow the smart contract to reason about whether a payload such
//! as a sync state or fill order request is valid in relation to
//! previous activity. Generally, only payloads with a higher nonce
//! than the last seen nonce for an asset are considered valid. For
//! fill order payloads, the nonce must equal the current nonce.
pub mod request;
pub mod response;
pub mod types;
