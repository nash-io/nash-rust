//! This module generates types from the Nash GraphQL schema

use graphql_client::*;

// Types for custom scalars are not picked up automatically from
// the schema. Need to add those manually here, and they will be
// used inside the generated queries.
type MarketName = String;
type CurrencySymbol = String;
type CurrencyNumber = String;
type DateTime = String;
type Base16 = String;
type PaginationCursor = String;
type AffiliateDeveloperCode = String;

// But otherwise these macros will generate type constructor code
// inside a new module, grounded on the associated structs

/// Rust constructor for GetOrderbook query
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/orderbook.graphql",
    response_derives = "Debug"
)]
pub struct GetOrderbook;

/// Rust constructor for GetTicker query
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/get_ticker.graphql",
    response_derives = "Debug"
)]
pub struct GetTicker;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/subscriptions/updated_ticker.graphql",
    response_derives = "Debug"
)]
pub struct UpdatedTicker;

/// Rust constructor for subscription to incoming trades
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/subscriptions/trades.graphql",
    response_derives = "Debug"
)]
pub struct SubscribeTrades;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/subscriptions/updated_orderbook.graphql",
    response_derives = "Debug"
)]
pub struct UpdatedOrderbook;

/// Rust type for CancelAllOrders query constructor
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/mutations/cancel_all_orders.graphql",
    response_derives = "Debug"
)]
pub struct CancelAllOrders;

/// Rust type for DhFillPool query constructor
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/dh_fill.graphql",
    response_derives = "Debug"
)]
pub struct DhFillPool;

/// Rust type for GetAssetsNonces constructor
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/assets_nonces.graphql",
    response_derives = "Debug"
)]
pub struct GetAssetsNonces;

/// Rust type for PlaceLimitOrder mutation constructor
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/mutations/limit_order.graphql",
    response_derives = "Debug"
)]
pub struct PlaceLimitOrder;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/mutations/market_order.graphql",
    response_derives = "Debug"
)]
pub struct PlaceMarketOrder;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/mutations/sign_states.graphql",
    response_derives = "Debug"
)]
pub struct SignStates;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/list_account_balances.graphql",
    response_derives = "Debug"
)]
pub struct ListAccountBalances;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/get_account_order.graphql",
    response_derives = "Debug"
)]
pub struct GetAccountOrder;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/mutations/cancel_order.graphql",
    response_derives = "Debug"
)]
pub struct CancelOrder;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/list_markets.graphql",
    response_derives = "Debug"
)]
pub struct ListMarkets;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/list_account_orders.graphql",
    response_derives = "Debug"
)]
pub struct ListAccountOrders;
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/list_account_trades.graphql",
    response_derives = "Debug"
)]
pub struct ListAccountTrades;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/list_candles.graphql",
    response_derives = "Debug"
)]
pub struct ListCandles;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.json",
    query_path = "src/graphql/queries/list_trades.graphql",
    response_derives = "Debug"
)]
pub struct ListTrades;
