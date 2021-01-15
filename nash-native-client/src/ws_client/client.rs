//! Client implementation of Nash API over websockets using channels and message brokers

use std::any::type_name;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::{FutureExt, SinkExt, StreamExt};
use futures_util::future::{select, Either};
use tokio::{net::TcpStream, sync::mpsc, sync::oneshot, sync::RwLock, time::Duration};
use tokio_native_tls::TlsStream;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{connect_async, stream::Stream, WebSocketStream};
use tracing::{error, trace, warn, Instrument};

use nash_protocol::errors::{ProtocolError, Result};
use nash_protocol::protocol::subscriptions::SubscriptionResponse;
use nash_protocol::protocol::{
    NashProtocol, NashProtocolPipeline, NashProtocolSubscription, ResponseOrError, State,
};

use super::absinthe::{AbsintheEvent, AbsintheTopic, AbsintheWSRequest, AbsintheWSResponse};

type WebSocket = WebSocketStream<Stream<TcpStream, TlsStream<TcpStream>>>;

// this will add heartbeat (keep alive) messages to the channel for ws to send out every 15s
pub fn spawn_heartbeat_loop(
    period: Duration,
    client_id: u64,
    outgoing_sender: mpsc::UnboundedSender<(AbsintheWSRequest, Option<oneshot::Receiver<bool>>)>,
) {
    tokio::spawn(async move {
        loop {
            let heartbeat = AbsintheWSRequest::new(
                client_id,
                0, // todo: this needs to increment but not overlap with other message ids
                AbsintheTopic::Phoenix,
                AbsintheEvent::Heartbeat,
                None,
            );
            if let Err(_ignore) = outgoing_sender.send((heartbeat, None)) {
                // if outgoing sender is dead just ignore, will be handled elsewhere
                break;
            }
            tokio::time::sleep(period).await;
        }
    });
}

// this will receive messages to send out over websockets on one channel, and pass incoming ws messages
// back up to client on another channel.
pub fn spawn_sender_loop(
    timeout_duration: Duration,
    mut websocket: WebSocket,
    mut ws_outgoing_receiver: mpsc::UnboundedReceiver<(
        AbsintheWSRequest,
        Option<oneshot::Receiver<bool>>,
    )>,
    mut ws_disconnect_receiver: mpsc::UnboundedReceiver<()>,
    message_broker_link: mpsc::UnboundedSender<BrokerAction>,
) {
    tokio::spawn(async move {
        // The idea is that try_recv will only work when it receives a disconnect signal
        // This is a bit ugly imo and we should probably change in the future
        while ws_disconnect_receiver.try_recv().is_err() {
            // let ready_map = HashMap::new();
            let next_outgoing = ws_outgoing_receiver.recv().boxed();
            let next_incoming = tokio::time::timeout(timeout_duration, websocket.next());
            match select(next_outgoing, next_incoming).await {
                Either::Left((outgoing, _)) => {
                    if let Some((request, _ready_rx)) = outgoing {
                        if let Ok(request_raw) = serde_json::to_string(&request) {
                            // If sending fails, pass error through broker and global channel
                            match websocket.send(Message::Text(request_raw)).await {
                                Ok(_) => {
                                    trace!(id = ?request.message_id(), "WS-SEND success");
                                }
                                Err(e) => {
                                    error!(error = %e, "WS-SEND channel error");
                                    let error = ProtocolError("failed to send message on WS connection, likely disconnected");
                                    let _ = message_broker_link.send(BrokerAction::Message(Err(error)));
                                    break;
                                }
                            }
                        } else {
                            error!(request = ?request, "WS-SEND invalid request");
                        }
                    } else {
                        error!("WS-SEND channel errored");
                        let error = ProtocolError("outgoing channel died or errored, likely disconnected");
                        let _ = message_broker_link.send(BrokerAction::Message(Err(error)));
                        break;
                    }
                }
                Either::Right((incoming, _)) => {
                    if let Ok(incoming) = incoming {
                        if let Some(Ok(message)) = incoming {
                            let raw_response = message
                                .into_text()
                                .map_err(|e| {
                                    ProtocolError::coerce_static_from_str(e.to_string().as_str())
                                });
                            let response: Result<AbsintheWSResponse> = raw_response
                                .and_then(|r| serde_json::from_str(&r).map_err(|e| {
                                    ProtocolError::coerce_static_from_str(e.to_string().as_str())
                                }));
                            match response {
                                Ok(response) => {
                                    trace!(id = ?response.message_id(), "WS-RECV success");
                                    let _ = message_broker_link.send(BrokerAction::Message(Ok(response)));
                                },
                                Err(e) => {
                                    error!(error = %e, "WS-RECV invalid response message");
                                    let _ = message_broker_link.send(BrokerAction::Message(Err(e)));
                                    break;
                                }
                            }
                        } else {
                            error!("WS-RECV channel errored");
                            let error = ProtocolError("incoming channel died or errored, likely disconnected");
                            let _ = message_broker_link.send(BrokerAction::Message(Err(error)));
                            break;
                        }
                    } else {
                        error!("WS-RECV timed out");
                        let error = ProtocolError("incoming WS timed out, likely disconnected");
                        let _ = message_broker_link.send(BrokerAction::Message(Err(error)));
                        break;
                    }
                }
            };
        }
        let error =
            ProtocolError("Disconnected.");
        message_broker_link.send(BrokerAction::Message(Err(error))).ok();
    });
}

