use crate::errors::{Result, ProtocolError};
use std::convert::{TryFrom, TryInto};
use crate::protocol::GraphQLResponse;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct MultiRequest<T> {
    pub requests: Vec<T>
}

impl<T> MultiRequest<T> {
    pub fn new(requests: Vec<T>) -> Result<Self> { Ok(Self { requests })}
}

/// A helper type for constructing blockchain payloads and GraphQL requests
pub struct MultiRequestConstructor<T> {
    pub constructors: Vec<T>
}

#[derive(Clone, Debug)]
pub struct MultiResponse<T> {
    pub responses: Vec<Result<T>>
}

impl<T> From<Vec<Result<T>>> for MultiResponse<T> {
    fn from(responses: Vec<Result<T>>) -> Self {
        Self { responses }
    }
}

impl<T: for<'de> Deserialize<'de>> TryFrom<serde_json::Value> for MultiResponse<T> {
    type Error = ProtocolError;
    fn try_from(response: serde_json::Value) -> Result<Self> {
        let response: GraphQLResponse = response.try_into()?;
        let errors: Vec<_> = response
            .errors
            .into_iter()
            .map(|error| (
                ProtocolError::coerce_static_from_str(&error.message),
                error
                    .path
                    .first()
                    .expect("Couldn't get path.")
                    .clone()
            )).collect();

        let mut responses: Vec<_> = response
            .data
            .into_iter()
            .collect();
        responses.sort_by(|(a, _), (b, _)| a.cmp(b));
        let responses = responses
            .into_iter()
            .map(|(path, value)| {
                if value.is_null() {
                    Err(errors
                        .iter()
                        .find(|(_, error_path)| path == *error_path)
                        .map(|(message, _)| message.clone())
                        .unwrap_or_else(|| ProtocolError("Couldn't find error message.")))
                } else {
                    let response: Result<T> = serde_json::from_value(value)
                        .map_err(|e|
                            ProtocolError::coerce_static_from_str(
                                &format!("Couldn't parse response: {:#?}", e)
                            )
                        );
                    response
                }
            })
            .collect();
        Ok(Self { responses })
    }
}
