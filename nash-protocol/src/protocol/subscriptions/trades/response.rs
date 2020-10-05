use super::super::super::ResponseOrError;
use super::request::SubscribeTrades;
use crate::errors::Result;
use crate::graphql;
use crate::types::{BuyOrSell, Market, SubscriptionTrade};
use graphql::subscribe_trades;

#[derive(Clone, Debug)]
pub struct TradesResponse {
    pub market: Market,
    pub trades: Vec<SubscriptionTrade>,
}

impl SubscribeTrades {
    pub fn response_from_graphql(
        &self,
        response: ResponseOrError<subscribe_trades::ResponseData>,
    ) -> Result<ResponseOrError<TradesResponse>> {
        Ok(match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                let trades: Result<Vec<SubscriptionTrade>> = response
                    .new_trades
                    .iter()
                    .map(|trade| {
                        Ok(SubscriptionTrade {
                            amount: self.market.asset_a.with_amount(&trade.amount.amount)?,
                            order_type: (&trade.direction).into(),
                            executed_at: trade.executed_at.clone(),
                        })
                    })
                    .collect();
                ResponseOrError::from_data(TradesResponse {
                    market: self.market,
                    trades: trades?,
                })
            }
            ResponseOrError::Error(error) => ResponseOrError::Error(error),
        })
    }
}

impl From<&subscribe_trades::Direction> for BuyOrSell {
    fn from(direction: &subscribe_trades::Direction) -> Self {
        match direction {
            subscribe_trades::Direction::BUY => BuyOrSell::Buy,
            subscribe_trades::Direction::SELL => BuyOrSell::Sell,
            // What is up with this?
            subscribe_trades::Direction::Other(_) => panic!("A trade without a direction?"),
        }
    }
}
