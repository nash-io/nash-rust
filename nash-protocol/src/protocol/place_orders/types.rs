use crate::protocol::place_order::{LimitOrderRequest, MarketOrderRequest, PlaceOrderResponse};

#[derive(Clone, Debug)]
pub struct LimitOrdersRequest {
    pub requests: Vec<LimitOrderRequest>
}

#[derive(Clone, Debug)]
pub struct MarketOrdersRequest {
    pub requests: Vec<MarketOrderRequest>
}

#[derive(Clone, Debug)]
pub struct PlaceOrdersResponse {
    pub responses: Vec<PlaceOrderResponse>
}