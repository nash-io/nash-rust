use super::super::{DataResponse, ResponseOrError};
use super::types::{OrderbookRequest, OrderbookResponse};
use crate::types::OrderbookOrder;
use crate::errors::Result;
use crate::graphql::get_orderbook;

impl OrderbookRequest {
    pub fn response_from_graphql(
        &self,
        response: ResponseOrError<get_orderbook::ResponseData>,
    ) -> Result<ResponseOrError<OrderbookResponse>> {
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let book = response.get_order_book;
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
                    data: OrderbookResponse {
                        asks: asks,
                        bids: bids,
                    },
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}
