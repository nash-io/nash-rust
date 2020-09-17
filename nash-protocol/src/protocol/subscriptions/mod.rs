pub mod trades;
pub mod updated_orderbook;


pub enum SubscriptionResponse {
    Orderbook(updated_orderbook::types::SubscribeOrderbookResponse)
}