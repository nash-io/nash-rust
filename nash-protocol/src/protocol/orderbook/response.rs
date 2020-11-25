use super::super::{DataResponse, ResponseOrError};
use super::types::{OrderbookRequest, OrderbookResponse};
use crate::errors::Result;
use crate::graphql::get_orderbook;
use crate::types::OrderbookOrder;
use crate::protocol::state::State;
use std::sync::Arc;
use futures::lock::Mutex;


impl OrderbookRequest {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<get_orderbook::ResponseData>, state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<OrderbookResponse>> {
        let state = state.lock().await;
        let market = state.get_market(&self.market)?;
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let book = response.get_order_book;
                let mut asks = Vec::new();
                let mut bids = Vec::new();
                let update_id = book.update_id;
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
                    data: OrderbookResponse {
                        asks: asks,
                        bids: bids,
                        update_id
                    },
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}
