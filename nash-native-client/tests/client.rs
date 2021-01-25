use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::offset::TimeZone;
use chrono::Utc;
use dotenv::dotenv;
use tokio::time::Duration;

use nash_native_client::{Client, Environment};
use nash_protocol::protocol::asset_nonces::AssetNoncesRequest;
use nash_protocol::protocol::cancel_all_orders::CancelAllOrders;
use nash_protocol::protocol::cancel_order::CancelOrderRequest;
use nash_protocol::protocol::dh_fill_pool::DhFillPoolRequest;
use nash_protocol::protocol::get_account_order::GetAccountOrderRequest;
use nash_protocol::protocol::get_ticker::TickerRequest;
use nash_protocol::protocol::list_account_balances::ListAccountBalancesRequest;
use nash_protocol::protocol::list_account_orders::ListAccountOrdersRequest;
use nash_protocol::protocol::list_account_trades::ListAccountTradesRequest;
use nash_protocol::protocol::list_candles::ListCandlesRequest;
use nash_protocol::protocol::list_markets::ListMarketsRequest;
use nash_protocol::protocol::list_trades::ListTradesRequest;
use nash_protocol::protocol::orderbook::OrderbookRequest;
use nash_protocol::protocol::place_order::{LimitOrderRequest, MarketOrderRequest};
use nash_protocol::protocol::sign_all_states::SignAllStates;
use nash_protocol::protocol::subscriptions::trades::SubscribeTrades;
use nash_protocol::protocol::subscriptions::updated_orderbook::SubscribeOrderbook;
use nash_protocol::protocol::subscriptions::new_account_trades::SubscribeAccountTrades;
use nash_protocol::protocol::subscriptions::updated_account_balances::SubscribeAccountBalances;
use nash_protocol::protocol::subscriptions::updated_account_orders::SubscribeAccountOrders;
use nash_protocol::types::{
    Blockchain, BuyOrSell, DateTimeRange, OrderCancellationPolicy, OrderStatus, OrderType,
};

async fn init_client() -> Client {
    dotenv().ok();
    let secret = std::env::var("NASH_API_SECRET").expect("Couldn't get environment variable.");
    let session = std::env::var("NASH_API_KEY").expect("Couldn't get environment variable.");
    Client::from_keys(
        &secret,
        &session,
        None,
        0,
        Environment::Production,
        Duration::from_secs_f32(5.0),
    )
        .await
        .unwrap()
}

