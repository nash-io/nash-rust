//! Client implementation of Nash API over websockets using channels and message brokers

use futures::lock::Mutex;
use futures::{FutureExt, SinkExt, StreamExt};
use futures_util::future::{select, Either};
use nash_protocol::protocol::{
    NashProtocol, NashProtocolPipeline, NashProtocolSubscription, ResponseOrError, State,
};
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::{delay_for, timeout, Duration};
use tokio_native_tls::TlsStream;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{connect_async, stream::Stream, WebSocketStream};

use nash_protocol::errors::{ProtocolError, Result};
use nash_protocol::protocol::subscriptions::SubscriptionResponse;

use super::absinthe::{AbsintheEvent, AbsintheTopic, AbsintheWSRequest, AbsintheWSResponse};

type WebSocket = WebSocketStream<Stream<TcpStream, TlsStream<TcpStream>>>;

// this will add hearbeat (keep alive) messages to the channel for ws to send out every 15s
pub fn spawn_heartbeat_loop(client_id: u64, sender: UnboundedSender<AbsintheWSRequest>) {
    tokio::spawn(async move {
        loop {
            let heartbeat = AbsintheWSRequest::new(
                client_id,
                0, // todo: this needs to increment but not overlap with other message ids
                AbsintheTopic::Phoenix,
                AbsintheEvent::Heartbeat,
                None,
            );
            if let Err(_ignore) = sender.send(heartbeat) {
                // Kill process on error
                break;
            }
            delay_for(Duration::from_millis(15000)).await;
        }
    });
}

