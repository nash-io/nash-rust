pub mod trades;
pub mod updated_orderbook;

use super::graphql::ResponseOrError;

#[derive(Debug)]
pub enum SubscriptionResponse {
    UpdatedOrderbook(ResponseOrError<updated_orderbook::types::SubscribeOrderbookResponse>),
    NewTrade(ResponseOrError<trades::types::TradesResponse>)
}