async fn init_sandbox_client() -> Client {
    Client::from_keys_path(
        None,
        0,
        None,
        Environment::Sandbox,
        Duration::from_secs_f32(5.0),
    )
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_all_http() {
    let client = init_client().await;
    let response = client
        .run_http(CancelAllOrders {
            market: "eth_usdc".to_string(),
        })
        .await;
    // let response = client.run_http(ListMarketsRequest).await;
    println!("{:?}", response);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiple_concurrent_requests_http() {
    let client = Arc::new(init_client().await);
    async fn make_cancel_order(client: Arc<Client>, i: u64) {
        println!("started loop {}", i);
        let req = client
            .run_http(CancelAllOrders {
                market: "eth_usdc".to_string(),
            })
            .await;
        if !req.is_err() {
            println!("finished cancel_order {}", i);
        } else {
            println!("error cancel_order {}", i);
        }
    }
    let mut handles = Vec::new();
    let mut count = 0;
    for _ in 0..10 {
        handles.push(tokio::spawn(make_cancel_order(client.clone(), count)));
        count += 1;
    }
    futures::future::join_all(handles).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_autosigning() {
    let _client = init_client().await;
    tokio::time::sleep(Duration::from_secs(100)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_loops_concurrent_request() {
    let client = Arc::new(init_client().await);
    let counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for i in 0..100 {
        let client_1 = client.clone();
        let counter_1 = counter.clone();
        handles.push(tokio::spawn(async move {
            let req = client_1
                .run(ListAccountOrdersRequest {
                    before: None,
                    market: Some("eth_btc".to_string()),
                    buy_or_sell: None,
                    limit: Some(100),
                    status: Some(vec![
                        OrderStatus::Open,
                        OrderStatus::Filled,
                        OrderStatus::Canceled,
                    ]),
                    order_type: Some(vec![OrderType::Limit]),
                    range: None,
                })
                .await;
            if !req.is_err() {
                println!("done orders  {}", i);
            } else {
                println!("error orders {}: {}", i, req.unwrap_err());
                counter_1.fetch_add(1, Ordering::Relaxed);
            }
        }));
        let client_2 = client.clone();
        let counter_2 = counter.clone();
        handles.push(tokio::spawn(async move {
            let req = client_2
                .run(ListAccountTradesRequest {
                    before: None,
                    market: Some("eth_btc".to_string()),
                    limit: Some(100),
                    range: None,
                })
                .await;
            if !req.is_err() {
                println!("done trades {}", i);
            } else {
                println!("error trades {}: {}", i, req.unwrap_err());
                counter_2.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    futures::future::join_all(handles).await;
    println!("num errors: {}", counter.load(Ordering::Relaxed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiple_concurrent_requests() {
    let client = init_client().await;
    let share_client = Arc::new(client);
    async fn make_long_request(client: Arc<Client>, i: u64) {
        println!("started long {}", i);
        let req = client.run(SignAllStates::new()).await;
        if !req.is_err() {
            println!("done (long) {}", i);
        } else {
            println!("error (long) {}: {}", i, req.unwrap_err());
        }
    }
    async fn make_short_request(client: Arc<Client>, i: u64) {
        println!("started short2 {}", i);
        let req = client.run(ListMarketsRequest).await;
        if !req.is_err() {
            println!("done (short) {}", i);
        } else {
            println!("error (short) {}: {}", i, req.unwrap_err());
        }
    }
    let mut handles = Vec::new();
    let mut count = 0;
    for _ in 0..10 {
        handles.push(tokio::spawn(make_long_request(share_client.clone(), count)));
        count += 1;
        handles.push(tokio::spawn(make_short_request(
            share_client.clone(),
            count,
        )));
        count += 1;
    }
    futures::future::join_all(handles).await;
}

#[test]
fn test_disconnect() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let _d = client.disconnect().await;
        let resp = client.run(ListMarketsRequest).await;
        // println!("{:?}", resp);
        assert_eq!(resp.is_err(), true);
    };
    runtime.block_on(async_block);
}

#[test]
fn test_list_markets_sandbox() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_sandbox_client().await;
        let response = client.run(ListMarketsRequest).await.unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_dh_fill_pool() {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let async_block = async {
        let client = init_client().await;
        println!("Client ready!");
        let response = client
            .run(DhFillPoolRequest::new(Blockchain::Ethereum).unwrap())
            .await
            .unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_asset_nonces() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client.run(AssetNoncesRequest::new()).await.unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn dependency_sign_all() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client.run(SignAllStates::new()).await.unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn list_account_orders() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(ListAccountOrdersRequest {
                before: None,
                market: Some("eth_usdc".to_string()),
                buy_or_sell: None,
                limit: Some(1),
                status: Some(vec![
                    OrderStatus::Open,
                    OrderStatus::Filled,
                    OrderStatus::Canceled,
                ]),
                order_type: Some(vec![OrderType::Limit]),
                range: Some(DateTimeRange {
                    start: Utc.ymd(2020, 9, 12).and_hms(0, 0, 0),
                    stop: Utc.ymd(2020, 10, 16).and_hms(0, 10, 0),
                }),
            })
            .await
            .unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
pub fn list_account_trades() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(ListAccountTradesRequest {
                before: Some("1598934832187000008".to_string()),
                market: Some("eth_usdc".to_string()),
                limit: Some(1),
                range: None,
            })
            .await
            .unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
pub fn list_candles() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(ListCandlesRequest {
                before: None,
                market: "eth_usdc".to_string(),
                limit: Some(1),
                chronological: None,
                interval: None,
                range: Some(DateTimeRange {
                    start: Utc.ymd(2020, 8, 1).and_hms(0, 0, 0),
                    stop: Utc.ymd(2020, 8, 1).and_hms(0, 10, 0),
                }),
            })
            .await
            .unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_cancel_all() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(CancelAllOrders {
                market: "eth_usdc".to_string(),
            })
            .await
            .unwrap();
        println!("{:?}", response);
        assert_eq!(response.response().unwrap().accepted, true);
    };
    runtime.block_on(async_block);
}

#[test]
fn test_list_account_balances() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(ListAccountBalancesRequest { filter: None })
            .await
            .unwrap();
        println!("{:#?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn test_list_markets() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client.run(ListMarketsRequest).await.unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn test_account_order_lookup_then_cancel() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let mut requests = Vec::new();
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Buy,
            amount: "10".to_string(),
            price: "5".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Sell,
            amount: "1".to_string(),
            price: "200".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Buy,
            amount: "10".to_string(),
            price: "5".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Sell,
            amount: "1".to_string(),
            price: "200".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Buy,
            amount: "10".to_string(),
            price: "1".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Buy,
            amount: "0.451".to_string(),
            price: "75.1".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Sell,
            amount: "1.24".to_string(),
            price: "821".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        requests.push(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Sell,
            amount: "1.24".to_string(),
            price: "821.12".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        });
        for request in requests {
            let response = client.run(request).await.unwrap();
            println!("{:?}", response);
            let order_id = response.response().unwrap().order_id.clone();
            // Small delay to make sure it is processed
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let response = client
                .run(GetAccountOrderRequest {
                    order_id: order_id.clone(),
                })
                .await
                .unwrap();
            println!("{:?}", response);
            let response = client
                .run(CancelOrderRequest {
                    market: "eth_usdc".to_string(),
                    order_id,
                })
                .await
                .unwrap();
            println!("{:?}", response);
        }
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_orderbook() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(OrderbookRequest {
                market: "eth_usdc".to_string(),
            })
            .await
            .unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_ticker() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(TickerRequest {
                market: "btc_usdc".to_string(),
            })
            .await
            .unwrap();
        println!("{:?}", response.response_or_error());
        let response = client
            .run(TickerRequest {
                market: "btc_usdc".to_string(),
            })
            .await
            .unwrap();
        println!("{:?}", response.response_or_error());
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_list_trades() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(ListTradesRequest {
                market: "eth_usdc".to_string(),
                limit: None,
                before: None,
            })
            .await
            .unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_sub_orderbook() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let mut response = client
            .subscribe_protocol(SubscribeOrderbook {
                market: "btc_usdc".to_string(),
            })
            .await
            .unwrap();
        let next_item = response.recv().await.unwrap().unwrap();
        println!("{:?}", next_item);
        let next_item = response.recv().await.unwrap().unwrap();
        println!("{:?}", next_item);
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_sub_trades() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let mut response = client
            .subscribe_protocol(SubscribeTrades {
                market: "btc_usdc".to_string(),
            })
            .await
            .unwrap();
        let next_item = response.recv().await.unwrap().unwrap();
        println!("{:?}", next_item);
        let next_item = response.recv().await.unwrap().unwrap();
        println!("{:?}", next_item);
    };
    runtime.block_on(async_block);
}

#[tokio::test]
async fn end_to_end_sub_account_trades() {
    let client = init_client().await;
    let mut response = client
        .subscribe_protocol(SubscribeAccountTrades {
            market_name: "btc_usdc".to_string(),
        })
        .await
        .unwrap();
    let next_item = response.recv().await.unwrap().unwrap();
    println!("{:?}", next_item);
}

#[tokio::test]
async fn end_to_end_sub_account_orders() {
    let client = init_client().await;
    let mut response = client
        .subscribe_protocol(SubscribeAccountOrders {
            market: None,
            status: None,
            buy_or_sell: None,
            order_type: None,
            range: None
        })
        .await
        .unwrap();
    let next_item = response.recv().await.unwrap().unwrap();
    println!("{:?}", next_item);
}

#[tokio::test]
async fn end_to_end_sub_account_balance() {
    let client = init_client().await;
    let mut response = client
        .subscribe_protocol(SubscribeAccountBalances {
            symbol: "usdc".to_string(), // this does not work with None
        })
        .await
        .unwrap();
    let next_item = response.recv().await.unwrap().unwrap();
    println!("{:?}", next_item);
}

#[test]
fn end_to_end_buy_eth_btc() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(LimitOrderRequest {
                client_order_id: None,
                market: "eth_btc".to_string(),
                buy_or_sell: BuyOrSell::Buy,
                amount: "0.1".to_string(),
                price: "0.0213070".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            })
            .await
            .unwrap();
        println!("{:?}", response);
        let response = client
            .run(CancelAllOrders {
                market: "eth_btc".to_string(),
            })
            .await
            .unwrap();
        println!("{:?}", response);
        assert_eq!(response.response().unwrap().accepted, true);
    };
    runtime.block_on(async_block);
}

#[test]
fn sub_orderbook_via_client_stream() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let mut response = client
            .subscribe_protocol(SubscribeOrderbook {
                market: "btc_usdc".to_string(),
            })
            .await
            .unwrap();
        for _ in 0..10 {
            let item = response.recv().await;
            println!("{:?}", item.unwrap().unwrap());
        }
    };
    runtime.block_on(async_block);
}

#[test]
fn limit_order_nonce_recovery() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let lor = LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Sell,
            amount: "0.02".to_string(),
            price: "900".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilTime(
                Utc.ymd(2020, 12, 16).and_hms(0, 0, 0),
            ),
            allow_taker: true,
        };
        // Get nonces
        client.run(AssetNoncesRequest::new()).await.ok();
        client.run(SignAllStates::new()).await.ok();

        // Break nonces
        let mut state_lock = client.inner.state.write().await;
        let mut bad_map = HashMap::new();
        bad_map.insert("eth".to_string(), vec![0 as u32]);
        bad_map.insert("usdc".to_string(), vec![0 as u32]);
        state_lock.remaining_orders = 100;
        state_lock.asset_nonces = Some(bad_map);
        drop(state_lock);

        // First attempt should fail with nonces complaint
        let response = client.run(lor.clone()).await.unwrap();
        println!("{:?}", response);
        // Second attempt should succeed because client state is set to refresh nonces
        let response = client.run(lor.clone()).await.unwrap();
        println!("{:?}", response);
        // Now cancel
        let order_id = response.response().unwrap().order_id.clone();
        let response = client
            .run(GetAccountOrderRequest { order_id })
            .await
            .unwrap();
        println!("{:?}", response);
        let response = client
            .run(CancelAllOrders {
                market: "eth_usdc".to_string(),
            })
            .await
            .unwrap();
        println!("{:?}", response);
        assert_eq!(response.response().unwrap().accepted, true);
    };
    runtime.block_on(async_block);
}

#[test]
fn end_to_end_market_order() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(MarketOrderRequest {
                client_order_id: None,
                market: "usdc_eth".to_string(),
                amount: "10".to_string(),
            })
            .await;
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}

#[tokio::test]
async fn place_order_no_sign_states_flat() {
    let client = init_client().await;
    client.turn_off_sign_states().await;
    let response = client
        .run(LimitOrderRequest {
            client_order_id: None,
            market: "eth_usdc".to_string(),
            buy_or_sell: BuyOrSell::Sell,
            amount: "0.004".to_string(),
            price: "1500".to_string(),
            cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
            allow_taker: true,
        })
        .await
        .unwrap();
    println!("{:?}", response);
    let response = client
        .run(CancelAllOrders {
            market: "eth_usdc".to_string(),
        })
        .await
        .unwrap();
    println!("{:?}", response);
    assert_eq!(response.response().unwrap().accepted, true);
}

#[test]
fn end_to_end_sell_limit_order() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client
            .run(LimitOrderRequest {
                client_order_id: None,
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Sell,
                amount: "0.004".to_string(),
                price: "1500".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            })
            .await
            .unwrap();
        println!("{:?}", response);
        let response = client
            .run(CancelAllOrders {
                market: "eth_usdc".to_string(),
            })
            .await
            .unwrap();
        println!("{:?}", response);
        assert_eq!(response.response().unwrap().accepted, true);
    };
    runtime.block_on(async_block);
}

#[test]
fn list_markets_test() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let async_block = async {
        let client = init_client().await;
        let response = client.run(ListMarketsRequest).await.unwrap();
        println!("{:?}", response);
    };
    runtime.block_on(async_block);
}