// this will recieve messages to send out over websockets on one channel, and pass incoming ws messages
// back up to client on anoter channel.
pub fn spawn_sender_loop(
    mut websocket: WebSocket,
    mut ws_outgoing_reciever: UnboundedReceiver<AbsintheWSRequest>,
    ws_incoming_sender: UnboundedSender<AbsintheWSResponse>,
    message_broker_link: UnboundedSender<BrokerAction>,
) {
    tokio::spawn(async move {
        loop {
            let next_outgoing = ws_outgoing_reciever.recv().boxed();
            let next_incoming = websocket.next();
            match select(next_outgoing, next_incoming).await {
                Either::Left((out_msg, _)) => {
                    if let Some(Ok(m_text)) = out_msg.map(|x| serde_json::to_string(&x)) {
                        // If sending fails, don't fail here. Let the client timeout and handle it
                        let _ignore = websocket.send(Message::Text(m_text)).await;
                    } else {
                        // Kill process on error
                        break;
                    }
                }
                Either::Right((in_msg, _)) => {
                    if let Some(Ok(Ok(resp))) = in_msg.map(|x| x.map(|x| x.into_text())) {
                        if let Ok(resp_copy1) = serde_json::from_str(&resp) {
                            // println!("{:?}", resp_copy1);
                            // Similarly, let the client timeout on incoming response if this fails and handle that
                            let _ignore = ws_incoming_sender.send(resp_copy1);
                            // todo: this is a hack since the graphql library has not implemented clone() for responses
                            // but we need another copy of the response to send to the broker
                            // this could actually be more efficient and have broker handle all messages?
                            let resp_copy2: AbsintheWSResponse =
                                serde_json::from_str(&resp).unwrap(); // this unwrap is safe
                            let _ignore =
                                message_broker_link.send(BrokerAction::Message(resp_copy2));
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            };
        }
    });
}

/// Broker will route responses to the right client channel based on message id lookup
pub enum BrokerAction {
    RegisterRequest(u64, UnboundedSender<AbsintheWSResponse>),
    RegisterSubscription(String, UnboundedSender<AbsintheWSResponse>),
    Message(AbsintheWSResponse),
}

struct MessageBroker {
    link: UnboundedSender<BrokerAction>,
}

impl MessageBroker {
    pub fn new() -> Self {
        let (link, mut internal_reciever) = unbounded_channel();
        tokio::spawn(async move {
            let mut request_map = HashMap::new();
            let mut subscription_map = HashMap::new();
            loop {
                if let Some(next_incoming) = internal_reciever.next().await {
                    match next_incoming {
                        // Register a channel to send messages to with given id
                        BrokerAction::RegisterRequest(id, channel) => {
                            request_map.insert(id, channel);
                        }
                        BrokerAction::RegisterSubscription(id, channel) => {
                            subscription_map.insert(id, channel);
                        }
                        // When message comes in, if id is registered with channel, send there
                        BrokerAction::Message(response) => {
                            // if message has subscription id, send it to subscription
                            if let Some(id) = response.subscription_id() {
                                if let Some(channel) = subscription_map.get_mut(&id) {
                                    // Again, we will let client timeout on waiting a response using its own policy.
                                    // Crashing inside the broker process does not allow us to handle the error gracefully
                                    if let Err(_ignore) = channel.send(response) {
                                        // Kill process on error
                                        break;
                                    }
                                }
                            }
                            // otherwise check if it is a response to a registered request
                            else if let Some(id) = response.message_id() {
                                if let Some(channel) = request_map.get_mut(&id) {
                                    if let Err(_ignore) = channel.send(response) {
                                        // Kill process on error
                                        break;
                                    }
                                    // queries only have one response, so no need to keep this around
                                    request_map.remove(&id);
                                }
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        });
        Self { link }
    }
}

fn global_subscription_loop<T: NashProtocolSubscription + Send + Sync + 'static>(
    mut callback_channel: UnboundedReceiver<AbsintheWSResponse>,
    user_callback_sender: UnboundedSender<
        Result<ResponseOrError<<T as NashProtocolSubscription>::SubscriptionResponse>>,
    >,
    global_subscription_sender: UnboundedSender<Result<SubscriptionResponse>>,
    request: T,
    state: Arc<Mutex<State>>,
) {
    tokio::spawn(async move {
        loop {
            let response = callback_channel.next().await;
            // is there a valid incoming payload?
            if let Some(response) = response {
                // can the payload json be parsed?
                if let Ok(json_payload) = response.subscription_json_payload() {
                    // First do normal subscription logic
                    let output = match request.subscription_response_from_json(json_payload.clone())
                    {
                        Ok(response) => {
                            match response {
                                ResponseOrError::Error(err_resp) => {
                                    Ok(ResponseOrError::Error(err_resp))
                                }
                                response => {
                                    // this unwrap below is safe because previous match case checks for error
                                    let sub_response = response.response().unwrap();
                                    match request
                                        .process_subscription_response(sub_response, state.clone())
                                        .await
                                    {
                                        Ok(_) => Ok(response),
                                        Err(e) => Err(e),
                                    }
                                }
                            }
                        }
                        Err(e) => Err(e),
                    };
                    // If callback_channel fails, kill process
                    if let Err(_e) = user_callback_sender.send(output) {
                        // Note: we do not want to kill the process in this case! User could just have destroyed the individual callback stream
                        // and we still want to send to the global stream! maybe add a log here in the future
                    }

                    // Now do global subscription logic. If global channel fails, also kill process
                    if let Err(_e) = global_subscription_sender
                        .send(request.wrap_response_as_any_subscription(json_payload))
                    {
                        break;
                    }
                } else {
                    // Kill process due to unparsable absinthe payload
                    break;
                }
            } else {
                // kill process due to closed channel
                break;
            }
        }
    });
}

pub enum Environment {
    Production,
    Sandbox,
    Dev(&'static str),
}

impl Environment {
    pub fn url(&self) -> &str {
        match self {
            Self::Production => "app.nash.io",
            Self::Sandbox => "sandbox.nash.io",
            Self::Dev(s) => s,
        }
    }
}

/// Interface for interacting with a websocket connection
pub struct Client {
    ws_outgoing_sender: UnboundedSender<AbsintheWSRequest>,
    ws_incoming_reciever: UnboundedReceiver<AbsintheWSResponse>,
    last_message_id: Mutex<u64>,
    client_id: u64,
    message_broker: MessageBroker,
    state: Arc<Mutex<State>>,
    timeout: u64,
    global_subscription_sender: UnboundedSender<Result<SubscriptionResponse>>,
    pub(crate) global_subscription_receiver: UnboundedReceiver<Result<SubscriptionResponse>>,
}

impl Client {
    /// Create a new client using an optional `keys_path`. The `client_id` is an identifier
    /// registered with the absinthe WS connection. It can possibly be removed.
    pub async fn new(
        keys_path: Option<&str>,
        client_id: u64,
        affiliate_code: Option<String>,
        env: Environment,
        timeout: u64,
    ) -> Result<Self> {
        let state = State::new(keys_path)?;
        Self::client_setup(state, client_id, affiliate_code, env, timeout).await
    }
    /// Create a client using a base64 encoded keylist and session id (contents of Nash produced .json file)
    pub async fn from_key_data(
        secret: &str,
        session: &str,
        affiliate_code: Option<String>,
        client_id: u64,
        env: Environment,
        timeout: u64,
    ) -> Result<Self> {
        let state = State::from_key_data(secret, session)?;
        Self::client_setup(state, client_id, affiliate_code, env, timeout).await
    }
    /// Main client setup logic
    async fn client_setup(
        mut state: State,
        client_id: u64,
        affiliate_code: Option<String>,
        env: Environment,
        timeout: u64,
    ) -> Result<Self> {
        let version = "2.0.0";

        state.affiliate_code = affiliate_code;

        let domain = env.url();

        // Setup authenticated or unauthenticated connection
        let conn_path = match &state.signer {
            Some(signer) => format!(
                "wss://{}/api/socket/websocket?token={}&vsn={}",
                domain, signer.api_keys.session_id, version
            ),
            None => format!("wss://{}/api/socket/websocket?vsn={}", domain, version),
        };

        // create connection
        let (socket, _response) = connect_async(&conn_path)
            .await
            .map_err(|_| ProtocolError("Could not connect to WS"))?;

        // channels to pass messages between threads. bounded at 100 unprocessed
        let (ws_outgoing_sender, ws_outgoing_reciever) = unbounded_channel();
        let (ws_incoming_sender, ws_incoming_reciever) = unbounded_channel();

        let (global_subscription_sender, global_subscription_receiver) = unbounded_channel();

        let message_broker = MessageBroker::new();

        // This will loop over WS connection, send things out, and route things in
        spawn_sender_loop(
            socket,
            ws_outgoing_reciever,
            ws_incoming_sender,
            message_broker.link.clone(),
        );

        // initialize the connection (first message id, 1)
        let last_message_id = 1;
        ws_outgoing_sender
            .send(AbsintheWSRequest::init_msg(client_id, last_message_id))
            .map_err(|_| ProtocolError("Could not initialize connection with Nash"))?;

        // start a heartbeat loop
        spawn_heartbeat_loop(client_id, ws_outgoing_sender.clone());

        Ok(Self {
            ws_outgoing_sender: ws_outgoing_sender.clone(),
            ws_incoming_reciever,
            last_message_id: Mutex::new(last_message_id),
            client_id,
            message_broker,
            state: Arc::new(Mutex::new(state)),
            timeout,
            global_subscription_sender,
            global_subscription_receiver,
        })
    }

    /// Execute a NashProtocol request. Query will be created, executed over network, response will
    /// be passed to the protocol's state update hook, and response will be returned. Used by the even
    /// more generic `run(..)`.
    async fn execute_protocol<T: NashProtocol + Sync>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<T::Response>> {
        let query = request.graphql(self.state.clone()).await?;
        // println!("{}", serde_json::to_string(&query).unwrap());
        let ws_response = timeout(
            Duration::from_millis(self.timeout),
            self.request(query).await?.next(),
        )
        .await
        .map_err(|_| ProtocolError("Request timeout"))?
        .ok_or(ProtocolError("Failed to recieve message"))?;
        let json_payload = ws_response.json_payload()?;
        let protocol_response = request.response_from_json(json_payload)?;
        if let Some(response) = protocol_response.response() {
            request
                .process_response(response, self.state.clone())
                .await?;
        };
        Ok(protocol_response)
    }

    /// Main entry point to execute Nash API requests. Capable of running anything that implements `NashProtocolPipeline`.
    /// All `NashProtocol` requests automatically do. Other more complex mutli-stage interactions like `SignAllStates`
    /// implement the trait manually. This will optionally run before and after hooks if those are defined for the pipeline
    /// or request (e.g., get asset nonces if they don't exist)
    pub async fn run<T: NashProtocolPipeline + Clone + Sync>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        // First run any dependencies of the request/pipeline
        let before_actions = request.run_before(self.state.clone()).await?;

        if let Some(actions) = before_actions {
            for action in actions {
                self.run_helper(action).await?;
            }
        }

        // Now run the pipeline
        // FIXME: get rid of the move here
        let out = self.run_helper(request.clone()).await;

        // Get things to run after the request/pipeline
        let after_actions = request.run_after(self.state.clone()).await?;

        // Now run anything specified for after the pipeline
        if let Some(actions) = after_actions {
            for action in actions {
                self.run_helper(action).await?;
            }
        }

        out
    }

    /// Does the main work of running a pipeline
    async fn run_helper<T: NashProtocolPipeline>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        let mut protocol_state = request.init_state(self.state.clone()).await;
        // While pipeline containst more actions for client to take, execute them
        loop {
            // FIXME: Ditto here on lock/unlock
            if let Some(protocol_request) = request
                .next_step(&protocol_state, self.state.clone())
                .await?
            {
                let protocol_response = self.execute_protocol(protocol_request).await?;
                // If error, end pipeline early and return GraphQL/network error data
                if protocol_response.is_error() {
                    return Ok(ResponseOrError::Error(
                        protocol_response
                            .consume_error()
                            .expect("Destructure error after check. Impossible to fail."),
                    ));
                }
                // Otherwise update the pipeline and continue
                request
                    .process_step(
                        protocol_response
                            .consume_response()
                            .expect("Destructure response after check. Impossible to fail."),
                        &mut protocol_state,
                    )
                    .await;
            } else {
                // If no more actions left, then done
                break;
            }
        }
        // Get pipeline output
        let output = request.output(protocol_state)?;
        Ok(output)
    }

    /// Entry point for running Nash protocol subscriptions
    pub async fn subscribe_protocol<T>(
        &self,
        request: T,
    ) -> Result<
        UnboundedReceiver<
            Result<ResponseOrError<<T as NashProtocolSubscription>::SubscriptionResponse>>,
        >,
    >
    where
        T: NashProtocolSubscription + Send + Sync + 'static,
    {
        let query = request.graphql(self.state.clone()).await?;
        // a subscription starts with a normal request
        let subscription_response = self
            .request(query)
            .await?
            .next()
            .await
            .ok_or(ProtocolError("Could not get subscription response"))?;
        // create a channel where associated data will be pushed back
        let (for_broker, callback_channel) = unbounded_channel();
        let broker_link = self.message_broker.link.clone();
        // use subscription id on the response we got back from the subscription query
        // to register incoming data with the broker
        let subsciption_id = subscription_response
            .subscription_setup_id()
            .ok_or(ProtocolError("Response does not include subscription id"))?;
        broker_link
            .send(BrokerAction::RegisterSubscription(
                subsciption_id,
                for_broker,
            ))
            .map_err(|_| ProtocolError("Could not register subscription with broker"))?;

        let global_subscription_sender = self.global_subscription_sender.clone();
        let (user_callback_sender, user_callback_receiver) = unbounded_channel();

        global_subscription_loop(
            callback_channel,
            user_callback_sender,
            global_subscription_sender.clone(),
            request.clone(),
            self.state.clone(),
        );

        Ok(user_callback_receiver)
    }

    async fn request(
        &self,
        request: serde_json::Value,
    ) -> Result<UnboundedReceiver<AbsintheWSResponse>> {
        let message_id = self.incr_id().await;
        let graphql_msg = AbsintheWSRequest::new(
            self.client_id,
            message_id,
            AbsintheTopic::Control,
            AbsintheEvent::Doc,
            Some(request),
        );
        // create a channel where message broker will push a response when it gets one
        let (for_broker, callback_channel) = unbounded_channel();
        let broker_link = self.message_broker.link.clone();
        // register that channel in the broker with our message id
        broker_link
            .send(BrokerAction::RegisterRequest(message_id, for_broker))
            .map_err(|_| ProtocolError("Could not register request with broker"))?;
        // send the query
        self.ws_outgoing_sender
            .send(graphql_msg)
            .map_err(|_| ProtocolError("Request failed to send over channel"))?;
        // return response from the message broker when it comes
        Ok(callback_channel)
    }

    pub async fn on_every_ws_response(&mut self, callback: fn(AbsintheWSResponse) -> ()) {
        // pull some incoming messages off of the reciever. we could loop over these
        // in main application logic, expose to foreign callback, etc.
        self.ws_incoming_reciever
            .borrow_mut()
            .for_each(|msg| {
                async move {
                    callback(msg);
                }
            })
            .await;
    }

    pub async fn incr_id(&self) -> u64 {
        let mut val = self.last_message_id.lock().await;
        *val += 1;
        *val
    }
}

#[cfg(test)]
mod tests {
    use super::{Client, Environment};
    use nash_protocol::protocol::asset_nonces::types::AssetNoncesRequest;
    use nash_protocol::protocol::cancel_all_orders::types::CancelAllOrders;
    use nash_protocol::protocol::cancel_order::types::CancelOrderRequest;
    use nash_protocol::protocol::dh_fill_pool::types::DhFillPoolRequest;
    use nash_protocol::protocol::get_account_order::types::GetAccountOrderRequest;
    use nash_protocol::protocol::get_ticker::types::TickerRequest;
    use nash_protocol::protocol::list_account_balances::types::ListAccountBalancesRequest;
    use nash_protocol::protocol::list_account_orders::types::ListAccountOrdersRequest;
    use nash_protocol::protocol::list_account_trades::types::ListAccountTradesRequest;
    use nash_protocol::protocol::list_candles::types::ListCandlesRequest;
    use nash_protocol::protocol::list_markets::types::ListMarketsRequest;
    use nash_protocol::protocol::orderbook::types::OrderbookRequest;
    use nash_protocol::protocol::place_order::types::LimitOrderRequest;
    use nash_protocol::protocol::sign_all_states::SignAllStates;
    use nash_protocol::protocol::subscriptions::updated_orderbook::request::SubscribeOrderbook;
    use nash_protocol::types::{
        Blockchain, BuyOrSell, DateTimeRange, Market, OrderCancellationPolicy, OrderStatus,
        OrderType,
    };

    use chrono::offset::TimeZone;
    use chrono::Utc;
    use futures_util::StreamExt;

    async fn init_client() -> Client {
        Client::new(
            Some("test_data/27db.json"),
            0,
            Some("voCKma".to_string()),
            Environment::Production,
            1500,
        )
        .await
        .unwrap()
    }

    async fn init_dev3_client() -> Client {
        Client::from_key_data(
            "eyJjaGlsZF9rZXlzIjp7Im0vNDQnLzAnLzAnLzAvMCI6eyJhZGRyZXNzIjoiMk5GNkRGSkdkbVpkRjNoQnU0b1dGdzNoUHdTbVBCejg2NWEiLCJjbGllbnRfc2VjcmV0X3NoYXJlIjoiMjRjODhlNTU0NWQ2OWQ0NmQzMDkzZTM0NDk5OTc1OTI2Njk2NTJhYWIxZDkzNjc1ZDc1NWFlN2JlYTMzZDY5ZiIsInB1YmxpY19rZXkiOiIwMmY3OGYxMzFlOTUxNjNmOWEzNjkzYTU4M2JjMjE3MTlhMTIwYzY0NWFhYzJlOTE0ZGE1ZDdkOTY1ZGNlOWQ3NGIiLCJzZXJ2ZXJfc2VjcmV0X3NoYXJlX2VuY3J5cHRlZCI6ImUzYTJhMWY0MmJiOTY0M2MyZmJmNzEyMDMxNzY3NGEyNzgyOTIwMGE4NjZhY2MyMzNkNzA2NGI5OTBmNmQwYzZmMjdhNzE5MmE3YmJkYWFkMWFkMTMzMmViNzYwZDcwNmYzN2RjYjRlMTRmN2QyYjM3ODc2ZDM4MjNhMjI3MTVhODk5MzM4M2FiMDdlNjBmY2U3ZDA3N2M5Njc4ODRjNDExYjE1OGVlNjc2NTdmNmJjYzM0NzhjMjFlZjk3MGQ4NTg5NGRhMjE4ZWY0ZTQzNGYwYmRhNThlNGE3NWE3OWU3MTg5N2ExZDczMmViYTk5ODQ1NDY4ZjkwMzU2YzAxNjNhMWIzZTc0MWM0MDI0ZmY2M2YwNmM3YWUxYzk2M2Q1OTNkYTU5MTNlYzQ4ZDIzZDMzNDQ5NmY4MzI2NjdhMGM3MDNiODMxZDgzYWE1MTJlNmMyOTlmMTM0YjYwZmUyNjA0NmVlYTVjNWYxNTBmMTJmMWRkZmU4ZDIxODM3YWU0MTYyZjY3MWFiNTM0MzliN2U3Y2Q4ZTc1OGQyYmZiZTRiNGJkYzg4OWMyM2NjNWE1NzFkM2Y3MTM5MDU2ZDNiMjA5MGYxOTdkZWY2NWY1MTkzMjUzNzI1NWFmZWMyMTgyMmI4ZmZlMTMwNTAzNmFlM2M3NjBmOGQzZDA5NThmNDYxYWY0NTQyMDVhZmNkMWE2ZjQzZjJkOTQxY2FhYzc2OWFjYjZkYmM2MTVhMWQ3Yzg1ZjI3ZGNkNWE3OWQ5NTY2MWVlZTQzN2QyMTcxZTIzOTBlYjc1MjYzZTA2ZmI4ZWZhY2NmOGRkMGNiZDcxMTg3NTVjOGQ1NjgyZDZhYmE5ZjRkZTE0OGI0Nzc5MmM1MTMzZjI5MGM1NTk1ZmYzMzIyYjZlMGY0MzMxZGU0MmJmNDdhYWE2ODgwMGRhYjYwNmIxZDIyNzkxNmNiZjRkOWYxNDM4YmI0NmMxMDFhNWM5ZjMyMDhhMjE5OTZkMWE3NDYxNzViMmRhN2VmZDlkYTU1NzRlMDFmMTliMTJiYTRhZmVhMTI3ZjkxZGJhMWQ1OWNhZWE2MGNmYWIyMGUwNTBhOGIxYmI0NjRkMjUzOTRlY2QzOWQ2ZGI4NGRlMjM4ZjI2ZDc3OGQ3NjZiYTY0YTE3NjBkODZlMjdmNGEwYjUzYzZjZjEzNTM1OGJmOGVkNmQ4M2VmMmY2MmMxODA4MDJjODBhZDZhOWZiYTBjZjkxZjU0NGNkZTM0NTlmODlmNmU5ZGQyYWM4YzRmNTMzYjJmNTUwMGNhYjdiOTljNmZiZjE0OTVkMGZhZDk5ZjdlODU2MDg0NjhmNGU5Y2NmMjY3NjNmYmYyZTAxNDMxMDI4MiJ9LCJtLzQ0Jy82MCcvMCcvMC8wIjp7ImFkZHJlc3MiOiJmODQ0YTVhNTJjMzg5ZDc4YTQwNjVjMzg2N2M5NTM3ZTQ2YjUyNzhjIiwiY2xpZW50X3NlY3JldF9zaGFyZSI6IjdhNzE3OGZhOTJkOTM4ZGJhMTgxNDk3NGI1MTZlMTMxMGY2M2U4OTU2Y2IzNDVkZDMxNGIwNGJkMzZlM2IwNGIiLCJwdWJsaWNfa2V5IjoiMDQ5OTUxYWZlNzFmYTYyZmZiZTE3YTM3M2Y0NTA5ZTY1ZDhmMTIzZjcxMmMxZTc4OTQ1OTE1YzIwMGM1YmVjYjA3NWJjODI5YTQ2MzQzMGZkNDc3YmQ1YmNmMzAyZjRkMTZlOGUyMGZiODZkODNiYTk4ODJjMzhjNWUzMTBkODMwMSIsInNlcnZlcl9zZWNyZXRfc2hhcmVfZW5jcnlwdGVkIjoiMmVjOWE3YjExZTc1ODkxMTgxNzRhMWU5MGIxN2M5YjEyYTU3Y2FiZjhlYzMzNmY3Yjk0MjY1MTAwZjdiM2ZhMWMyY2EzNmM0NDZhOWU0ZDJjMzFjNTRlZmQ0N2Y4OWRlN2EyZGJmNjU3ODhkNDQyNTc2NzRmNDFjZDc0ZGIwNTk4ZjBkNDFlNDY0OWViNGQ4Mzk5OTNkZGZkMWUyY2ZhNzBmZTNiNjM0NzFiYmYzOGNlMWUyZjU5ZGUxYjE4YjYxNzk5N2ZkNTRmZTUxYjBkNGE3MTNhMTNhYmViYmYyZWUxYTY1MjU3YTk5Mzk5MDJiNWMyNWIzZmJjNzdmNGJjNmQ2MDhmMjIyY2Q1ZTYyZGE4MjlmZWUwZWM4YzJmNjY0ZGI3MWY3NmZhYjVkODMyZGVkOTYxNjU2ZjU2Yzg1OTAzNTZlNjkyMDVhNGNmNmJmYmQzOWZmZWRhNDM0OTVmOGM0ODlmNWQ2NzU3Y2UxMjZlYWZlNzRiMjI0NjAyODI5NTk4YTc4NWZmZTdhMGYxYzg5YTY1Y2E3NjAyZjUwZjFkNjQ1NDVlYjg5ODA0ZmIzODdhNDY1ZGY3MzQxNWYzZmQxZWJmYjBkZjk2YTA2YmE5NzBhMDc1MGY4N2JhNzQ2OWNmMjBmNWExNzFjZDE3NDkwNmE0MDExNmMyYzBlMGQ5MGNkOTNmZmQ2ZTA4YWE3MWNlYTBiOTcxYjZjZGEyZTE4MGM2YjZhMTY5NTcwM2VlOTc1Zjc1OTA4Y2IxZjcwNmEyOWU0ZTc2YTczZjNkZjIxMTNiMDA3ZDZhMjhlN2I4NDNiMmU1MWRlNDU4NDY4NmY2YmU3NDkyNWMxMDI1ODYxMjc0ZTFjMTAwYmE5Mzc1MzIwOTlhMjE0ODcxYTVhN2YzYmNhMzQwYzQzYzUzZTJjMjA4MTUwN2JlMTliNWNkNDNmMTMxODFjZDQ4YTRiYjg5ZmVhZGMyOGVhODk1MWUwYzYxZjI5NWI3OGFiNTZhNTk3MjAwYzkyMjliMWViZjk1ODVhM2UzYTc0ZjM3NGY2MDRiNWIyNjBiOTQ5ZWQ5ZTQ5NTcyZTYzMzUyNzU0ODVhZGMzMDI3NTc5YWMwMGY3YmNmNDk3NTBhMDhmNDAwNjNkYjZiZWExOTk0NjBkYjM2ZTg4MDZlODM5MzI3NWI5OWIzMDYxNjBlNTFlZTkwMTliYjk4MDFjZTcyYzMyNWYwNTRmZTFmYjU4ZmY2MzRkMmE2NTgzM2VkMzk4NmM1MjM2YTcxN2MwYjRhMzkxNGE2ZjQ0YWNjY2JiZTc5ZTk2MTM0NzUyNjEwMTAyZGI0YTNiNmRiMzllZmU1MzA1MzBlNTA5NDkxOGFmYzYyZSJ9LCJtLzQ0Jy84ODgnLzAnLzAvMCI6eyJhZGRyZXNzIjoiQWNoVktrNlVIR1IzUHNTZjloMWVUQ0FLOE1mYmtnQ1Z6WSIsImNsaWVudF9zZWNyZXRfc2hhcmUiOiI0ZWRjM2IwNjE2OTkxZDQ2ODI5ZDE3ZjA4MzMwZjkyNTQ4OGZhYmEwMmIyMGYzODZlZmVlZWJlYjQ1ZmI3ZWM1IiwicHVibGljX2tleSI6IjAzMDVlYTJmMzA5ZjFjZDQ5OTNjMmE5ZmU4ZGNhNGQ0ZGI3NjU2MWRlMDViNTEwYjM3MTM5YTk5MGNlMDNmZWI3OCIsInNlcnZlcl9zZWNyZXRfc2hhcmVfZW5jcnlwdGVkIjoiMTc1OTA2YWQ0NWViY2E3ZmY1MzdkZDVlZDFmMzYzOTFlMDY2OWU5YzI4NDEzNjFmOWU5ODM3MjUyZmEzODA4MTgxZjQ5ODA0ZmRiNDhiN2E4MGVlZDk1NDZjZTU2YTAwNGNiMzNlMzczMTRmZTM5ZGI5ODU2NTg0YmYyZjZiNTM3Y2VmM2IxYmRjZjFmNDE5NzJmZDgyMmE2NWYwY2M2N2UwNjZhNGNkNmUwMWEyZTBmOGJiMGVlN2Q0OWE3NzhjMWMzYjM3MjY3NTA2ZmQ5NmQ3YzJiZGVjY2NiZjk3MjMxZTY0MWVhZjQ0MDM0NGM4OGVhMjNhZmM0NjJmYTA3ZGU1ODU5ZGFjMTRlYmU5N2Y0NTcwNWUzMTk5Zjc2YzRmNjlkNzVmMDhlOGY0YzJmNzg5MDFlZjZiOWJiYTk5NGQ3MzUxZGUwMDc3YTE1NDk4NzEyYWYwYTYzMmU0YjIyNTRjNmRhYzBjMWNiYWEzYzAyNzQ4ZTIxYjBmNTVmZmRlZTg0YTM0NTZkN2IyOWVmMTMzMWQ2YzE1NWVmZmY2NWJmYzY0MzVkYmEyMzIyOTQzNzlkYjE3YTRjZDU3ZDQ1NjAxMTc2ZjA4NjAxMzhiNjZhNGQ0YjVhMGNlMGI2ZTEzMmYzOWU1MDAzYWEzMmJiZGZlODI0Y2E3ODU1NGFlNmQ0MWM3NGE3ZDRjODA5NjVjOWRmY2QxZWYwMzU3MGZhNzA0NmFlZWRkNGM4YzZkZTkzNGUyYjIwNzVmMzg4ODZmOTA2MzBhOWM1ZDgwYjIxMGNhYTkwOGQzMjA0Mzg2NmM2OThmMTYyODEyNjNjNjZiM2VkOWZiNzA5MTNhNTYxNzU4MGNjYjEwOTdjNWY0YWUwNjYzYmZlYmFiMjRlODA0ODJhYWExOWE5ZjFhOTQ4NWJkMzU5M2ViNjFjMzNlYTVkZTE3NjFkMGY5OWIwZWFjOGMyNGJiNTYzYmNjYjJmYTVkMjkwMDUyNWM5YjYyODQzNjY1N2I0OGNlMjA0MTQ2NWFkOTE3ZGRkYWFmYzM3YmU2OTk2MWE0ODM4YWQ1ZWFkYjVhNDEyNDFmYTM2YWJmOTYyODI1N2JhMWM1YWQ3NTQ2NjgxMTQ5YzA1ZjU4YTE1MmRkY2UzN2RlMmY5MDJiYTNjMWJmNzgxYWZmNmQ3MTE2MTY3ZjhkNmNhMmZhODM4N2NmMzI2YzQzN2FkZDQ2Zjg5MWFmMjlmZjVjNjgyY2I3OGJhNzcwNzY5YTllNTVjM2JkZjc3NDg1NDQyMWQ4NWYwZjM5NjExY2E2NWFlZWMwZDQ3ZjY1ZGQxOTY2NTMxMTM0OTU4YTBiZjQ5ZGY3MDJkN2UxODg0NmRjYWU3NyJ9fSwicGFpbGxpZXJfcGsiOnsibiI6IjdlYmQ1YzU4NjY4MDA5YjU2NmFhM2ZlZjc0NzYzNTU1ZDg1Y2MxYTg3ZDhmY2E5OTY2MDg4YWVjZDBmMDM2N2UzMWFjZjMzZjg5NDZiMDkyMTUyNzFlMWE3NjcyYjIwMGY2ZmYzZWFkN2QxOTdhMGEyOTk5MmY2OWMwZWY5MDQ3MGNlZWE0YTM5MjVjZmIzZjVhOWRiZGU5ODlmYjM3NTU5M2MzNjZlYzFkMDJiNTMzYzdiZmZhYTg3Yzc4ZTY1NjU2NmMwMGYxZjU4YTAyNTAyYjUyNWZkNTA2ODNmODcyYmYzNjUxZGZmNDVmZTE4NGVjM2I4Y2Q1MzQ1ZjBiNDZmMDk1YjUwZjk4ZmMxMTExOTJlZThjZTM4ODMxOTAwMjI4YTI0MjM1NTgyNzBlMmIyNTBhNjRlNjAxM2YyYjkyYTVkMzEyNDUwNjQ4MmE0ZTIxOTlkYTE2YjVkY2I2ZjE2ZGJjYzdhZDYxN2JlOTQ3MTgxYTRkMTI4Y2I0MGQwMWM4NjRhNjJjYzBkM2EzMjcyNjE1Njg0YmMxZGVlZGFkMTc5MWI5NDYxZWQwYjkzZjBhYzVlZmMxYjlmNWEzNjM3MmZiOTNmNzJiZTY3ZjU5NTVjZmM2YmNkNmJkMjAxMmFmNmE0Y2Q0MjAwMjRhZGIwZWQ2N2I0YTQxMTBiOWUxIn0sInBheWxvYWRfcHVibGljX2tleSI6IjAyZWEwODg4ZTczMWUwM2U4MGNmN2E3YzkyNWQ2Nzc0ZjI0NzlmM2U5ZDBjNjU4YjFiNDYwNzgxY2M0OGQ2ZjRlNCIsInBheWxvYWRfc2lnbmluZ19rZXkiOiIwZDk3Y2M4YjhiMGM3OWUxOWIyMmYzOGQ1MGEzMTQ2M2Q0MjA5NjE5YzVmMTRmOTg5YWZlNTFiMmZjZjFhYzdlIiwidmVyc2lvbiI6MH0",
            "ab69e08c-61d0-4bde-8ed7-36bf45ee6d18", 
            None, 
            0, 
            Environment::Dev("app.dev3.nash.io/"), 
            1000
        ).await.unwrap()
    }

    #[test]
    fn end_to_end_dh_fill_pool() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();

        let async_block = async {
            let client = init_client().await;
            println!("Client ready!");
            let response = client
                .run(DhFillPoolRequest::new(Blockchain::Ethereum).unwrap())
                .await
                .unwrap();
            println!("{:?}", response);
            println!("{:?}", client.state.lock().await);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn end_to_end_asset_nonces() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_dev3_client().await;
            let response = client.run(AssetNoncesRequest::new()).await.unwrap();
            println!("{:?}", response);
            println!("{:?}", client.state.lock().await.asset_nonces);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn dependency_sign_all() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client.run(SignAllStates::new()).await.unwrap();
            println!("{:?}", response.response());
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn list_account_orders() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(ListAccountOrdersRequest {
                    before: None,
                    market: Market::eth_usdc(),
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
                        stop: Utc.ymd(2020, 9, 16).and_hms(0, 10, 0),
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
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(ListAccountTradesRequest {
                    before: Some("1598934832187000008".to_string()),
                    market: Market::eth_usdc(),
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
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(ListCandlesRequest {
                    before: None,
                    market: Market::eth_usdc(),
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
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(CancelAllOrders {
                    market: Market::eth_usdc(),
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
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(ListAccountBalancesRequest { filter: None })
                .await
                .unwrap();
            println!("{:?}", response);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn test_list_markets() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_dev3_client().await;
            let response = client
                .run(ListMarketsRequest)
                .await
                .unwrap();
            println!("{:?}", response);
            println!("{:?}", client.state.lock().await.assets);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn test_account_order_lookup_then_cancel() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_dev3_client().await;
            let mut requests = Vec::new();
            requests.push(
                LimitOrderRequest {
                    market: Market::neo_usdc(),
                    buy_or_sell: BuyOrSell::Buy,
                    amount: "10".to_string(),
                    price: "5".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            requests.push(
                LimitOrderRequest {
                    market: Market::neo_usdc(),
                    buy_or_sell: BuyOrSell::Sell,
                    amount: "1".to_string(),
                    price: "200".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            requests.push(
                LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Buy,
                    amount: "10".to_string(),
                    price: "5".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            requests.push(
                LimitOrderRequest {
                    market: Market::neo_usdc(),
                    buy_or_sell: BuyOrSell::Sell,
                    amount: "1".to_string(),
                    price: "200".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            requests.push(
                LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Buy,
                    amount: "10".to_string(),
                    price: "1".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            requests.push(
                LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Buy,
                    amount: "0.451".to_string(),
                    price: "75.1".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            requests.push(
                LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Sell,
                    amount: "1.24".to_string(),
                    price: "821".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            requests.push(
                LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Sell,
                    amount: "1.24".to_string(),
                    price: "821.12".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                }
            );
            for request in requests {
                let response = client
                    .run(request)
                    .await
                    .unwrap();
                println!("{:?}", response);
                let order_id = response.response().unwrap().order_id.clone();
                // Small delay to make sure it is processed
                tokio::time::delay_for(tokio::time::Duration::from_millis(500)).await;
                let response = client
                    .run(GetAccountOrderRequest {
                        order_id: order_id.clone(),
                    })
                    .await
                    .unwrap();
                println!("{:?}", response);
                let response = client
                    .run(CancelOrderRequest {
                        market: Market::eth_usdc(),
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
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(OrderbookRequest {
                    market: Market::eth_usdc(),
                })
                .await
                .unwrap();
            println!("{:?}", response);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn end_to_end_ticker() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(TickerRequest {
                    market: Market::eth_usdc(),
                })
                .await
                .unwrap();
            println!("{:?}", response);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn end_to_end_sub_orderbook() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let mut response = client
                .subscribe_protocol(SubscribeOrderbook {
                    market: Market::btc_usdc(),
                })
                .await
                .unwrap();
            let next_item = response.next().await.unwrap().unwrap();
            println!("{:?}", next_item);
            let next_item = response.next().await.unwrap().unwrap();
            println!("{:?}", next_item);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn sub_orderbook_via_client_stream() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            {
                let _response = client
                .subscribe_protocol(SubscribeOrderbook {
                    market: Market::btc_usdc(),
                })
                .await
                .unwrap();
            }
            let (item, client) = client.into_future().await;
            println!("{:?}", item.unwrap().unwrap());
            let (item, _) = client.into_future().await;
            println!("{:?}", item.unwrap().unwrap());
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn end_to_end_sell_limit_order() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Sell,
                    amount: "0.02".to_string(),
                    price: "800".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilTime(
                        Utc.ymd(2020, 12, 16).and_hms(0, 0, 0),
                    ),
                    allow_taker: true,
                })
                .await
                .unwrap();
            println!("{:?}", response);
            let order_id = response.response().unwrap().order_id.clone();
            let response = client
                .run(GetAccountOrderRequest { order_id })
                .await
                .unwrap();
            println!("{:?}", response);
            let response = client
                .run(CancelAllOrders {
                    market: Market::eth_usdc(),
                })
                .await
                .unwrap();
            println!("{:?}", response);
            assert_eq!(response.response().unwrap().accepted, true);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn end_to_end_buy_limit_order() {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Buy,
                    amount: "0.2".to_string(),
                    price: "50".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                })
                .await
                .unwrap();
            println!("{:?}", response);
            let response = client
                .run(CancelAllOrders {
                    market: Market::eth_usdc(),
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
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client.run(ListMarketsRequest).await.unwrap();
            println!("{:?}", response);
            println!("{:?}", client.state.lock().await.assets);
        };
        runtime.block_on(async_block);
    }
}
