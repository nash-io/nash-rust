use super::super::super::{DataResponse, ResponseOrError};
use super::request::SubscribeOrderbook;
use crate::errors::Result;
use crate::graphql::updated_orderbook;
use crate::types::OrderbookOrder;
use crate::protocol::state::State;
use std::sync::Arc;
use futures::lock::Mutex;
use bigdecimal::BigDecimal;
use std::str::FromStr;

/// Order book updates pushed over a subscription consist of a list of bid orders and
/// a list of ask orders.
#[derive(Clone, Debug)]
pub struct SubscribeOrderbookResponse {
    pub bids: Vec<OrderbookOrder>,
    pub asks: Vec<OrderbookOrder>,
}

// FIXME: if possible, remove duplication with orderbook query
impl SubscribeOrderbook {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<updated_orderbook::ResponseData>,
        _state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<SubscribeOrderbookResponse>> {
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let book = response.updated_order_book;
                let mut asks = Vec::new();
                let mut bids = Vec::new();
                for ask in book.asks {
                    asks.push(OrderbookOrder {
                        price: ask.price.amount.to_string(),
                        amount: BigDecimal::from_str(&ask.amount.amount)?,
                    });
                }
                for bid in book.bids {
                    bids.push(OrderbookOrder {
                        price: bid.price.amount.to_string(),
                        amount: BigDecimal::from_str(&bid.amount.amount)?,
                    });
                }
                Ok(ResponseOrError::Response(DataResponse {
                    data: SubscribeOrderbookResponse {
                        asks: asks,
                        bids: bids,
                    },
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}
