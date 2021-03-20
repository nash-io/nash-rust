//! These traits describe the high level behavior of the Nash protocol. Clients can use them
//! to provide a generic implementation across requests
use super::subscriptions::SubscriptionResponse;
use super::{ProtocolHook, ResponseOrError, State};
use crate::errors::ProtocolError;
use crate::errors::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;
use std::fmt::Debug;
use std::sync::Arc;
use crate::protocol::ErrorResponse;

//****************************************//
//  Nash protocol trait                   //
//****************************************//

/// Trait that all Nash protocol elements implement. Enforces transformation to GraphQL as well
/// as state changes on response processing.
#[async_trait]
pub trait NashProtocol: Debug + Send + Sync {
    type Response: Send + Sync;
    /// If you want to limit the amount of concurrency of a protocol return a Semaphore here
    async fn acquire_permit(&self, _state: Arc<RwLock<State>>) -> Option<tokio::sync::OwnedSemaphorePermit> {
        None
    }
    /// Convert the protocol request to GraphQL from communication with Nash server
    // Note: state is declared as mutable
    async fn graphql(&self, state: Arc<RwLock<State>>) -> Result<serde_json::Value>;
    /// Convert JSON response to request to the protocol's associated type
    async fn response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<Self::Response>>;
    /// Any state changes that result from execution of the protocol request
    /// The default implementation does nothing to state
    async fn process_response(
        &self,
        _response: &Self::Response,
        _state: Arc<RwLock<State>>,
    ) -> Result<()> {
        Ok(())
    }
    /// Any errors that result from execution of the protocol request
    /// The default implementation does nothing to state
    async fn process_error(
        &self,
        _response: &ErrorResponse,
        _state: Arc<RwLock<State>>,
    ) -> Result<()> {
        Ok(())
    }
    // Any dependencies for the pipeline (e.g., if it needs r values or asset nonces)
    async fn run_before(&self, _state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        Ok(None)
    }
    // Any requests to run after the pipeline (e.g., update asset nonces after state signing)
    async fn run_after(&self, _state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        Ok(None)
    }
}

//****************************************//
//  Nash protocol pipeline trait          //
//****************************************//

/// Trait to abstract over a series of linked protocol requests. For example we use this
/// to abstract over repeated calls to sign_state until there are no more states to sign.
#[async_trait]
pub trait NashProtocolPipeline: Debug + Send + Sync {
    /// State managed by pipeline and used to hold intermediate results and allow
    /// the implementer to decide whether the pipeline is over
    type PipelineState: Send + Sync;
    /// Wrapper type for all actions this pipeline can take
    type ActionType: NashProtocol;
    /// If you want to limit the amount of concurrency of a pipeline return a Semaphore here
    async fn acquire_permit(&self, _state: Arc<RwLock<State>>) -> Option<tokio::sync::OwnedSemaphorePermit> {
        None
    }
    /// Create initial state for the pipeline
    async fn init_state(&self, state: Arc<RwLock<State>>) -> Self::PipelineState;
    /// Give next action to take or return `None` if pipeline is finished. `&State` needs
    /// to be mutable as client may modify itself when producing the next step (e.g., removing
    /// and r value generate a signature)
    async fn next_step(
        &self,
        pipeline_state: &Self::PipelineState,
        client_state: Arc<RwLock<State>>,
    ) -> Result<Option<Self::ActionType>>;
    /// Process the results of a pipeline step
    async fn process_step(
        &self,
        result: <<Self as NashProtocolPipeline>::ActionType as NashProtocol>::Response,
        pipeline_state: &mut Self::PipelineState,
    );
    /// Get results from pipeline or `None` if the pipeline is not finished
    fn output(
        &self,
        pipeline_state: Self::PipelineState,
    ) -> Result<ResponseOrError<<Self::ActionType as NashProtocol>::Response>>;
    // Any dependencies for the pipeline (e.g., if it needs r values or asset nonces)
    async fn run_before(&self, _state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        Ok(None)
    }
    // Any requests to run after the pipeline (e.g., update asset nonces after state signing)
    async fn run_after(&self, _state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        Ok(None)
    }
}

/// A pipeline is a superset of `NashProtocol`, so something of type NashProtocol
/// can by itself be considered a valid pipeline. This is convenient if we just want to
/// have a single client interface that can take pipelines or protocol requests. We can
/// do this once generically and it will apply to all implementations
#[async_trait]
impl<T> NashProtocolPipeline for T
where
    T: NashProtocol + Clone + Sync + Send,
{
    type PipelineState = Option<ResponseOrError<T::Response>>;
    type ActionType = T;
    // Here we just delegate this to underlying protocol request
    async fn acquire_permit(&self, state: Arc<RwLock<State>>) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.acquire_permit(state).await
    }
    // This begins as `None` but will be set to a wrapped T::Response
    async fn init_state(&self, _state: Arc<RwLock<State>>) -> Self::PipelineState {
        None
    }
    // This will only return a next step once assuming state is set by client in `process_step`
    async fn next_step(
        &self,
        pipeline_state: &Self::PipelineState,
        _client_state: Arc<RwLock<State>>,
    ) -> Result<Option<Self::ActionType>> {
        // If we have a response already, things are done
        if let Some(_) = pipeline_state {
            Ok(None)
        }
        // Else this request itself is the first (and only) item in the pipeline
        else {
            Ok(Some(self.clone()))
        }
    }
    // Just grab the request and set state with some wrapping
    async fn process_step(
        &self,
        result: <<Self as NashProtocolPipeline>::ActionType as NashProtocol>::Response,
        state: &mut Self::PipelineState,
    ) {
        *state = Some(ResponseOrError::from_data(result));
    }
    // Just return state
    fn output(&self, state: Self::PipelineState) -> Result<ResponseOrError<T::Response>> {
        if let Some(state) = state {
            Ok(state)
        } else {
            Err(ProtocolError(
                "Protocol request not run, cannot retrieve output",
            ))
        }
    }
    // Any other requests or pipelines to run before this one.
    // Here we just delegate this to underlying protocol request
    async fn run_before(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        self.run_before(state).await
    }
    // Any other requests or pipelines to run after this one.
    // Here we just delegate this to underlying protocol request
    async fn run_after(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        self.run_after(state).await
    }
}

/// Trait that defines subscriptions over the Nash protocol. The main difference is that
/// the transformation of response data to `SubscriptionResponse` must occur on every
/// incoming subscription event. Similarly, the subscription is able to modify client state
/// on every incoming event. This last property is important for a subscription that updates
/// asset nonces once that is available on the backend.
#[async_trait]
pub trait NashProtocolSubscription: Clone {
    type SubscriptionResponse: Send + Sync;
    /// Convert the protocol request to GraphQL from communication with Nash server
    async fn graphql(&self, state: Arc<RwLock<State>>) -> Result<serde_json::Value>;
    /// Convert JSON response from incoming subscription data into protocol's associated type
    async fn subscription_response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<Self::SubscriptionResponse>>;
    /// Update state based on data from incoming subscription response
    async fn process_subscription_response(
        &self,
        _response: &Self::SubscriptionResponse,
        _state: Arc<RwLock<State>>,
    ) -> Result<()> {
        Ok(())
    }
    async fn wrap_response_as_any_subscription(
        &self,
        response: serde_json::Value,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<SubscriptionResponse>>;
}

/// Similar to TryFrom, but threads additional State in as context
/// that is necessary to perform the conversion
#[async_trait]
pub trait TryFromState<T>: Sized {
    async fn from(source: T, state: Arc<RwLock<State>>) -> Result<Self>;
}
