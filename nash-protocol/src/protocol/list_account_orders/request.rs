use super::types::ListAccountOrdersRequest;
use crate::graphql;
use crate::graphql::list_account_orders;
use crate::types::{BuyOrSell, OrderStatus, OrderType};

use graphql_client::GraphQLQuery;

impl ListAccountOrdersRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<list_account_orders::Variables> {
        let get_order = list_account_orders::Variables {
            payload: list_account_orders::ListAccountOrdersParams {
                before: self.before.clone(),
                limit: self.limit,
                status: self
                    .status
                    .as_ref()
                    .map(|x| x.iter().map(|x| Some(x.clone().into())).collect()),
                buy_or_sell: self.buy_or_sell.map(|x| x.into()),
                market_name: self.market.clone(),
                type_: self
                    .order_type
                    .as_ref()
                    .map(|x| x.iter().map(|x| Some(x.clone().into())).collect()),
                range_start: self.range.as_ref().map(|x| format!("{:?}", x.start)),
                range_stop: self.range.as_ref().map(|x| format!("{:?}", x.stop)),
            },
        };
        graphql::ListAccountOrders::build_query(get_order)
    }
}

impl From<BuyOrSell> for list_account_orders::OrderBuyOrSell {
    fn from(buy_or_sell: BuyOrSell) -> Self {
        match buy_or_sell {
            BuyOrSell::Buy => list_account_orders::OrderBuyOrSell::BUY,
            BuyOrSell::Sell => list_account_orders::OrderBuyOrSell::SELL,
        }
    }
}

impl From<OrderStatus> for list_account_orders::OrderStatus {
    fn from(status: OrderStatus) -> Self {
        match status {
            OrderStatus::Canceled => list_account_orders::OrderStatus::CANCELLED,
            OrderStatus::Filled => list_account_orders::OrderStatus::FILLED,
            OrderStatus::Open => list_account_orders::OrderStatus::OPEN,
            OrderStatus::Pending => list_account_orders::OrderStatus::PENDING,
        }
    }
}

impl From<OrderType> for list_account_orders::OrderType {
    fn from(type_: OrderType) -> Self {
        match type_ {
            OrderType::Limit => list_account_orders::OrderType::LIMIT,
            OrderType::Market => list_account_orders::OrderType::MARKET,
            OrderType::StopLimit => list_account_orders::OrderType::STOP_LIMIT,
            OrderType::StopMarket => list_account_orders::OrderType::STOP_MARKET,
        }
    }
}
