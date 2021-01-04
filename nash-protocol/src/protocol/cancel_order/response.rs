use super::types::CancelOrderResponse;
use crate::graphql::cancel_order;

impl From<cancel_order::ResponseData> for CancelOrderResponse {
    // This unwrap is safe. ME_FIXME
    fn from(response: cancel_order::ResponseData) -> Self {
        Self {
            order_id: response.cancel_order.order_id.unwrap(),
        }
    }
}
