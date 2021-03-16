use crate::graphql::cancel_order;
use crate::protocol::multi_request::{ResponseData, ResponseVisitor};
use serde::{Deserialize, Deserializer};

pub type CancelResponseData = ResponseData<cancel_order::ResponseData>;

impl<'de> Deserialize<'de> for CancelResponseData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de> {
        deserializer.deserialize_map(
            ResponseVisitor::<Self, cancel_order::ResponseData>::new("cancelOrder".to_string())
        )
    }
}