/// Broker will route responses to the right client channel based on message id lookup
pub enum BrokerAction {
    RegisterRequest(
        u64,
        oneshot::Sender<Result<AbsintheWSResponse>>,
        oneshot::Sender<bool>,
    ),
    RegisterSubscription(String, mpsc::UnboundedSender<Result<AbsintheWSResponse>>),
    Message(Result<AbsintheWSResponse>),
}

struct MessageBroker {
    link: mpsc::UnboundedSender<BrokerAction>,
}

impl MessageBroker {
    pub fn new() -> Self {
        let (link, mut internal_receiver) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut request_map = HashMap::new();
            let mut subscription_map = HashMap::new();
            loop {
                if let Some(next_incoming) = internal_receiver.next().await {
                    match next_incoming {
                        // Register a channel to send messages to with given id
                        BrokerAction::RegisterRequest(id, channel, _ready_tx) => {
                            trace!(%id, "BROKER registering request");
                            request_map.insert(id, channel);
                            //ready_tx.send(true);
                        }
                        BrokerAction::RegisterSubscription(id, channel) => {
                            trace!(%id, "BROKER registering subscription");
                            subscription_map.insert(id, channel);
                        }
                        // When message comes in, if id is registered with channel, send there
                        BrokerAction::Message(Ok(response)) => {
                            // if message has subscription id, send it to subscription
                            if let Some(id) = response.subscription_id() {
                                if let Some(channel) = subscription_map.get_mut(&id) {
                                    // Again, we will let client timeout on waiting a response using its own policy.
                                    // Crashing inside the broker process does not allow us to handle the error gracefully
                                    if let Err(_ignore) = channel.send(Ok(response)) {
                                        // Kill process on error
                                        break;
                                    }
                                }
                            }
                            // otherwise check if it is a response to a registered request
                            else if let Some(id) = response.message_id() {
                                if let Some(channel) = request_map.remove(&id) {
                                    if channel.send(Ok(response)).is_ok() {
                                        trace!(id, "BROKER dispatched response");
                                    } else {
                                        // Kill process on error
                                        break;
                                    }
                                } else {
                                    warn!(id, ?response, "BROKER response without return channel");
                                }
                            } else {
                                warn!(?response, "BROKER response without id");
                            }
                        }
                        BrokerAction::Message(Err(e)) => {
                            error!(error = %e, "BROKER propagating channel error");
                            // iterate over all subscription and request channels and propagate error
                            for (_id, channel) in subscription_map.drain() {
                                let _ = channel.send(Err(e.clone()));
                            }
                            for (_id, channel) in request_map.drain() {
                                let _ = channel.send(Err(e.clone()));
                            }
                            // kill broker process if WS connection closed
                            break;
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
    mut callback_channel: mpsc::UnboundedReceiver<Result<AbsintheWSResponse>>,
    user_callback_sender: mpsc::UnboundedSender<
        Result<ResponseOrError<<T as NashProtocolSubscription>::SubscriptionResponse>>,
    >,
    global_subscription_sender: mpsc::UnboundedSender<
        Result<ResponseOrError<SubscriptionResponse>>,
    >,
    request: T,
    state: Arc<RwLock<State>>,
) {
    tokio::spawn(async move {
        loop {
            let response = callback_channel.next().await;
            // is there a valid incoming payload?
            match response {
                Some(Ok(response)) => {
                    // can the payload json be parsed?
                    if let Ok(json_payload) = response.subscription_json_payload() {
                        // First do normal subscription logic
                        let output = match request
                            .subscription_response_from_json(json_payload.clone(), state.clone())
                            .await
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
                                            .process_subscription_response(
                                                sub_response,
                                                state.clone(),
                                            )
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
                        if let Err(_e) = global_subscription_sender.send(
                            request
                                .wrap_response_as_any_subscription(json_payload, state.clone())
                                .await,
                        ) {
                            break;
                        }
                    } else {
                        // Kill process due to unparsable absinthe payload
                        break;
                    }
                }
                Some(Err(e)) => {
                    // kill process due to closed channel
                    let _ = global_subscription_sender.send(Err(e));
                    // if for some reason the global subscription doesn't exist anymore (likely because client doesn't exist!) then just ignore
                    // and close out the process loop
                    break;
                }
                None => {
                    let _ = global_subscription_sender
                        .send(Err(ProtocolError("channel returned None. dead?")));
                    break;
                }
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
            Self::Sandbox => "app.sandbox.nash.io",
            Self::Dev(s) => s,
        }
    }
}

// FIXME
async fn manage_client_error(state: Arc<RwLock<State>>) {
    let mut state = state.write().await;
    // quick fix, on any client error trigger an asset nonces refresh
    // in future, next step would be to destructure the error recieved from ME
    // passed via an extra argument and act on client state appropriately
    state.assets_nonces_refresh = true;
}

/// Interface for interacting with a websocket connection
pub struct InnerClient {
    ws_outgoing_sender: mpsc::UnboundedSender<(AbsintheWSRequest, Option<oneshot::Receiver<bool>>)>,
    ws_disconnect_sender: mpsc::UnboundedSender<()>,
    next_message_id: Arc<AtomicU64>,
    client_id: u64,
    message_broker: MessageBroker,
    state: Arc<RwLock<State>>,
    timeout: Duration,
    global_subscription_sender:
        mpsc::UnboundedSender<Result<ResponseOrError<SubscriptionResponse>>>,
}

impl InnerClient {
    /// Create a new client using an optional `keys_path`. The `client_id` is an identifier
    /// registered with the absinthe WS connection. It can possibly be removed.
    pub async fn new(
        keys_path: Option<&str>,
        client_id: u64,
        affiliate_code: Option<String>,
        env: Environment,
        timeout: Duration,
    ) -> Result<(
        Self,
        mpsc::UnboundedReceiver<Result<ResponseOrError<SubscriptionResponse>>>,
    )> {
        let state = State::from_keys_path(keys_path)?;
        Self::client_setup(state, client_id, affiliate_code, env, timeout).await
    }
    /// Create a client using a base64 encoded keylist and session id (contents of Nash produced .json file)
    pub async fn from_key_data(
        secret: &str,
        session: &str,
        affiliate_code: Option<String>,
        client_id: u64,
        env: Environment,
        timeout: Duration,
    ) -> Result<(
        Self,
        mpsc::UnboundedReceiver<Result<ResponseOrError<SubscriptionResponse>>>,
    )> {
        let state = State::from_keys(secret, session)?;
        Self::client_setup(state, client_id, affiliate_code, env, timeout).await
    }

    pub async fn disconnect(&self) {
        self.ws_disconnect_sender.send(()).ok();
    }
    /// Main client setup logic
    async fn client_setup(
        mut state: State,
        client_id: u64,
        affiliate_code: Option<String>,
        env: Environment,
        timeout: Duration,
    ) -> Result<(
        Self,
        mpsc::UnboundedReceiver<Result<ResponseOrError<SubscriptionResponse>>>,
    )> {
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
        let (socket, _response) = connect_async(&conn_path).await.map_err(|error| {
            ProtocolError::coerce_static_from_str(&format!("Could not connect to WS: {}", error))
        })?;

        // channels to pass messages between threads. bounded at 100 unprocessed
        let (ws_outgoing_sender, ws_outgoing_receiver) = mpsc::unbounded_channel();
        let (ws_disconnect_sender, ws_disconnect_receiver) = mpsc::unbounded_channel();

        let (global_subscription_sender, global_subscription_receiver) = mpsc::unbounded_channel();

        let message_broker = MessageBroker::new();

        // This will loop over WS connection, send things out, and route things in
        spawn_sender_loop(
            timeout,
            socket,
            ws_outgoing_receiver,
            ws_disconnect_receiver,
            message_broker.link.clone(),
        );

        // initialize the connection (first message id, 1)
        let message_id = 1;
        ws_outgoing_sender
            .send((AbsintheWSRequest::init_msg(client_id, message_id), None))
            .map_err(|_| ProtocolError("Could not initialize connection with Nash"))?;

        // start a heartbeat loop
        spawn_heartbeat_loop(timeout, client_id, ws_outgoing_sender.clone());

        let client = Self {
            ws_outgoing_sender,
            ws_disconnect_sender,
            next_message_id: Arc::new(AtomicU64::new(message_id + 1)),
            client_id,
            message_broker,
            state: Arc::new(RwLock::new(state)),
            timeout,
            global_subscription_sender,
        };

        // grab market data upon initial setup
        let _ = client
            .run(nash_protocol::protocol::list_markets::ListMarketsRequest)
            .await?;

        Ok((client, global_subscription_receiver))
    }

    /// Execute a NashProtocol request. Query will be created, executed over network, response will
    /// be passed to the protocol's state update hook, and response will be returned. Used by the even
    /// more generic `run(..)`.
    async fn execute_protocol<T: NashProtocol + Sync>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<T::Response>> {
        let query = request.graphql(self.state.clone()).await?;
        let ws_response = tokio::time::timeout(self.timeout, self.request(query).await?)
            .await
            .map_err(|_| ProtocolError("Request timeout"))?
            .map_err(|_| ProtocolError("Failed to receive response from return channel"))??;
        let json_payload = ws_response.json_payload()?;
        let protocol_response = request
            .response_from_json(json_payload, self.state.clone())
            .await?;
        if let Some(response) = protocol_response.response() {
            request
                .process_response(response, self.state.clone())
                .await?;
        }
        Ok(protocol_response)
    }

    /// Main entry point to execute Nash API requests. Capable of running anything that implements `NashProtocolPipeline`.
    /// All `NashProtocol` requests automatically do. Other more complex multi-stage interactions like `SignAllStates`
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
        let mut permit = None;
        if let Some(semaphore) = request.get_semaphore(self.inner.state.clone()).await {
            permit = Some(semaphore.acquire().await);
        }
        let mut protocol_state = request.init_state(self.state.clone()).await;
        // While pipeline contains more actions for client to take, execute them
        loop {
            if let Some(protocol_request) = request
                .next_step(&protocol_state, self.state.clone())
                .await?
            {
                let protocol_response = self.execute_protocol(protocol_request).await?;
                // If error, end pipeline early and return GraphQL/network error data
                if protocol_response.is_error() {
                    manage_client_error(self.state.clone()).await;

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
        mpsc::UnboundedReceiver<
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
            .await
            .map_err(|_| ProtocolError("Could not get subscription response"))??;
        // create a channel where associated data will be pushed back
        let (for_broker, callback_channel) = mpsc::unbounded_channel();
        let broker_link = self.message_broker.link.clone();
        // use subscription id on the response we got back from the subscription query
        // to register incoming data with the broker
        let subscription_id = subscription_response
            .subscription_setup_id()
            .ok_or(ProtocolError("Response does not include subscription id"))?;
        broker_link
            .send(BrokerAction::RegisterSubscription(
                subscription_id,
                for_broker,
            ))
            .map_err(|_| ProtocolError("Could not register subscription with broker"))?;

        let global_subscription_sender = self.global_subscription_sender.clone();
        let (user_callback_sender, user_callback_receiver) = mpsc::unbounded_channel();

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
    ) -> Result<oneshot::Receiver<Result<AbsintheWSResponse>>> {
        let message_id = self.incr_id().await;
        let graphql_msg = AbsintheWSRequest::new(
            self.client_id,
            message_id,
            AbsintheTopic::Control,
            AbsintheEvent::Doc,
            Some(request),
        );
        // create a channel where message broker will push a response when it gets one
        let (for_broker, callback_channel) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let broker_link = self.message_broker.link.clone();
        // register that channel in the broker with our message id
        broker_link
            .send(BrokerAction::RegisterRequest(
                message_id, for_broker, ready_tx,
            ))
            .map_err(|_| ProtocolError("Could not register request with broker"))?;
        // send the query
        self.ws_outgoing_sender
            .send((graphql_msg, Some(ready_rx)))
            .map_err(|_| ProtocolError("Request failed to send over channel"))?;
        // return response from the message broker when it comes
        Ok(callback_channel)
    }

    pub async fn incr_id(&self) -> u64 {
        self.next_message_id.fetch_add(1, Ordering::SeqCst)
    }
}

// Adding another layer of abstraction on top of client make it easier/safer to manage concurrent usage of the client
// In particular this is necessary to spawn a process in the background that signs state
pub struct Client {
    inner: Arc<InnerClient>,
    sign_loop_status: Arc<tokio::sync::Mutex<bool>>,
    pub(crate) global_subscription_receiver:
        mpsc::UnboundedReceiver<Result<ResponseOrError<SubscriptionResponse>>>,
}

impl Client {
    /// Initialize client using a file path to API keys
    pub async fn new(
        keys_path: Option<&str>,
        client_id: u64,
        affiliate_code: Option<String>,
        env: Environment,
        timeout: Duration,
        sign_states_loop_interval: Option<u64>,
    ) -> Result<Self> {
        let state = State::from_keys_path(keys_path)?;
        Self::setup(
            state,
            affiliate_code,
            client_id,
            env,
            timeout,
            sign_states_loop_interval,
        )
        .await
    }

    /// Initialize client from key data directly
    pub async fn from_key_data(
        secret: &str,
        session: &str,
        affiliate_code: Option<String>,
        client_id: u64,
        env: Environment,
        timeout: Duration,
        sign_states_loop_interval: Option<u64>,
    ) -> Result<Self> {
        let state = State::from_keys(secret, session)?;
        Self::setup(
            state,
            affiliate_code,
            client_id,
            env,
            timeout,
            sign_states_loop_interval,
        )
        .await
    }

    // can be used by market makers to turn off state signing
    pub async fn turn_off_sign_states(&self) {
        let client = self.inner.lock().await;
        let mut state = client.state.lock().await;
        state.dont_sign_states = true;
    }

    async fn setup(
        state: State,
        affiliate_code: Option<String>,
        client_id: u64,
        env: Environment,
        timeout: Duration,
        sign_states_loop_interval: Option<u64>,
    ) -> Result<Self> {
        let (client, g_sub) =
            InnerClient::client_setup(state, client_id, affiliate_code, env, timeout).await?;
        let client = Arc::new(client);
        let sign_loop_status = Arc::new(tokio::sync::Mutex::new(false));
        let client = Self {
            inner: client,
            global_subscription_receiver: g_sub,
            sign_loop_status,
        };
        if let Some(period) = sign_states_loop_interval {
            client.init_state_signing(period);
        }
        Ok(client)
    }

    /// Disconnect the client
    pub async fn disconnect(&self) {
        self.inner.ws_disconnect_sender.send(()).ok();
    }

    /// Entry point for Nash protocol requests
    pub async fn run<T: NashProtocolPipeline + Clone + Sync>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        async {
            let result = self.inner.run(request).await;
            if let Err(ref e) = result {
                error!(error = %e, "request errored");
            }
            result
        }
        .instrument(tracing::info_span!(
            "CLIENT RUN",
            request = type_name::<T>()
        ))
        .await
    }

    /// Entry point for running Nash protocol subscriptions
    pub async fn subscribe_protocol<T>(
        &self,
        request: T,
    ) -> Result<
        mpsc::UnboundedReceiver<
            Result<ResponseOrError<<T as NashProtocolSubscription>::SubscriptionResponse>>,
        >,
    >
    where
        T: NashProtocolSubscription + Send + Sync + 'static,
    {
        self.inner.subscribe_protocol(request).await
    }

    pub fn init_state_signing(&self, period: u64) {
        let client = self.inner.clone();
        let sign_loop_status = self.sign_loop_status.clone();
        tokio::spawn(async move {
            // set loop status to true once it begins, then free the lock
            // holding the lock throughout the loop wouldn't allow access to value
            let mut status = sign_loop_status.lock().await;
            *status = true;
            drop(status);
            loop {
                let state_lock = client.state.read().await;
                if state_lock.remaining_orders < 10 {
                    println!(
                        "SignAllStates triggered, {} remaining orders",
                        state_lock.remaining_orders
                    );
                    // running the client requires a free lock...
                    drop(state_lock);
                    let req = client
                        .run(nash_protocol::protocol::sign_all_states::SignAllStates::new())
                        .await;
                    if req.is_err() {
                        let mut status = sign_loop_status.lock().await;
                        *status = false;
                        // kill process
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_secs(period)).await;
            }
        });
    }

    /// Get status of state signing loop
    pub async fn state_signing_loop_status(&self) -> bool {
        self.sign_loop_status.lock().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use chrono::offset::TimeZone;
    use chrono::Utc;
    use dotenv::dotenv;
    use futures_util::StreamExt;
    use tokio::time::Duration;

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
    use nash_protocol::types::{
        Blockchain, BuyOrSell, DateTimeRange, OrderCancellationPolicy, OrderStatus, OrderType,
    };

    use super::{Arc, Client, Environment, HashMap};
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn init_client() -> Client {
        dotenv().ok();
        let secret = std::env::var("NASH_API_SECRET").expect("Couldn't get environment variable.");
        let session = std::env::var("NASH_API_KEY").expect("Couldn't get environment variable.");
        Client::from_key_data(
            &secret,
            &session,
            None,
            0,
            Environment::Production,
            Duration::from_secs_f32(60.0),
            None,
        )
        .await
        .unwrap()
    }

    async fn init_sandbox_client() -> Client {
        Client::new(
            None,
            0,
            None,
            Environment::Sandbox,
            Duration::from_secs_f32(5.0),
            None,
        )
        .await
        .unwrap()
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
            let mut counter_1 = counter.clone();
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
            let mut counter_2 = counter.clone();
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
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Buy,
                amount: "10".to_string(),
                price: "5".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            });
            requests.push(LimitOrderRequest {
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Sell,
                amount: "1".to_string(),
                price: "200".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            });
            requests.push(LimitOrderRequest {
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Buy,
                amount: "10".to_string(),
                price: "5".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            });
            requests.push(LimitOrderRequest {
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Sell,
                amount: "1".to_string(),
                price: "200".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            });
            requests.push(LimitOrderRequest {
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Buy,
                amount: "10".to_string(),
                price: "1".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            });
            requests.push(LimitOrderRequest {
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Buy,
                amount: "0.451".to_string(),
                price: "75.1".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            });
            requests.push(LimitOrderRequest {
                market: "eth_usdc".to_string(),
                buy_or_sell: BuyOrSell::Sell,
                amount: "1.24".to_string(),
                price: "821".to_string(),
                cancellation_policy: OrderCancellationPolicy::GoodTilCancelled,
                allow_taker: true,
            });
            requests.push(LimitOrderRequest {
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
            let next_item = response.next().await.unwrap().unwrap();
            println!("{:?}", next_item);
            let next_item = response.next().await.unwrap().unwrap();
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
            let next_item = response.next().await.unwrap().unwrap();
            println!("{:?}", next_item);
            let next_item = response.next().await.unwrap().unwrap();
            println!("{:?}", next_item);
        };
        runtime.block_on(async_block);
    }

    #[test]
    fn end_to_end_buy_eth_btc() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let async_block = async {
            let client = init_client().await;
            let response = client
                .run(LimitOrderRequest {
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
                let item = response.next().await;
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
}
