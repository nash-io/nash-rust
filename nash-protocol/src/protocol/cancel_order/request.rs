use super::super::{general_canonical_string, RequestPayloadSignature};
use super::types::CancelOrderRequest;
use crate::graphql;
use crate::graphql::cancel_order;
use crate::utils::current_time_as_i64;

use super::super::signer::Signer;

use graphql_client::GraphQLQuery;

impl CancelOrderRequest {
    pub fn make_query(
        &self,
        signer: &Signer,
    ) -> graphql_client::QueryBody<cancel_order::Variables> {
        let mut cancel_args = cancel_order::Variables {
            payload: cancel_order::CancelOrderParams {
                market_name: self.market.clone(),
                order_id: self.order_id.clone(),
                timestamp: current_time_as_i64(),
            },
            signature: RequestPayloadSignature::empty().into(),
        };
        let sig_payload = cancel_all_canonical_string(&cancel_args);
        let sig = signer.sign_canonical_string(&sig_payload);
        cancel_args.signature = sig.into();
        graphql::CancelOrder::build_query(cancel_args)
    }
}

fn cancel_all_canonical_string(variables: &cancel_order::Variables) -> String {
    let serialized_all = serde_json::to_string(variables).unwrap();
    general_canonical_string(
        "cancel_order".to_string(),
        serde_json::from_str(&serialized_all).unwrap(),
        vec![],
    )
}

impl From<RequestPayloadSignature> for cancel_order::Signature {
    fn from(sig: RequestPayloadSignature) -> Self {
        cancel_order::Signature {
            signed_digest: sig.signed_digest,
            public_key: sig.public_key,
        }
    }
}
