use super::super::{general_canonical_string, RequestPayloadSignature};
use super::types::CancelAllOrders;
use crate::graphql;
use crate::graphql::cancel_all_orders;
use crate::utils::current_time_as_i64;

use super::super::signer::Signer;

use graphql_client::GraphQLQuery;

impl CancelAllOrders {
    pub fn make_query(
        &self,
        signer: &Signer,
    ) -> graphql_client::QueryBody<cancel_all_orders::Variables> {
        let mut cancel_args = cancel_all_orders::Variables {
            payload: cancel_all_orders::CancelAllOrdersParams {
                market_name: Some(self.market.clone()),
                timestamp: current_time_as_i64(),
            },
            signature: RequestPayloadSignature::empty().into(),
        };
        let sig_payload = cancel_all_canonical_string(&cancel_args);
        let sig = signer.sign_canonical_string(&sig_payload);
        cancel_args.signature = sig.into();
        graphql::CancelAllOrders::build_query(cancel_args)
    }
}

/// Generate payload string for signing a request to cancel all orders
fn cancel_all_canonical_string(variables: &cancel_all_orders::Variables) -> String {
    let serialized_all = serde_json::to_string(variables).unwrap();
    general_canonical_string(
        "cancel_all_orders".to_string(),
        serde_json::from_str(&serialized_all).unwrap(),
        vec![],
    )
}

/// Convert ugly generated `cancel_all_orders::Signature` type into common signature
impl From<RequestPayloadSignature> for cancel_all_orders::Signature {
    fn from(sig: RequestPayloadSignature) -> Self {
        cancel_all_orders::Signature {
            signed_digest: sig.signed_digest,
            public_key: sig.public_key,
        }
    }
}
