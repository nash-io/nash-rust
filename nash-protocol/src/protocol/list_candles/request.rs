use super::types::ListCandlesRequest;
use crate::graphql;
use crate::graphql::list_candles;
use crate::types::CandleInterval;

use graphql_client::GraphQLQuery;

impl ListCandlesRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<list_candles::Variables> {
        let get_order = list_candles::Variables {
            before: self.before.clone(),
            limit: self.limit,
            market_name: self.market.market_name(),
            chronological: self.chronological,
            interval: self.interval.as_ref().map(|x| x.into()),
            range_start: self.range.as_ref().map(|x| format!("{:?}", x.start)),
            range_stop: self.range.as_ref().map(|x| format!("{:?}", x.stop)),
        };
        graphql::ListCandles::build_query(get_order)
    }
}

impl From<&CandleInterval> for list_candles::CandleInterval {
    fn from(interval: &CandleInterval) -> Self {
        match interval {
            CandleInterval::FifteenMinute => list_candles::CandleInterval::FIFTEEN_MINUTE,
            CandleInterval::FiveMinute => list_candles::CandleInterval::FIVE_MINUTE,
            CandleInterval::FourHour => list_candles::CandleInterval::FOUR_HOUR,
            CandleInterval::OneDay => list_candles::CandleInterval::ONE_DAY,
            CandleInterval::OneHour => list_candles::CandleInterval::ONE_HOUR,
            CandleInterval::SixHour => list_candles::CandleInterval::SIX_HOUR,
            CandleInterval::ThirtyMinute => list_candles::CandleInterval::THIRTY_MINUTE,
            CandleInterval::ThreeHour => list_candles::CandleInterval::THREE_HOUR,
            CandleInterval::TwelveHour => list_candles::CandleInterval::TWELVE_HOUR,
            CandleInterval::OneMonth => list_candles::CandleInterval::ONE_MONTH,
            CandleInterval::OneWeek => list_candles::CandleInterval::ONE_WEEK,
            CandleInterval::OneMinute => list_candles::CandleInterval::ONE_MINUTE,
        }
    }
}
