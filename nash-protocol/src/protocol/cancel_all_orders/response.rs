use super::types::CancelAllOrdersResponse;
use crate::graphql::cancel_all_orders;

impl From<cancel_all_orders::ResponseData> for CancelAllOrdersResponse {
    // This unwrap is safe. ME_FIXME
    fn from(response: cancel_all_orders::ResponseData) -> Self {
        Self {
            accepted: response.cancel_all_orders.accepted.unwrap(),
        }
    }
}
