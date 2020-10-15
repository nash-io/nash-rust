use super::super::super::ResponseOrError;
use super::request::SubscribeTrades;
use crate::errors::{ProtocolError, Result};
use crate::graphql;
use crate::types::{BuyOrSell, Market, Trade, AccountTradeSide};
use graphql::subscribe_trades;
use std::convert::{TryInto, TryFrom};
use chrono::{DateTime, Utc};
use std::str::FromStr;


/// List of new incoming trades for a market via subscription.
#[derive(Clone, Debug)]
pub struct TradesResponse {
    pub market: Market,
    pub trades: Vec<Trade>,
}

impl TryFrom<subscribe_trades::ResponseData> for Vec<Trade> {
    type Error = ProtocolError;
    fn try_from(response: subscribe_trades::ResponseData) -> Result<Vec<Trade>> {
        let mut trades = Vec::new();
        for trade_data in response.new_trades {
            let market = Market::from_str(&trade_data.market.name)?;
            let taker_fee = market.asset_b.with_amount(&trade_data.taker_fee.amount)?;
            let maker_fee = market.asset_b.with_amount(&trade_data.maker_fee.amount)?;
            let amount = market.asset_a.with_amount(&trade_data.amount.amount)?;
            let maker_recieved = market
                .get_asset(&trade_data.maker_received.currency)?
                .with_amount(&trade_data.maker_received.amount)?;
            let taker_recieved = market
                .get_asset(&trade_data.taker_received.currency)?
                .with_amount(&trade_data.taker_received.amount)?;
            let limit_price = market.asset_b.with_amount(&trade_data.limit_price.amount)?;
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

impl SubscribeTrades {
    pub fn response_from_graphql(
        &self,
        response: ResponseOrError<subscribe_trades::ResponseData>,
    ) -> Result<ResponseOrError<TradesResponse>> {
        Ok(match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                ResponseOrError::from_data(TradesResponse {
                    market: self.market,
                    trades: response.try_into()?,
                })
            }
            ResponseOrError::Error(error) => ResponseOrError::Error(error),
        })
    }
}

impl From<subscribe_trades::Direction> for BuyOrSell {
    fn from(direction: subscribe_trades::Direction) -> Self {
        match direction {
            subscribe_trades::Direction::BUY => BuyOrSell::Buy,
            subscribe_trades::Direction::SELL => BuyOrSell::Sell,
            // What is up with this?
            subscribe_trades::Direction::Other(_) => panic!("A trade without a direction?"),
        }
    }
}

impl From<subscribe_trades::AccountTradeSide> for AccountTradeSide {
    fn from(trade_side: subscribe_trades::AccountTradeSide) -> Self {
        match trade_side {
            subscribe_trades::AccountTradeSide::MAKER => AccountTradeSide::Maker,
            subscribe_trades::AccountTradeSide::TAKER => AccountTradeSide::Taker,
            subscribe_trades::AccountTradeSide::NONE => AccountTradeSide::None,
            _ => panic!("Unsupported value in AccountTradeSide"),
        }
    }
}
