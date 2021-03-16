use std::marker::PhantomData;
use std::fmt;
use serde::de::{self, MapAccess, Visitor};
use serde_json::{Value, Map};
use serde::Deserialize;
use crate::protocol::multi_request::MultiResponse;

pub struct ResponseData<T> {
    pub responses: Vec<T>
}

impl<T, U: From<T>> From<ResponseData<T>> for MultiResponse<U> {
    fn from(response: ResponseData<T>) -> Self {
        let responses = response.responses.into_iter().map(|x| x.into()).collect();
        Self { responses }
    }
}

impl<T> From<Vec<T>> for ResponseData<T> {
    fn from(responses: Vec<T>) -> Self {
        Self { responses }
    }
}

pub struct ResponseVisitor<T, U> {
    pub field: String,
    response_data: PhantomData<T>,
    graphql_response_data: PhantomData<U>
}

impl<T, U> ResponseVisitor<T, U> {
    pub fn new(field: String) -> Self {
        Self {
            field,
            response_data: PhantomData,
            graphql_response_data: PhantomData
        }
    }
}

impl<'de, U: for <'a> Deserialize<'a>, T: From<Vec<U>>> Visitor<'de> for ResponseVisitor<T, U> {
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Expected multiple responses")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where A: MapAccess<'de> {
        let mut responses = Vec::new();
        while let Some((_, value)) = map.next_entry::<String, Value>()? {
            let mut object = Map::new();
            object.insert(self.field.clone(), value);
            let response = serde_json::from_value::<U>(Value::Object(object)).map_err(|e| {
                de::Error::custom(format!("Couldn't parse: {:#?}", e))
            })?;
            responses.push(response);
        }
        Ok(T::from(responses))
    }
}
