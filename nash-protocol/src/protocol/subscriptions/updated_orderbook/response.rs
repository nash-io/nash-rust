use super::super::super::{DataResponse, ResponseOrError};
use super::request::SubscribeOrderbook;
use crate::errors::Result;
use crate::graphql::updated_orderbook;
use crate::types::OrderbookOrder;
use crate::protocol::state::State;
use std::sync::Arc;
use futures::lock::Mutex;

/// Order book updates pushed over a subscription consist of a list of bid orders and
/// a list of ask orders.
#[derive(Clone, Debug)]
pub struct SubscribeOrderbookResponse {
    pub last_update_id: i64,
    pub update_id: i64,
    pub bids: Vec<OrderbookOrder>,
    pub asks: Vec<OrderbookOrder>,
}

// FIXME: if possible, remove duplication with orderbook query
impl SubscribeOrderbook {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<updated_orderbook::ResponseData>,
        state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<SubscribeOrderbookResponse>> {
        let state = state.lock().await;
        let market = state.get_market(&self.market)?;
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let book = response.updated_order_book;
                let mut asks = Vec::new();
                let mut bids = Vec::new();
                for ask in book.asks {
                    asks.push(OrderbookOrder {
                        price: ask.price.amount.to_string(),
                        amount: market.asset_a.with_amount(&ask.amount.amount)?,
                    });
                }
                for bid in book.bids {
                    bids.push(OrderbookOrder {
                        price: bid.price.amount.to_string(),
                        amount: market.asset_a.with_amount(&bid.amount.amount)?,
                    });
                }
                Ok(ResponseOrError::Response(DataResponse {
                    data: SubscribeOrderbookResponse {
                        update_id: book.update_id,
                        last_update_id: book.last_update_id,
                        asks,
                        bids,
                    },
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}
