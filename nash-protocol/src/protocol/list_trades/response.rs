use std::str::FromStr;
use chrono::{DateTime, Utc};
use crate::errors::{ProtocolError, Result};
use crate::graphql::list_trades;
use crate::types::{AccountTradeSide, BuyOrSell, Trade};
use super::types::ListTradesResponse;
use bigdecimal::BigDecimal;
use crate::protocol::traits::TryFromState;
use crate::protocol::state::State;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

#[async_trait]
impl TryFromState<list_trades::ResponseData> for ListTradesResponse {

    async fn from(response: list_trades::ResponseData, _state: Arc<RwLock<State>>) -> Result<ListTradesResponse> {
        let mut trades = Vec::new();

        for trade_data in response.list_trades.trades {
            let market = trade_data.market.name.clone();
            let taker_fee = BigDecimal::from_str(&trade_data.taker_fee.amount)?;
            let maker_fee = BigDecimal::from_str(&trade_data.maker_fee.amount)?;
            let amount = BigDecimal::from_str(&trade_data.amount.amount)?;
            let maker_recieved = BigDecimal::from_str(&trade_data.maker_received.amount)?;
            let taker_recieved = BigDecimal::from_str(&trade_data.taker_received.amount)?;
            let limit_price = BigDecimal::from_str(&trade_data.limit_price.amount)?;

            trades.push(Trade {
                id: trade_data.id,
                taker_order_id: trade_data.taker_order_id.clone(),
                maker_order_id: trade_data.maker_order_id.clone(),
                amount,
                executed_at: DateTime::<Utc>::from_str(&trade_data.executed_at)
                    .map_err(|_| ProtocolError("Could not convert value to DateTime"))?,
                account_side: trade_data.account_side.into(),
                taker_fee,
                maker_fee,
                maker_recieved,
                taker_recieved,
                market,
                direction: trade_data.direction.into(),
                limit_price,
            })
        }

        Ok(ListTradesResponse {
            trades,
            next_page: response.list_trades.next.clone(),
        })
    }
}

impl From<list_trades::Direction> for BuyOrSell {
    fn from(response: list_trades::Direction) -> Self {
        match response {
            list_trades::Direction::BUY => Self::Buy,
            list_trades::Direction::SELL => Self::Sell,
            _ => panic!("Unexpected value in BuyOrSell enum"),
        }
    }
}

impl From<list_trades::AccountTradeSide> for AccountTradeSide {
    fn from(trade_side: list_trades::AccountTradeSide) -> Self {
        match trade_side {
            list_trades::AccountTradeSide::MAKER => AccountTradeSide::Maker,
            list_trades::AccountTradeSide::TAKER => AccountTradeSide::Taker,
            list_trades::AccountTradeSide::NONE => AccountTradeSide::None,
            _ => panic!("Unsupported value in AccountTradeSide"),
        }
    }
}
