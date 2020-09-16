use super::super::super::{DataResponse, ResponseOrError};
use super::types::{SubscribeOrderbook, SubscribeOrderbookResponse};
use crate::types::OrderbookOrder;
use crate::errors::Result;
use crate::graphql::updated_orderbook;

// FIXME: if possible, remove duplication with orderbook query
impl SubscribeOrderbook {
    pub fn response_from_graphql(
        &self,
        response: ResponseOrError<updated_orderbook::ResponseData>,
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
                        amount: self.market.asset_a.with_amount(&ask.amount.amount)?,
                    });
                }
                for bid in book.bids {
                    bids.push(OrderbookOrder {
                        price: bid.price.amount.to_string(),
                        amount: self.market.asset_a.with_amount(&bid.amount.amount)?,
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
