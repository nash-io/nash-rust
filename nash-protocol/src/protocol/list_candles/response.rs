use super::types::ListCandlesResponse;
use crate::errors::{ProtocolError, Result};
use crate::graphql::list_candles;
use crate::types::{Candle, CandleInterval};
use chrono::{DateTime, Utc};
use bigdecimal::BigDecimal;
use std::str::FromStr;
use crate::protocol::traits::TryFromState;
use crate::protocol::state::State;
use std::sync::Arc;
use futures::lock::Mutex;
use async_trait::async_trait;


#[async_trait]
impl TryFromState<list_candles::ResponseData> for ListCandlesResponse {
    async fn from(response: list_candles::ResponseData, _state: Arc<Mutex<State>>) -> Result<ListCandlesResponse> {
        let mut candles = Vec::new();
        for candle_data in response.list_candles.candles {
            let a_volume = BigDecimal::from_str(&candle_data.a_volume.amount)?;
            let b_volume = BigDecimal::from_str(&candle_data.b_volume.amount)?;
            let low_price = BigDecimal::from_str(&candle_data.low_price.amount)?;
            let open_price = BigDecimal::from_str(&candle_data.open_price.amount)?;
            let close_price = BigDecimal::from_str(&candle_data.close_price.amount)?;
            let high_price = BigDecimal::from_str(&candle_data.high_price.amount)?;
            candles.push(Candle {
                a_volume,
                b_volume,
                high_price,
                low_price,
                open_price,
                close_price,
                interval: candle_data.interval.into(),
                interval_start: DateTime::<Utc>::from_str(&candle_data.interval_starting_at)
                    .map_err(|_| ProtocolError("Could not convert value to DateTime"))?,
            })
        }
        Ok(ListCandlesResponse {
            candles,
            next_page: response.list_candles.next,
        })
    }
}

impl From<list_candles::CandleInterval> for CandleInterval {
    fn from(interval: list_candles::CandleInterval) -> Self {
        match interval {
            list_candles::CandleInterval::FIFTEEN_MINUTE => CandleInterval::FifteenMinute,
            list_candles::CandleInterval::FIVE_MINUTE => CandleInterval::FiveMinute,
            list_candles::CandleInterval::FOUR_HOUR => CandleInterval::FourHour,
            list_candles::CandleInterval::ONE_DAY => CandleInterval::OneDay,
            list_candles::CandleInterval::ONE_HOUR => CandleInterval::OneHour,
            list_candles::CandleInterval::SIX_HOUR => CandleInterval::SixHour,
            list_candles::CandleInterval::THIRTY_MINUTE => CandleInterval::ThirtyMinute,
            list_candles::CandleInterval::THREE_HOUR => CandleInterval::ThreeHour,
            list_candles::CandleInterval::TWELVE_HOUR => CandleInterval::TwelveHour,
            list_candles::CandleInterval::ONE_MONTH => CandleInterval::OneMonth,
            list_candles::CandleInterval::ONE_WEEK => CandleInterval::OneWeek,
            list_candles::CandleInterval::ONE_MINUTE => CandleInterval::OneMinute,
            _ => panic!("Unsupported interval"),
        }
    }
}
