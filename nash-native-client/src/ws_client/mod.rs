//! A client that supports the Nash protocol over websockets

mod absinthe;
mod client;
pub mod stream;

pub use client::Client;
pub(crate) use client::InnerClient;
