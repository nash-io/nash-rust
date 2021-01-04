use super::super::{DataResponse, ResponseOrError};
use super::types::{TickerRequest, TickerResponse};
use crate::errors::Result;
use crate::graphql::get_ticker;
use crate::protocol::state::State;
use bigdecimal::BigDecimal;
use futures::lock::Mutex;
use std::str::FromStr;
use std::sync::Arc;

impl TickerRequest {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<get_ticker::ResponseData>,
        _state: Arc<Mutex<State>>,
    ) -> Result<ResponseOrError<TickerResponse>> {
        // These unwraps are safe. ME_FIXME
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let ticker = response.get_ticker;

                let high_price_24h = match &ticker.high_price24h {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };
                let low_price_24h = match &ticker.low_price24h {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };
                let last_price = match &ticker.last_price {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };
                let price_change_24h = match &ticker.price_change24h {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };
                let best_ask_amount = match &ticker.best_ask_size {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };
                let best_ask_price = match &ticker.best_ask_price {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };
                let best_bid_amount = match &ticker.best_bid_size {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };
                let best_bid_price = match &ticker.best_bid_price {
                    Some(price) => Some(BigDecimal::from_str(&price.amount)?),
                    None => None,
                };

                let converted_ticker = TickerResponse {
                    id: ticker.id.clone(),
                    market_name: ticker.market_name.clone(),
                    a_volume_24h: BigDecimal::from_str(&ticker.a_volume24h.amount)?,
                    b_volume_24h: BigDecimal::from_str(&ticker.b_volume24h.amount)?,
                    high_price_24h,
                    low_price_24h,
                    last_price,
                    price_change_24h,
                    best_ask_amount,
                    best_ask_price,
                    best_bid_amount,
                    best_bid_price,
                };

                Ok(ResponseOrError::Response(DataResponse {
                    data: converted_ticker,
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}
