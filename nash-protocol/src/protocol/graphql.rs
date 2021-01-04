//! Share protocol logic for structuring GraphQL requests and parsing responses

use super::state::State;
use super::traits::TryFromState;
use crate::errors::{ProtocolError, Result};
use futures::lock::Mutex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fmt::Display;
use std::sync::Arc;

//****************************************//
//  GraphQL response parsing              //
//****************************************//

/// Helper to convert JSON to a response or error
pub fn json_to_type_or_error<T: DeserializeOwned>(
    response: serde_json::Value,
) -> Result<ResponseOrError<T>> {
    let string_val = serde_json::to_string(&response)
        .map_err(|_| ProtocolError("Unexpected problem serializing JSON to string"))?;
    Ok(serde_json::from_str(&string_val)
        .map_err(|_| ProtocolError("Could not convert JSON to T"))?)
}

pub fn serializable_to_json<T: Serialize>(obj: &T) -> Result<serde_json::Value> {
    let str_val = serde_json::to_string(obj)
        .map_err(|_| ProtocolError("Unexpected problem serializing T to string"))?;
    Ok(serde_json::from_str(&str_val)
        .expect("Transforming to JSON failed. This should never happen"))
}

/// Helper to convert data corresponding to raw GraphQL types (B) into
/// nicer library managed types A when failure is possible
pub fn try_response_from_json<A, B>(response: serde_json::Value) -> Result<ResponseOrError<A>>
where
    A: TryFrom<B>,
    <A as TryFrom<B>>::Error: Display,
    B: DeserializeOwned,
{
    let parse_graphql = json_to_type_or_error::<B>(response)?;
    // Maybe we get back a parsed result from server response
    let mapped = parse_graphql.map(Box::new(|data| data.try_into()));
    // Unpacking the inner state is annoying, but need to surface the conversion error if encountered
    match mapped {
        ResponseOrError::Response(DataResponse { data }) => match data {
            Ok(data) => Ok(ResponseOrError::from_data(data)),
            Err(err) => Err(ProtocolError::coerce_static_from_str(&format!("{}", err))),
        },
        ResponseOrError::Error(e) => Ok(ResponseOrError::Error(e)),
    }
}

/// Helper to convert data corresponding to raw GraphQL types (B) into
/// nicer library managed types A when failure is possible
pub async fn try_response_with_state_from_json<A, B>(
    response: serde_json::Value,
    state: Arc<Mutex<State>>,
) -> Result<ResponseOrError<A>>
where
    A: TryFromState<B>,
    B: DeserializeOwned,
{
    let parse_graphql = json_to_type_or_error::<B>(response)?;
    // Maybe we get back a parsed result from server response
    match parse_graphql {
        ResponseOrError::Response(DataResponse { data }) => {
            let conversion = TryFromState::from(data, state.clone()).await;
            match conversion {
                Ok(data) => Ok(ResponseOrError::from_data(data)),
                Err(err) => Err(ProtocolError::coerce_static_from_str(&format!("{}", err))),
            }
        }
        ResponseOrError::Error(e) => Ok(ResponseOrError::Error(e)),
    }
}

pub enum ErrorOrData<T> {
    Data(T),
    Error(ErrorResponse),
}

/// Wrapper type to account for GraphQL errors
#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub enum ResponseOrError<T> {
    Response(DataResponse<T>),
    Error(ErrorResponse),
}

impl<T> ResponseOrError<T> {
    /// Build from data
    pub fn from_data(data: T) -> Self {
        Self::Response(DataResponse { data })
    }

    // TODO: why can't the enum look like this?
    pub fn consume_match(self) -> ErrorOrData<T> {
        match self {
            Self::Response(DataResponse { data }) => ErrorOrData::Data(data),
            Self::Error(e) => ErrorOrData::Error(e),
        }
    }

    /// Get response from wrapper if it exists
    pub fn response(&self) -> Option<&T> {
        match self {
            Self::Response(DataResponse { data }) => Some(data),
            Self::Error(_) => None,
        }
    }
    /// Get response from wrapper if it exists, will destroy wrapper
    pub fn consume_response(self) -> Option<T> {
        match self {
            Self::Response(DataResponse { data }) => Some(data),
            Self::Error(_) => None,
        }
    }
    pub fn consume_error(self) -> Option<ErrorResponse> {
        match self {
            Self::Response(_) => None,
            Self::Error(e) => Some(e),
        }
    }
    /// Get response or else error
    pub fn response_or_error(self) -> Result<T> {
        match self {
            Self::Response(DataResponse { data }) => Ok(data),
            Self::Error(e) => Err(ProtocolError::coerce_static_from_str(&format!("{:?}", e))),
        }
    }
    /// Get error from wrapper if it exists
    pub fn error(&self) -> Option<&ErrorResponse> {
        match self {
            Self::Response(_) => None,
            Self::Error(error) => Some(error),
        }
    }
    /// Is the response a GraphQL error?
    pub fn is_error(&self) -> bool {
        match self {
            Self::Error(_) => true,
            Self::Response(_) => false,
        }
    }
    /// Transform the inner value of a valid response. Will not transform an error
    pub fn map<M>(self, f: Box<dyn Fn(T) -> M>) -> ResponseOrError<M> {
        match self {
            Self::Error(e) => ResponseOrError::Error(e),
            Self::Response(r) => ResponseOrError::Response(r.map(f)),
        }
    }
}

/// Inner wrapper on valid GraphQL response data
#[derive(Deserialize, Serialize, Debug)]
pub struct DataResponse<T> {
    pub data: T,
}

impl<T> DataResponse<T> {
    fn map<M>(self, f: Box<dyn Fn(T) -> M>) -> DataResponse<M> {
        DataResponse { data: f(self.data) }
    }
}

/// Inner wrapper on error GraphQL response data
#[derive(Deserialize, Serialize, Debug)]
pub struct ErrorResponse {
    pub errors: Vec<Error>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Error {
    pub message: String,
}

//****************************************//
//  Common blockchain types               //
//****************************************//

/// This signature format is used for GraphQL request payload signatures
/// Data is SHA256 hashed and signed with a secp256k1 ECDSA key
#[derive(Clone, Debug)]
pub struct RequestPayloadSignature {
    pub signed_digest: String,
    pub public_key: String,
}

impl RequestPayloadSignature {
    pub fn empty() -> Self {
        Self {
            signed_digest: "".to_string(),
            public_key: "".to_string(),
        }
    }
}
