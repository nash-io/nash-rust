use super::super::super::ResponseOrError;
use super::request::SubscribeAccountTrades;
use crate::errors::{ProtocolError, Result};
use crate::graphql;
use crate::types::{BuyOrSell, Trade, AccountTradeSide};
use graphql::new_account_trades;
use chrono::{DateTime, Utc};
use bigdecimal::BigDecimal;
use std::str::FromStr;
use crate::protocol::traits::TryFromState;
use crate::protocol::state::State;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

/// List of new incoming trades for a market via subscription.
#[derive(Clone, Debug)]
pub struct AccountTradesResponse {
    pub trades: Vec<Trade>,
}
#[async_trait]
impl TryFromState<new_account_trades::ResponseData> for Vec<Trade> {
    async fn from(response: new_account_trades::ResponseData, _state: Arc<RwLock<State>>) -> Result<Vec<Trade>> {
        let mut trades = Vec::new();
        for trade_data in response.new_account_trades {
            let market = trade_data.market.name.clone();
            let taker_fee = BigDecimal::from_str(&trade_data.taker_fee.amount)?;
            let maker_fee = BigDecimal::from_str(&trade_data.maker_fee.amount)?;
            let amount = BigDecimal::from_str(&trade_data.amount.amount)?;
            let maker_recieved = BigDecimal::from_str(&trade_data.maker_received.amount)?;
            let taker_recieved = BigDecimal::from_str(&trade_data.taker_received.amount)?;
            let limit_price = BigDecimal::from_str(&trade_data.limit_price.amount)?;
            trades.push(Trade {
                market,
                amount,
                taker_fee,
                maker_fee,
                maker_recieved,
                taker_recieved,
                taker_order_id: trade_data.taker_order_id.clone(),
                maker_order_id: trade_data.maker_order_id.clone(),
                account_side: trade_data.account_side.into(),
                id: trade_data.id,
                executed_at: DateTime::<Utc>::from_str(&trade_data.executed_at)
                    .map_err(|_| ProtocolError("Could not convert value to DateTime"))?,
                limit_price,
                direction: trade_data.direction.into(),
            })
        }
        Ok(trades)
    }
}

impl SubscribeAccountTrades {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<new_account_trades::ResponseData>,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<AccountTradesResponse>> {
        Ok(match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                ResponseOrError::from_data(AccountTradesResponse {
                    trades: TryFromState::from(response, state).await?,
                })
            }
            ResponseOrError::Error(error) => ResponseOrError::Error(error),
        })
    }
}

impl From<new_account_trades::Direction> for BuyOrSell {
    fn from(direction: new_account_trades::Direction) -> Self {
        match direction {
            new_account_trades::Direction::BUY => BuyOrSell::Buy,
            new_account_trades::Direction::SELL => BuyOrSell::Sell,
            // What is up with this?
            new_account_trades::Direction::Other(_) => panic!("A trade without a direction?"),
        }
    }
}

impl From<new_account_trades::AccountTradeSide> for AccountTradeSide {
    fn from(trade_side: new_account_trades::AccountTradeSide) -> Self {
        match trade_side {
            new_account_trades::AccountTradeSide::MAKER => AccountTradeSide::Maker,
            new_account_trades::AccountTradeSide::TAKER => AccountTradeSide::Taker,
            new_account_trades::AccountTradeSide::NONE => AccountTradeSide::None,
            _ => panic!("Unsupported value in AccountTradeSide"),
        }
    }
}
