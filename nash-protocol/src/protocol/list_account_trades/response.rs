use super::types::ListAccountTradesResponse;
use crate::errors::{ProtocolError, Result};
use crate::graphql::list_account_trades;
use crate::types::{AccountTradeSide, BuyOrSell, Market, Trade};
use chrono::{DateTime, Utc};
use std::convert::TryFrom;
use std::str::FromStr;

impl TryFrom<list_account_trades::ResponseData> for ListAccountTradesResponse {
    type Error = ProtocolError;
    fn try_from(response: list_account_trades::ResponseData) -> Result<ListAccountTradesResponse> {
        let mut trades = Vec::new();
        for trade_data in response.list_account_trades.trades {
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
        Ok(ListAccountTradesResponse {
            trades,
            next_page: response.list_account_trades.next.clone(),
        })
    }
}

impl From<list_account_trades::Direction> for BuyOrSell {
    fn from(response: list_account_trades::Direction) -> Self {
        match response {
            list_account_trades::Direction::BUY => Self::Buy,
            list_account_trades::Direction::SELL => Self::Sell,
            _ => panic!("Unexpected value in BuyOrSell enum"),
        }
    }
}

impl From<list_account_trades::AccountTradeSide> for AccountTradeSide {
    fn from(trade_side: list_account_trades::AccountTradeSide) -> Self {
        match trade_side {
            list_account_trades::AccountTradeSide::MAKER => AccountTradeSide::Maker,
            list_account_trades::AccountTradeSide::TAKER => AccountTradeSide::Taker,
            list_account_trades::AccountTradeSide::NONE => AccountTradeSide::None,
            _ => panic!("Unsupported value in AccountTradeSide"),
        }
    }
}
