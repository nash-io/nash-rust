use super::types::CancelOrdersRequest;
use super::super::signer::Signer;
use crate::protocol::multi_request::DynamicQueryBody;
use std::collections::HashMap;

impl CancelOrdersRequest {
    pub fn make_query(
        &self,
        signer: &Signer,
    ) -> DynamicQueryBody {
        let mut variables = HashMap::new();
        let mut params = String::new();
        let mut calls = String::new();
        for (index, variable) in self.requests.iter().enumerate() {
            let variable = variable.make_variables(signer);

            // FIXME: This is also replicated in MarketOrdersConstructor::signed_graphql_request
            let payload = format!("payload{}", index);
            let signature = format!("signature{}", index);
            params = if index == 0 { params } else { format!("{}, ", params)};
            params = format!("{}${}: CancelOrderParams!, ${}: Signature!", params, payload, signature);
            calls = format!(r#"
                {}
                response{}: cancelOrder(payload: ${}, signature: ${}) {{
                    orderIr
                }}
                "#, calls, index, payload, signature);
            variables.insert(payload, serde_json::to_value(variable.payload).unwrap());
            variables.insert(signature, serde_json::to_value(variable.signature).unwrap());
        }
        DynamicQueryBody {
            variables,
            operation_name: "CancelOrder",
            query: format!(r#"
                mutation CancelOrder({}) {{
                    {}
                }}
            "#, params, calls)
        }
    }
}
