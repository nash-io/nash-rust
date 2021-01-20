//! Client implementation of Nash API over websockets using channels and message brokers

use std::any::type_name;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_recursion::async_recursion;
use futures::{FutureExt, SinkExt, StreamExt};
use futures_util::future::{select, Either};
use rand::Rng;
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

use crate::http_extension::HttpClientState;
use crate::Environment;

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
        while ws_disconnect_receiver.recv().now_or_never().is_none() {
            // let ready_map = HashMap::new();
            let next_outgoing = ws_outgoing_receiver.recv();
            tokio::pin!(next_outgoing);
            let next_incoming = tokio::time::timeout(timeout_duration, websocket.next());
            tokio::pin!(next_incoming);
            match select(next_outgoing, next_incoming).await {
                Either::Left((outgoing, _)) => {
                    if let Some((request, _ready_rx)) = outgoing {
                        if let Ok(request_raw) = serde_json::to_string(&request) {
                            // If sending fails, pass error through broker and global channel
                            match websocket.send(Message::Text(request_raw)).await {
                                Ok(_) => {
                                    trace!(id = ?request.message_id(), "SEND");
                                }
                                Err(e) => {
                                    error!(error = %e, "SEND channel error");
                                    let error = ProtocolError("failed to send message on WS connection, likely disconnected");
                                    let _ =
                                        message_broker_link.send(BrokerAction::Message(Err(error)));
                                    break;
                                }
                            }
                        } else {
                            error!(request = ?request, "SEND invalid request");
                        }
                    } else {
                        error!("SEND channel error");
                        let error =
                            ProtocolError("outgoing channel died or errored, likely disconnected");
                        let _ = message_broker_link.send(BrokerAction::Message(Err(error)));
                        break;
                    }
                }
                Either::Right((incoming, _)) => {
                    if let Ok(incoming) = incoming {
                        if let Some(Ok(message)) = incoming {
                            let raw_response = message.into_text().map_err(|e| {
                                ProtocolError::coerce_static_from_str(e.to_string().as_str())
                            });
                            let response: Result<AbsintheWSResponse> = raw_response.and_then(|r| {
                                serde_json::from_str(&r).map_err(|e| {
                                    ProtocolError::coerce_static_from_str(e.to_string().as_str())
                                })
                            });
                            match response {
                                Ok(response) => {
                                    trace!(id = ?response.message_id(), "RECV success");
                                    let _ = message_broker_link
                                        .send(BrokerAction::Message(Ok(response)));
                                }
                                Err(e) => {
                                    error!(error = %e, "RECV invalid response message");
                                    let _ = message_broker_link.send(BrokerAction::Message(Err(e)));
                                    break;
                                }
                            }
                        } else {
                            error!("RECV channel error");
                            let error = ProtocolError(
                                "incoming channel died or errored, likely disconnected",
                            );
                            let _ = message_broker_link.send(BrokerAction::Message(Err(error)));
                            break;
                        }
                    } else {
                        error!("RECV timed out");
                        let error = ProtocolError("incoming WS timed out, likely disconnected");
                        let _ = message_broker_link.send(BrokerAction::Message(Err(error)));
                        break;
                    }
                }
            };
        }
        error!("DISCONNECT");
        let error = ProtocolError("Disconnected.");
        message_broker_link
            .send(BrokerAction::Message(Err(error)))
            .ok();
    });
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
            let response = callback_channel.recv().await;
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
                if let Some(next_incoming) = internal_receiver.recv().await {
                    match next_incoming {
                        // Register a channel to send messages to with given id
                        BrokerAction::RegisterRequest(id, channel, _ready_tx) => {
                            trace!(%id, "BROKER request");
                            request_map.insert(id, channel);
                            //ready_tx.send(true);
                        }
                        BrokerAction::RegisterSubscription(id, channel) => {
                            trace!(%id, "BROKER subscription");
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
                                        trace!(id, "BROKER response");
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
                            error!(error = %e, "BROKER channel error");
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

pub struct WsClientState {
    ws_outgoing_sender: mpsc::UnboundedSender<(AbsintheWSRequest, Option<oneshot::Receiver<bool>>)>,
    ws_disconnect_sender: mpsc::UnboundedSender<()>,
    global_subscription_sender:
        mpsc::UnboundedSender<Result<ResponseOrError<SubscriptionResponse>>>,
    next_message_id: Arc<AtomicU64>,
    message_broker: MessageBroker,

    client_id: u64,
    timeout: Duration,
}

impl WsClientState {
    fn incr_message_id(&self) -> u64 {
        self.next_message_id.fetch_add(1, Ordering::SeqCst)
    }
}

pub struct InnerClient {
    pub(crate) ws_state: WsClientState,
    pub(crate) http_state: HttpClientState,
    pub(crate) state: Arc<RwLock<State>>,
}

impl InnerClient {
    pub async fn setup(
        mut state: State,
        client_id: u64,
        env: Environment,
        timeout: Duration,
        affiliate_code: Option<String>,
    ) -> Result<(
        Self,
        mpsc::UnboundedReceiver<Result<ResponseOrError<SubscriptionResponse>>>,
    )> {
        state.affiliate_code = affiliate_code;
        let (ws_state, global_subscription_receiver) =
            Self::setup_ws(&mut state, client_id, env, timeout).await?;
        let http_state = Self::setup_http(&mut state, env, timeout).await?;
        let client = InnerClient {
            ws_state,
            http_state,
            state: Arc::new(RwLock::new(state)),
        };
        Ok((client, global_subscription_receiver))
    }
    /// Init logic for websocket client
    pub(crate) async fn setup_ws(
        state: &mut State,
        client_id: u64,
        env: Environment,
        timeout: Duration,
    ) -> Result<(
        WsClientState,
        mpsc::UnboundedReceiver<Result<ResponseOrError<SubscriptionResponse>>>,
    )> {
        let version = "2.0.0";
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

        let client_state = WsClientState {
            ws_outgoing_sender,
            ws_disconnect_sender,
            global_subscription_sender,
            message_broker,
            next_message_id: Arc::new(AtomicU64::new(message_id + 1)),
            client_id,
            timeout,
        };
        Ok((client_state, global_subscription_receiver))
    }

    /// Execute a serialized NashProtocol request via websockets
    async fn request(
        &self,
        request: serde_json::Value,
    ) -> Result<oneshot::Receiver<Result<AbsintheWSResponse>>> {
        let message_id = self.ws_state.incr_message_id();
        let graphql_msg = AbsintheWSRequest::new(
            self.ws_state.client_id,
            message_id,
            AbsintheTopic::Control,
            AbsintheEvent::Doc,
            Some(request),
        );
        // create a channel where message broker will push a response when it gets one
        let (for_broker, callback_channel) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let broker_link = self.ws_state.message_broker.link.clone();
        // register that channel in the broker with our message id
        trace!(id = %message_id, "attached id");
        broker_link
            .send(BrokerAction::RegisterRequest(
                message_id, for_broker, ready_tx,
            ))
            .map_err(|_| ProtocolError("Could not register request with broker"))?;
        // send the query
        self.ws_state
            .ws_outgoing_sender
            .send((graphql_msg, Some(ready_rx)))
            .map_err(|_| ProtocolError("Request failed to send over channel"))?;
        // return response from the message broker when it comes
        Ok(callback_channel)
    }

    /// Execute a NashProtocol request. Query will be created, executed over network, response will
    /// be passed to the protocol's state update hook, and response will be returned. Used by the even
    /// more generic `run(..)`.
    async fn execute_protocol<T: NashProtocol>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<T::Response>> {
        let query = request.graphql(self.state.clone()).await?;
        let ws_response = tokio::time::timeout(self.ws_state.timeout, self.request(query).await?)
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

    #[async_recursion]
    pub async fn run<T: NashProtocolPipeline + Clone>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        async {
            let response = {
                if let Some(semaphore) = request.get_semaphore(self.state.clone()).await {
                    let _permit = semaphore.acquire().await;
                    self.run_helper(request).await
                } else {
                    self.run_helper(request).await
                }
            };
            if let Err(ref e) = response {
                error!(error = %e, "request error");
            }
            response
        }
        .instrument(tracing::info_span!(
                "RUN (ws)",
                request = type_name::<T>(),
                id = %rand::thread_rng().gen::<u32>()))
        .await
    }

    /// Does the main work of running a pipeline via websockets
    async fn run_helper<T: NashProtocolPipeline + Clone>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        trace!("running pre-hooks");
        // First run any dependencies of the request/pipeline
        let before_actions = request.run_before(self.state.clone()).await?;
        if let Some(actions) = before_actions {
            for action in actions {
                self.run(action).await?;
            }
        }
        trace!("running main pipeline");
        // Now run the pipeline
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
                    Self::manage_client_error(self.state.clone()).await;

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
        trace!("running post-hooks");
        // Get things to run after the request/pipeline
        let after_actions = request.run_after(self.state.clone()).await?;
        // Now run anything specified for after the pipeline
        if let Some(actions) = after_actions {
            for action in actions {
                self.run(action).await?;
            }
        }
        // Return the pipeline output
        request.output(protocol_state)
    }

    /// Entry point for running Nash protocol subscriptions
    pub async fn subscribe_protocol<T: NashProtocolSubscription + Send + Sync + 'static>(
        &self,
        request: T,
    ) -> Result<
        mpsc::UnboundedReceiver<
            Result<ResponseOrError<<T as NashProtocolSubscription>::SubscriptionResponse>>,
        >,
    > {
        let query = request.graphql(self.state.clone()).await?;
        // a subscription starts with a normal request
        let subscription_response = self
            .request(query)
            .await?
            .await
            .map_err(|_| ProtocolError("Could not get subscription response"))??;
        // create a channel where associated data will be pushed back
        let (for_broker, callback_channel) = mpsc::unbounded_channel();
        let broker_link = self.ws_state.message_broker.link.clone();
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

        let (user_callback_sender, user_callback_receiver) = mpsc::unbounded_channel();

        global_subscription_loop(
            callback_channel,
            user_callback_sender,
            self.ws_state.global_subscription_sender.clone(),
            request.clone(),
            self.state.clone(),
        );

        Ok(user_callback_receiver)
    }

    pub async fn disconnect(&self) {
        self.ws_state.ws_disconnect_sender.send(()).ok();
    }

    // FIXME
    pub async fn manage_client_error(state: Arc<RwLock<State>>) {
        let mut state = state.write().await;
        // quick fix, on any client error trigger an asset nonces refresh
        // in future, next step would be to destructure the error recieved from ME
        // passed via an extra argument and act on client state appropriately
        state.assets_nonces_refresh = true;
    }
}

pub struct Client {
    pub(crate) inner: Arc<InnerClient>,
    pub(crate) global_subscription_receiver:
        mpsc::UnboundedReceiver<Result<ResponseOrError<SubscriptionResponse>>>,
}

impl Client {
    /// Create a new client using an optional `keys_path`. The `client_id` is an identifier
    /// registered with the absinthe WS connection. It can possibly be removed.
    pub async fn from_keys_path(
        keys_path: Option<&str>,
        client_id: u64,
        affiliate_code: Option<String>,
        env: Environment,
        timeout: Duration,
    ) -> Result<Self> {
        let state = State::from_keys_path(keys_path)?;
        Self::setup(state, affiliate_code, client_id, env, timeout).await
    }

    /// Create a client using a base64 encoded keylist and session id (contents of Nash produced .json file)
    pub async fn from_keys(
        secret: &str,
        session: &str,
        affiliate_code: Option<String>,
        client_id: u64,
        env: Environment,
        timeout: Duration,
    ) -> Result<Self> {
        let state = State::from_keys(secret, session)?;
        Self::setup(state, affiliate_code, client_id, env, timeout).await
    }

    async fn setup(
        state: State,
        affiliate_code: Option<String>,
        client_id: u64,
        env: Environment,
        timeout: Duration,
    ) -> Result<Self> {
        let (inner, global_subscription_receiver) =
            InnerClient::setup(state, client_id, env, timeout, affiliate_code).await?;
        let client = Self {
            inner: Arc::new(inner),
            global_subscription_receiver,
        };
        // Grab market data upon initial setup
        let _ = client
            .run(nash_protocol::protocol::list_markets::ListMarketsRequest)
            .await?;
        Ok(client)
    }

    /// Main entry point to execute Nash API requests via websockets. Capable of running anything that implements `NashProtocolPipeline`.
    /// All `NashProtocol` requests automatically do. Other more complex multi-stage interactions like `SignAllStates`
    /// implement the trait manually. This will optionally run before and after hooks if those are defined for the pipeline
    /// or request (e.g., get asset nonces if they don't exist)
    #[inline]
    pub async fn run<T: NashProtocolPipeline + Clone>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        self.inner.run(request).await
    }

    /// Entry point for running Nash protocol subscriptions
    #[inline]
    pub async fn subscribe_protocol<T: NashProtocolSubscription + Send + Sync + 'static>(
        &self,
        request: T,
    ) -> Result<
        mpsc::UnboundedReceiver<
            Result<ResponseOrError<<T as NashProtocolSubscription>::SubscriptionResponse>>,
        >,
    > {
        self.inner.subscribe_protocol(request).await
    }

    /// Disconnect the websockets
    #[inline]
    pub async fn disconnect(&self) {
        self.inner.disconnect().await;
    }

    pub fn start_background_state_signing(&self, interval: Duration) {
        let weak_inner = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            while let Some(inner) = weak_inner.upgrade() {
                let tick_start = tokio::time::Instant::now();
                let remaining_orders = inner.state.read().await.remaining_orders;
                if remaining_orders < 10 {
                    trace!(%remaining_orders, "sign_all_states triggered");
                    let request = inner
                        .run(nash_protocol::protocol::sign_all_states::SignAllStates::new())
                        .await;
                    if let Err(ref e) = request {
                        error!(error = %e, "sign_all_states errored");
                    }
                }
                tokio::time::sleep_until(tick_start + interval).await;
            }
        });
    }

    /// Can be used by market makers to turn off state signing
    pub async fn turn_off_sign_states(&self) {
        let mut state = self.inner.state.write().await;
        state.dont_sign_states = true;
    }
}
