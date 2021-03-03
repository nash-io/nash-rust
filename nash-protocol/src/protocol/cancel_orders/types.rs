use crate::protocol::cancel_order::{CancelOrderRequest, CancelOrderResponse};

#[derive(Clone, Debug)]
pub struct CancelOrdersRequest {
    pub requests: Vec<CancelOrderRequest>
}

#[derive(Clone, Debug)]
pub struct CancelOrdersResponse {
    pub responses: Vec<CancelOrderResponse>
}