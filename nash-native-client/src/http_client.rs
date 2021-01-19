use nash_protocol::protocol::{
    NashProtocol, NashProtocolPipeline, ResponseOrError, State,
};
use nash_protocol::errors::{ProtocolError, Result};

use super::ws_client::client::{InnerClient, manage_client_error};
use reqwest::header::AUTHORIZATION;


impl InnerClient {

    async fn request_http(
        &self,
        request: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // Do simple request/response...
        let http_client = reqwest::Client::new();
        let url = format!("https://{}/api/graphql", self.env.url());
        println!("{:?}", url);
        let mut request = http_client.post(&url)
            .json(&request);
        if let Some(session) = self.session.as_ref() {
            let token = format!("Token {}", session);
            request = request.header(AUTHORIZATION, token)
        }
        let response = request
            .send()
            .await;

        println!("{:?}", response);

        response
            .map_err(|_| ProtocolError("Failed request"))?
            .json()
            .await
            .map_err(|_| ProtocolError("Could not parse response as JSON"))
    }

    /// Execute a NashProtocol request. Query will be created, executed over network, response will
    /// be passed to the protocol's state update hook, and response will be returned. Used by the even
    /// more generic `run(..)`.
    async fn execute_protocol_http<T: NashProtocol + Sync>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<T::Response>> {
        let query = request.graphql(self.state.clone()).await?;
        let json_payload = self.request_http(query).await?;
        let protocol_response = request.response_from_json(json_payload, self.state.clone()).await?;
        if let Some(response) = protocol_response.response() {
            request
                .process_response(response, self.state.clone())
                .await?;
        }
        Ok(protocol_response)
    }

    /// Does the main work of running a pipeline
    async fn run_helper_http<T: NashProtocolPipeline>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        let mut protocol_state = request.init_state(self.state.clone()).await;
        // While pipeline containst more actions for client to take, execute them
        loop {
            if let Some(protocol_request) = request
                .next_step(&protocol_state, self.state.clone())
                .await?
            {
                let protocol_response = self.execute_protocol_http(protocol_request).await?;
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

    pub async fn run_http<T: NashProtocolPipeline + Clone + Sync>(
        &self,
        request: T,
    ) -> Result<ResponseOrError<<T::ActionType as NashProtocol>::Response>> {
        // First run any dependencies of the request/pipeline
        let before_actions = request.run_before(self.state.clone()).await?;

        if let Some(actions) = before_actions {
            for action in actions {
                self.run_helper_http(action).await?;
            }
        }

        // Now run the pipeline
        let out = self.run_helper_http(request.clone()).await;

        // Get things to run after the request/pipeline
        let after_actions = request.run_after(self.state.clone()).await?;

        // Now run anything specified for after the pipeline
        if let Some(actions) = after_actions {
            for action in actions {
                self.run_helper_http(action).await?;
            }
        }

        out
    }

}

#[cfg(test)]
mod test {
    use super::super::ws_client::client::{InnerClient, Environment};
    use nash_protocol::protocol::cancel_all_orders::CancelAllOrders;
    use nash_protocol::protocol::list_markets::ListMarketsRequest;
    use nash_protocol::protocol::sign_all_states::SignAllStates;
    use dotenv::dotenv;
    use tokio::time::Duration;
    use std::sync::Arc;

    async fn init_client() -> InnerClient {
        dotenv().ok();
        let secret  = std::env::var("NASH_API_SECRET").expect("Couldn't get environment variable.");
        let session = std::env::var("NASH_API_KEY").expect("Couldn't get environment variable.");
        let (client, _) = InnerClient::from_key_data(
            &secret,
            &session,
            None,
            0,
            Environment::Production,
            Duration::from_secs_f32(2.0),
        )
        .await
        .unwrap();
        client
    }

    #[tokio::test(core_threads = 2)]
    async fn cancel_all_http(){
        let client = init_client().await;
        let response = client.run_http(CancelAllOrders{market: "eth_usdc".to_string()}).await;
        // let response = client.run_http(ListMarketsRequest).await;
        println!("{:?}", response);
    }

    #[tokio::test(core_threads = 2)]
    async fn multiple_concurrent_requests(){
        let client = init_client().await;
        let share_client = Arc::new(client);
        async fn make_cancel_order(client: Arc<InnerClient>, i: u64) {
            println!("started loop {}", i);
            let req = client.run(CancelAllOrders{market: "eth_usdc".to_string()}).await;
            if !req.is_err() {
                println!("finished cancel_order {}", i); 
            } else {
                println!("error cancel_order {}", i);
            }
        }
        let mut handles = Vec::new();
        let mut count = 0;
        for _ in 0..10 {
            handles.push(tokio::spawn(make_cancel_order(share_client.clone(), count)));
            count += 1;
        }
        futures::future::join_all(handles).await;
    }

}
