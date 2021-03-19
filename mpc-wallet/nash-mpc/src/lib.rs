#[macro_use]
extern crate lazy_static;

pub mod client;
pub mod common;
pub mod curves;
pub mod server;
pub use paillier_common;
pub use rust_bigint;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NashMPCError {
    #[error("DH secret keys and DH public keys have different lengths.")]
    DHlen,
    #[error("Randomness error.")]
    Random,
    #[error("Arithmetic scalar operation failed.")]
    ScalarArithmetic,
    #[error("Invalid scalar.")]
    ScalarInvalid,
    #[error("Paillier public key has not been verified.")]
    PaillierVerification,
    #[error("Invalid curve.")]
    CurveInvalid,
    #[error("Arithmetic point operation failed.")]
    PointArithmetic,
    #[error("Invalid point.")]
    PointInvalid,
    #[error("Signature verification failed.")]
    SignatureVerification,
    #[error("Invalid integer.")]
    IntegerInvalid,
    #[error("Proof verification failed.")]
    CorrectKeyVerification,
    #[error("Could not retrieve value from pool.")]
    Pool,
    #[error("Could not parse hex string as integer.")]
    HexString(#[from] rust_bigint::HexError),
}