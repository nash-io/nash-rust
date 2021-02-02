//! Client implementation of Nash API over http

use std::any::type_name;
use std::time::Duration;

use async_recursion::async_recursion;
use rand::Rng;
use reqwest::header::AUTHORIZATION;
use tracing::{error, Instrument};

use nash_protocol::errors::{ProtocolError, Result};
use nash_protocol::protocol::{NashProtocol, NashProtocolPipeline, ResponseOrError, State};

use crate::types::Environment;
use crate::ws_client::{Client, InnerClient};

pub(crate) struct HttpClientState {
    client: reqwest::Client,
    api_url: String,
    auth_token: Option<String>,
}

impl InnerClient {
    /// Init internal http client
    pub(crate) async fn setup_http(
        state: &mut State,
        env: Environment,
        timeout: Duration,
    ) -> Result<HttpClientState> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|_| ProtocolError("Could not initialize reqwest client"))?;
        let api_url = format!("https://{}/api/graphql", env.url());
        let auth_token = state
            .signer
            .as_ref()
            .map(|s| format!("Token {}", s.api_keys.session_id));
        Ok(HttpClientState {
            client,
            api_url,
            auth_token,
        })
    }

    /// Execute a serialized NashProtocol request via http
    async fn request_http(&self, request: serde_json::Value) -> Result<serde_json::Value> {
        // Do simple request/response...
        let mut request = self
            .http_state
            .client
            .post(&self.http_state.api_url)
            .json(&request);
        if let Some(auth_token) = &self.http_state.auth_token {
            request = request.header(AUTHORIZATION, auth_token)
        }
        let response = request.send().await;
        response
            .map_err(|e| {
                if e.is_timeout() {
                    ProtocolError("Request timeout")
                } else {
                    ProtocolError::coerce_static_from_str(&format!("Failed HTTP request: {}", e))
                }
            })?
            .json()
            .await
            .map_err(|e| {
                ProtocolError::coerce_static_from_str(&format!(
                    "Could not parse response as JSON: {}",
                    e
                ))
            })
    }

    /// Execute a NashProtocol request. Query will be created, executed over network, response will
    /// be passed to the protocol's state update hook, and response will be returned. Used by the even
    /// more generic `run_http(..)`.
    async fn execute_protocol_http<T: NashProtocol + Sync>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<T::Response>> {
        let query = request.graphql(self.state.clone()).await?;
        let json_payload = self.request_http(query).await?;
        let protocol_response = request
            .response_from_json(json_payload, self.state.clone())
            .await?;
        match protocol_response{
            ResponseOrError::Response(ref response) => {
                request
                    .process_response(&response.data, self.state.clone())
                    .await?;
            }
            ResponseOrError::Error(ref error_response) => {
                request
                    .process_error(error_response, self.state.clone())
                    .await?;
            }
        }
        Ok(protocol_response)
    }

    #[async_recursion]
    pub async fn run_http<T: NashProtocolPipeline + Clone>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        println!("running http request {:?}", request);
        async {
            let response = {
                if let Some(_permit) = request.acquire_permit(self.state.clone()).await {
                    self.run_helper_http(request).await
                } else {
                    self.run_helper_http(request).await
                }
            };
            if let Err(ref e) = response {
                error!(error = %e, "request error");
            }
            response
        }
        .instrument(tracing::info_span!(
                "RUN (http)",
                request = type_name::<T>(),
                id = %rand::thread_rng().gen::<u32>()))
        .await
    }

    pub async fn run_http_with_permit<T: NashProtocolPipeline + Clone>(
        &self,
        request: T,
        _permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        println!("running http with permit request {:?}", request);
        async {
            let response = self.run_helper_http(request).await;
            if let Err(ref e) = response {
                error!(error = %e, "request error");
            }
            response
        }
        .instrument(tracing::info_span!(
                "RUN (http)",
                request = type_name::<T>(),
                id = %rand::thread_rng().gen::<u32>()))
        .await
    }

    /// Does the main work of running a pipeline via http
    async fn run_helper_http<T: NashProtocolPipeline + Clone>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        // First run any dependencies of the request/pipeline
        let before_actions = request.run_before(self.state.clone()).await?;
        if let Some(actions) = before_actions {
            for action in actions {
                self.run_http(action).await?;
            }
        }
        // Now run the pipeline
        let mut protocol_state = request.init_state(self.state.clone()).await;
        // While pipeline contains more actions for client to take, execute them
        loop {
            if let Some(protocol_request) = request
                .next_step(&protocol_state, self.state.clone())
                .await?
            {
                let protocol_response = self.execute_protocol_http(protocol_request).await?;
                // If error, end pipeline early and return GraphQL/network error data
                if protocol_response.is_error() {
                    Self::manage_client_error(
                        self.state.clone(),
                        protocol_response.error().unwrap(),
                    )
                    .await;

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
        // Get things to run after the request/pipeline
        let after_actions = request.run_after(self.state.clone()).await?;
        // Now run anything specified for after the pipeline
        if let Some(actions) = after_actions {
            for action in actions {
                self.run_http(action).await?;
            }
        }
        // Return the pipeline output
        request.output(protocol_state)
    }
}

impl Client {
    /// Main entry point to execute Nash API requests via HTTP. Capable of running anything that implements `NashProtocolPipeline`.
    /// All `NashProtocol` requests automatically do. Other more complex multi-stage interactions like `SignAllStates`
    /// implement the trait manually. This will optionally run before and after hooks if those are defined for the pipeline
    /// or request (e.g., get asset nonces if they don't exist)
    #[inline]
    pub async fn run_http<T: NashProtocolPipeline + Clone>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        self.inner.run_http(request).await
    }
}
