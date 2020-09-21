#[macro_use]
extern crate lazy_static;

pub mod client;
pub mod common;
pub mod curves;
pub mod server;
pub use rust_bigint;
pub use paillier;

#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum ErrorKey {
    InvalidPublicKey,
}
