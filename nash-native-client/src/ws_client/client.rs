//! Client implementation over websockets using channels and message brokers

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
                        break;
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

    async fn init_dev1_client() -> Client {
        Client::from_key_data(
            "eyJjaGlsZF9rZXlzIjp7Im0vNDQnLzAnLzAnLzAvMCI6eyJhZGRyZXNzIjoiMk43ZWRVNWdXMlVFNk5ETlFjTkJMa05ZVE5GRlI0TVJVSmYiLCJjbGllbnRfc2VjcmV0X3NoYXJlIjoiNzA5OGJkMmQ5MGU2MDgxYjc4MGEyMDJlMmE3MWMzN2FjOWFhMmU5NmY1MTY5ZDExNTA5NWFlYmVkYTgwZDY4NCIsInB1YmxpY19rZXkiOiIwMzg0YjVjNjI0MGM3YTQ2MzcwZGUzYWNlOTljOTAzMDE2ZGJiNmU0ZDdmNTk3MzE5ZTliOTA5MDViNmM5ZWJhMDIiLCJzZXJ2ZXJfc2VjcmV0X3NoYXJlX2VuY3J5cHRlZCI6IjE0ZTM2ODhlMmVjYmUwNWVhNjU4N2YyMjlmN2I2YWNkZmEyNzUyMzQ5MDZiOTIxNDhlNmZiNzRkMTFmOGI1ZTdhZGY4NTQxZDhiODcxYjgyYmYwMjBhYTY4NDEyZjQ0MGNiNTNiNTZiMGM5OGI5ZmNiY2QzZjliOWIxYzQxZjY4ZGE4MjcyNGIwOTMzNTljNTg4MjQxYTFhOTE3ZTU4NjU0MGE1Yzk1ZDRmY2E4ZTQzZWE3ZWI4NDMwZjhjNGNiNzRiYzVhMThlMzk5MjMwOGZiNzM1YzY5MGQ1YjM2NDg2MmRlMjEyNDU3NmRjMGQzNjhiMGJjZTc1MjA2ZDVlODkwMzY5YzhhMDA4MWNkYWFlNDQzNTg4NzJlZTI4YjE4MzFlYzdjODg3MTU4Njg3NjU0NDljOTk0ZTdiMWU0OWJmMzI5ODQ2M2QwYWZiZmJhMjAzYzIzMDg5Nzk5NDg4ZjBhYWU4MWUzM2ZlMGRmNzI2YjY3YmFmNzk0NzU0MDI5YzNhYTM3NTc1NjhjY2FiMTljYjdmMjdjZjgxOWQ2NzUwNzViZWVmN2FlMjJmZjkzMWY2YWMzNzc0NDg1MDc4YTYyMDU4MzI5NDIyNWIxMDNkNmFjN2IyMjVhMjY2YjI1M2Q4MTNiMTE4ZDYyNGViYzUwZjNjOWExYTJkN2I0N2JhZWExYmNiNDljODJmZGJjNDUyOWQ1NGQ4MDQ2ODE4YzIyM2EzNWQxODViNmExZmFkNDk1MDdiNDdmNTZhOWFkMjY4Y2E0NTYzOTc4NDVlZGZhYzg0ZWJhOTY4N2YxMDVhNWUyNDk1M2JlMTBjNGQ2YTBhYWIyNDVkYWMzNDIwZWM4NWM2MzFjMjJkYTYyZmRjYzc0MDdmZmExZWQ4NjNkZjc1YjQwMzdmZWRhNDAzNDY1YWI3OTJiMTE1MmE4NzY5NzY3NjZhZjYyNTZkZGYxNWIzYTVjNzlkNDBiYWQ4ZmJlNGY1MTQzY2I3NmRkZTYzOWUxY2RiYjU0YjJiYmNhN2YyOTk3ZjFjOWViODU3MzQxYTkyMTllNzEzMWQ3NTI5YTM1NDM0N2NkMDY4MmNkMmVjYWFhMTM1NTRmMTM4NTE1MTkzNDJlZTAyZTI1MDJmMDFjY2MzYTI3YWY4ZmVmYWI0NDk0NDg2MjBlNjIxNjRkYWRiZTMzMzJiYjM2NmJiNzQxOTE3MjMzZWI0ZTA4NmIxNGQ3ZmUxMTU5NWFhYmQzMTMwOTdhZjA4N2U0OTM4OGEwMWRlZjI4OWJjZGFhYmQ4OGEwODRkMzg1YjE4ZGZiZjJlOWZmYWZkMjg1Zjk3NzQ0YzgzOWVkNzhiODFiMGVlMjI4NWI2OGEwM2RkNDMifSwibS80NCcvNjAnLzAnLzAvMCI6eyJhZGRyZXNzIjoiZmI0ODRiNmIxMmE3YjRiMGQ5OTViZjI0NTVhMDJlNjZhNzgxZTE0ZiIsImNsaWVudF9zZWNyZXRfc2hhcmUiOiIyYjQ4MjExYWZkYzE5YmUwMWM0ZmVmOTFjZDU5OTNmYTIxNjllMDI5YTNlZDMwMTQwOTQxMTlmZGI1NTJlZTI2IiwicHVibGljX2tleSI6IjA0MGJlMzA0ZWU2ZGZmYTBlM2JmYWU4NzUyMjhiOTg2MWYzZDVjZWI0YzRjN2ExODhhN2Q0NGJkYzZhZmJlNTIwMGY1ODZjYjRhMDc2ZTYzYWNlODcyNTM1OGM3MTA1NWU2NWE3OTQ2NGQ5NmFlNzBhYWZiZmUzOGU0ODA4MjdmMmYiLCJzZXJ2ZXJfc2VjcmV0X3NoYXJlX2VuY3J5cHRlZCI6ImI3YjVmN2QwZjU3NTU4NjcxZWI1OTcyMGJmNDI2ZmZlY2M0ZjZkOGIyYTg4YWM1OGU5OTM3ZjI0NzVjZDNhMmNkMzM4MWJiNmZhMmJkMjk4MmM2MDUyOTJlNDFiMmMxMGE1ZmQxMmVmZjljNjE5YjUyZDg2MTcyZDg5ZjA5NDYwNjAwN2JmZmIyMjEyMDc0MTFkN2JjYTFkNTQ1YzQyYzUwYzY4OWRiNzhlZjFhMTk5YjNlNDJiZGQ1YmQ5NjE0ZjJhMmFkZWFmYjI0Y2Q0YzczMDdjMTdlOWM2YjNhNzA0NzMxZWQ3YzI0ZWFjY2M4NzI3NTE1MTQwYzIxZjQ5M2YzODliMDljN2Q3OWNjZjg4NmUzYmE4OWI5MTQ1NDBkZThmOWM5YzFiNTYyZTc2NDQzMzU1MWM2ZTM2ZWNkZDY3MThjYTMxMWZhNjkwY2FjNDIzM2ViOTlkMDY5Y2Y0MmE1NDkxNzI5YWE4NmZjNzkzOGFkZGQxYWI5OGZhYWUwNDRhZTBlZjdiMWNkYzhmZTliMGNhZWRjYjdkYTlkMzE5ZjQzNzkwYjdlMWI1MDE4N2UxYTgyZDFmYzIyOTQ2NDhlN2M2OGFkN2ViN2E1NDM3NTQxZWJiMDRhN2Q5Mzg4YjY4NDBkNWM3MmExODAxMWMwYzEwZDdhYjhmYjJhNmZhNjQ2MTM3MzJmYjZkZDM4YjYyYmZhMWJiY2YyOTI5ZTcyOWFmNjFiZmYxMDFlMmIzNzk2M2M0MjNlMGNiMjE3MmEwZTMzNzk4OGFjOTQ0YTgwMzQ5MjNlNDg0NTcwMzA0NTMzMjJiYzQ5MjIxZDc1Yzk0NDEyZWM1NWFiNmVlYWNiMTIzMzZiYWY2ZTI2M2JiZjBiNzFmMDYyMTcxMGVjYzRjMzM1YzhhMTkyMzRkZTg2YWUyYjM1ZDk5YzVhOWY2ODUzYTViYjkyZmQzNGEyNDlhY2E3MTQ4OWRkMjNiZmY4YmU0MzkxZjBiNWZmMTc5ZDJlNzMxOTdiZTU0NGVlOWFlYmVhMjVkYWFlNjAwMTJjOTEwYTE3YjQwYzNlYWI1YTFmYjc0NTM1YzU3NGEzMDU0NzMyZjNjMmUwNGEyMzhhYWE4YzRkZTFkMWE3NTljY2RkNzdmYWRiZTA2MDdiNTcwNDFiZmYwYTFkNGEyOWFkNjRkOWQ4YTg1M2ZmZmEzZjJlOTE1NzlkYTljYjc1ZDg5MDczMTIyMWM4MTdmMDZlMTMxYzFhNDdhOTg3ZjVhMDNhOTI2ODdiYjZhMzlmM2U1YzIyNzFlNWYyYjRhYzNmNTU3NmIzM2YzMzI5NDUyMzg5M2FlNWYyZDFhMjFjNGI5MzM4NjI0MWQzMjdhYyJ9LCJtLzQ0Jy84ODgnLzAnLzAvMCI6eyJhZGRyZXNzIjoiQVNvendIMURjR2daSlkxWHBKOFdFbWFOTDY3R2diWWRiQyIsImNsaWVudF9zZWNyZXRfc2hhcmUiOiIxZmZiNTY5YWFmMWM4ZTYxNTg0YzY5ZmNiY2VlYzU1ODY4MDdlNTUyN2IyMGNjNDBjNDA5NmFlNmQwYjc1ZTM5IiwicHVibGljX2tleSI6IjAyYTVmNGFiMzQ5YWU5MmJmMDJlNDAzYTU2YzdlNTBlNjVjMWIzNWFhMTFhMGEwNGQ1NjJiZmE0ZjhiZDU3OWY5ZSIsInNlcnZlcl9zZWNyZXRfc2hhcmVfZW5jcnlwdGVkIjoiMTZlMmU5ODg0ODRlZTRmMjc0OTI5MzRlZWJhZWViMzdkZDAxYWM1MTNiYmMzZGQ5MjhmM2Q0Y2EzZDM1NTNiMTE1YjBkZWZhNGNjN2Q0MzE1MmEwZDlkMGM4MDFhYTNkYWZjNTI3NmU1NWFlZDYzNzYwZjhkZjVhOTgxOWUyMGUzODlkZGY4ZTA3MGNmZGJmNTQzMGJhNWE4MGZiNDhkYjRiNzA2ODRkNzUxZGUyY2RkNmU4YTY1YWE4ZWJlYjdjNmM4OTliNTc2OTRlODdjODU5OWQxMzhjMDRmNTA1ZmU1NGFkMzYyMGM2MDVmMDhhODhmYjVlMDQ4Njg4NDMzM2NiOGZlNWE1NzFjZTA4ZjgyMWNhMWE2ZDM0MWUyMGQ5ZmY5ZDE5OTQyODI2MzQxM2NhMGUxNzMzMzAxY2MzOWZjZDRkN2ZjOGJkNDBhNjUxMDk0ZmRmNmEwZTNhYWMwZTE1NjBhMGRiNzI2YjllNmQ3ODI1YzllMTE4MjM4ZmFiOGU5NDRmOTFmNDZkMzE2ZDE4YjJkNWJjNDJkNjc5M2UxNjdiMTQ0OTg5NWU2YTc0Mjk2NzMzMzEwZDAxOTI3NDdiYzhmNjFkYjczMGZjZWQwODMxYzU3ZjI0ZmI5NGVkNmM3ZTQ4MzhiOWVmNWY0YzI2MjFkNzljNzA2MGViMjg0YTQ5ZmI1NzRmNTIzZGEyZjgxNjBmN2VlMzQ5NjAwNTJiZTZjYTE2ZWYzZGFiMjZkYzczZDY1MGFmMjg1N2E5MjQ3ODk2MGE5OTFlZDg3YmQwODc2YjMxMTk5MDZlOWIzMGVmNThhNDllYTYyZDAyYzI3OWVkYmU0NjhkNTVjMDdjNDA5Y2M0ZDliMjk1Njg4YzY2N2RhNWQzNzJkNmY4NWMwYTFhMjlkMmU0Y2EzZjY1NWVkMTRhN2RjNzIwMzM2ZWI4MTQyMjBiYmU4MjczNDYzYzRiYjVhZDQ5YWFmNTdkMDExODUxMzI5MWQ2MGE1ZjZkYTQ2Yjg1ZmVjZTkzMjkyOWRlN2JmZDZmOTkyY2YyN2U5NWY4ZDlmMTU1NmJjYzk2ODAzYzM1NzczZmI0Nzk1YTIyYmZiOTMwNTQxNmZkYzQ0YmI4ODZjZTc2NzRjYmRhYmZmNmIxODM3MGJjNDE1YTA0MWUwYjY5OGIwZjBlMmJhODk3NTVkODJkZTU1OGY0ZmQ3MTcyMmI3NDk4OTA1NTBkYzg5OTZlZjRiNzVmMzllYTQ1OWRhYWM0NGJiZWQyZWUwODNlNjBjMTg3ZDY2YTgwMjBmZjFmOGYyNjc1MzEyZjE5MzBjZmMwNDg4NmFlMWUyZmVjY2UzNmM1MDg4ODlkMGM0Mjg4NjlkNCJ9fSwicGFpbGxpZXJfcGsiOnsibiI6IjdlYmQ1YzU4NjY4MDA5YjU2NmFhM2ZlZjc0NzYzNTU1ZDg1Y2MxYTg3ZDhmY2E5OTY2MDg4YWVjZDBmMDM2N2UzMWFjZjMzZjg5NDZiMDkyMTUyNzFlMWE3NjcyYjIwMGY2ZmYzZWFkN2QxOTdhMGEyOTk5MmY2OWMwZWY5MDQ3MGNlZWE0YTM5MjVjZmIzZjVhOWRiZGU5ODlmYjM3NTU5M2MzNjZlYzFkMDJiNTMzYzdiZmZhYTg3Yzc4ZTY1NjU2NmMwMGYxZjU4YTAyNTAyYjUyNWZkNTA2ODNmODcyYmYzNjUxZGZmNDVmZTE4NGVjM2I4Y2Q1MzQ1ZjBiNDZmMDk1YjUwZjk4ZmMxMTExOTJlZThjZTM4ODMxOTAwMjI4YTI0MjM1NTgyNzBlMmIyNTBhNjRlNjAxM2YyYjkyYTVkMzEyNDUwNjQ4MmE0ZTIxOTlkYTE2YjVkY2I2ZjE2ZGJjYzdhZDYxN2JlOTQ3MTgxYTRkMTI4Y2I0MGQwMWM4NjRhNjJjYzBkM2EzMjcyNjE1Njg0YmMxZGVlZGFkMTc5MWI5NDYxZWQwYjkzZjBhYzVlZmMxYjlmNWEzNjM3MmZiOTNmNzJiZTY3ZjU5NTVjZmM2YmNkNmJkMjAxMmFmNmE0Y2Q0MjAwMjRhZGIwZWQ2N2I0YTQxMTBiOWUxIn0sInBheWxvYWRfcHVibGljX2tleSI6IjAyY2M0YTFiNTVkYmIwOWNiNWEwMmUxMjY5MDE2NjAxNWZjNTAxNGQzYjdhYzhjOGUzZjczMzAyZGFmODgwMDUxOCIsInBheWxvYWRfc2lnbmluZ19rZXkiOiJkZTM0NGU3YWQ3YjI5ZGQxNWFmY2ZkNGJkNmI4ZjUzZDNkMDA0YTRiNDA2YTI3OWUyM2UzZjZjNTdmMWU0OGI2IiwidmVyc2lvbiI6MH0=",
            "9147d5f9-2183-4c1f-8e8a-6cf868da570c", 
            None, 
            0, 
            Environment::Dev("app.dev1.nash.io/"), 
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
            let client = init_dev1_client().await;
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
            let client = init_dev1_client().await;
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
            let client = init_dev1_client().await;
            let response = client
                .run(LimitOrderRequest {
                    market: Market::eth_usdc(),
                    buy_or_sell: BuyOrSell::Buy,
                    amount: "0.041".to_string(),
                    price: "150".to_string(),
                    cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                    allow_taker: true,
                })
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
            let _response = client
                .subscribe_protocol(SubscribeOrderbook {
                    market: Market::btc_usdc(),
                })
                .await
                .unwrap();
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
