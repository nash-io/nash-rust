use crate::graphql;
use graphql::updated_account_orders;
use graphql_client::GraphQLQuery;
use crate::types::{BuyOrSell, OrderStatus, OrderType, DateTimeRange};

/// Initiate subscription to get new orders for an account
#[derive(Clone, Debug)]
pub struct SubscribeAccountOrders {
    pub market: Option<String>,
    pub buy_or_sell: Option<BuyOrSell>,
    pub status: Option<Vec<OrderStatus>>,
    pub order_type: Option<Vec<OrderType>>,
    pub range: Option<DateTimeRange>
}

impl From<BuyOrSell> for updated_account_orders::OrderBuyOrSell {
    fn from(buy_or_sell: BuyOrSell) -> Self {
        match buy_or_sell {
            BuyOrSell::Buy => updated_account_orders::OrderBuyOrSell::BUY,
            BuyOrSell::Sell => updated_account_orders::OrderBuyOrSell::SELL,
        }
    }
}

impl From<OrderStatus> for updated_account_orders::OrderStatus {
    fn from(status: OrderStatus) -> Self {
        match status {
            OrderStatus::Canceled => updated_account_orders::OrderStatus::CANCELLED,
            OrderStatus::Filled => updated_account_orders::OrderStatus::FILLED,
            OrderStatus::Open => updated_account_orders::OrderStatus::OPEN,
            OrderStatus::Pending => updated_account_orders::OrderStatus::PENDING,
        }
    }
}

impl From<OrderType> for updated_account_orders::OrderType {
    fn from(type_: OrderType) -> Self {
        match type_ {
            OrderType::Limit => updated_account_orders::OrderType::LIMIT,
            OrderType::Market => updated_account_orders::OrderType::MARKET,
            OrderType::StopLimit => updated_account_orders::OrderType::STOP_LIMIT,
            OrderType::StopMarket => updated_account_orders::OrderType::STOP_MARKET,
        }
    }
}

impl SubscribeAccountOrders {
    pub fn make_query(&self) -> graphql_client::QueryBody<updated_account_orders::Variables> {
        graphql::UpdatedAccountOrders::build_query(updated_account_orders::Variables {
            payload: updated_account_orders::UpdatedAccountOrdersParams {
                market_name: self.market.clone(),
                buy_or_sell: self.buy_or_sell.map(|x| x.into()),
                status: self
                    .status
                    .as_ref()
                    .map(|x| x.iter().map(|x| Some(x.clone().into())).collect()),
                type_: self
                    .order_type
                    .as_ref()
                    .map(|x| x.iter().map(|x| Some(x.clone().into())).collect()),
                range_start: self.range.as_ref().map(|x| format!("{:?}", x.start)),
                range_stop: self.range.as_ref().map(|x| format!("{:?}", x.stop)),
            }
        })
    }
}
