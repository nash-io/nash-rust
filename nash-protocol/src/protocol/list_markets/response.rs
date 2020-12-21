use super::types::ListMarketsResponse;
use crate::errors::{ProtocolError, Result};
use crate::graphql::list_markets;
use crate::types::{Asset, Market};

use std::collections::HashMap;
use std::convert::TryFrom;

fn decimal_str_to_precision(decimal_str: &str) -> Result<u32> {
    let parts: Vec<&str> = decimal_str.split('.').collect();
    if parts.len() != 2 {
        return Err(ProtocolError("Invalid format for decimal string"));
    }
    Ok(parts[1].len() as u32)
}

impl TryFrom<list_markets::ResponseData> for ListMarketsResponse {
    type Error = ProtocolError;

    fn try_from(response: list_markets::ResponseData) -> Result<Self> {
        let mut markets = HashMap::new();
        for market_data in response.list_markets.iter().filter(|market_data| market_data.primary) {
            let market_name = market_data.name.clone();
            let asset_a_str = &market_data.a_asset.symbol;
            let asset_b_str = &market_data.b_asset.symbol;
            let asset_a = Asset::try_from(asset_a_str.as_str());
            let asset_b = Asset::try_from(asset_b_str.as_str());
            // Only return markets for assets known to the client
            // These unwraps are safe if
            if let (Ok(asset_a), Ok(asset_b)) = (asset_a, asset_b) {
                let precision_a =
                    decimal_str_to_precision(&market_data.min_trade_increment).expect("Impossible given 'decimal_str_to_precision' unless ME returns garbage for a precision");
                let precision_b =
                    decimal_str_to_precision(&market_data.min_trade_increment_b).expect("Impossible given 'decimal_str_to_precision' unless ME returns garbage for b precision");
                let prec_asset_a = asset_a.with_precision(precision_a);
                let prec_asset_b = asset_b.with_precision(precision_b);
                let min_trade_size_a = prec_asset_a.with_amount(&market_data.min_trade_size)?;
                let min_trade_size_b = prec_asset_b.with_amount(&market_data.min_trade_size_b)?;
                markets.insert(
                    market_name,
                    Market::new(
                        prec_asset_a,
                        prec_asset_b,
                        min_trade_size_a,
                        min_trade_size_b,
                    ),
                );
            }
        }
        Ok(Self { markets })
    }
}
