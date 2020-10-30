use super::super::{DataResponse, ResponseOrError};
use super::types::{TickerRequest, TickerResponse};
use crate::errors::Result;
use crate::graphql::get_ticker;
use crate::protocol::state::State;
use std::sync::Arc;
use futures::lock::Mutex;

impl TickerRequest {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<get_ticker::ResponseData>,
        state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<TickerResponse>> {
        // These unwraps are safe. ME_FIXME
        let state = state.lock().await;
        let market = state.get_market(&self.market)?;
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let ticker = response.get_ticker;
                let converted_ticker = TickerResponse {
                    id: ticker.id.clone(),
                    market_name: ticker.market_name.clone(),
                    a_volume_24h: market
                        .asset_a
                        .with_amount(&ticker.a_volume24h.amount)?,
                    b_volume_24h: market
                        .asset_b
                        .with_amount(&ticker.b_volume24h.amount)?,
                    high_price_24h: market
                        .asset_b
                        .with_amount(&ticker.high_price24h.unwrap().amount)?,
                    low_price_24h: market
                        .asset_b
                        .with_amount(&ticker.low_price24h.unwrap().amount)?,
                    last_price: market
                        .asset_b
                        .with_amount(&ticker.last_price.unwrap().amount)?,
                    price_change_24h: market
                        .asset_b
                        .with_amount(&ticker.price_change24h.unwrap().amount)?,
                    best_ask_amount: market
                        .asset_a
                        .with_amount(&ticker.best_ask_size.unwrap().amount)?,
                    best_ask_price: market
                        .asset_b
                        .with_amount(&ticker.best_ask_price.unwrap().amount)?,
                    best_bid_amount: market
                        .asset_a
                        .with_amount(&ticker.best_bid_size.unwrap().amount)?,
                    best_bid_price: market
                        .asset_b
                        .with_amount(&ticker.best_bid_price.unwrap().amount)?,
                };
                Ok(ResponseOrError::Response(DataResponse {
                    data: converted_ticker,
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}
