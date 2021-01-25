use crate::graphql;
use graphql::new_account_trades;
use graphql_client::GraphQLQuery;

/// Initiate subscription to get new trades
#[derive(Clone, Debug)]
pub struct SubscribeAccountTrades {
    pub market_name: String, // TODO: not sure if this is supposed to be option, did not work in my test with None value
}

impl SubscribeAccountTrades {
    pub fn make_query(&self) -> graphql_client::QueryBody<new_account_trades::Variables> {
        graphql::NewAccountTrades::build_query(new_account_trades::Variables {
            payload: new_account_trades::NewAccountTradesParams {
                market_name: Some(self.market_name.clone()),
            }
        })
    }
}
