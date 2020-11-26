use super::super::{DataResponse, ResponseOrError};
use super::types::{OrderbookRequest, OrderbookResponse};
use crate::errors::Result;
use crate::graphql::get_orderbook;
use crate::types::OrderbookOrder;
use crate::protocol::state::State;
use std::sync::Arc;
use futures::lock::Mutex;
use bigdecimal::BigDecimal;
use std::str::FromStr;


impl OrderbookRequest {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<get_orderbook::ResponseData>, _state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<OrderbookResponse>> {
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let book = response.get_order_book;
                let mut asks = Vec::new();
                let mut bids = Vec::new();
                let update_id = book.update_id;
                let last_update_id = book.last_update_id;
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
                    data: OrderbookResponse {
                        asks,
                        bids,
                        update_id,
                        last_update_id
                    },
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}
