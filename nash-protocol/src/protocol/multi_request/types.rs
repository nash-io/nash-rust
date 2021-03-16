use crate::errors::Result;

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
    pub responses: Vec<T>
}

impl<T> From<Vec<T>> for MultiResponse<T> {
    fn from(responses: Vec<T>) -> Self {
        Self { responses }
    }
}
