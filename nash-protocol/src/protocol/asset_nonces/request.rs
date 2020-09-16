use super::super::{general_canonical_string, RequestPayloadSignature};
use super::types::AssetNoncesRequest;
use crate::errors::{ProtocolError, Result};
use crate::graphql;
use crate::types::Asset;
use crate::utils::current_time_as_i64;

use super::super::signer::Signer;

use graphql::get_assets_nonces;
use graphql_client::GraphQLQuery;

impl AssetNoncesRequest {
    pub fn make_query(
        &self,
        signer: &Signer,
        assets: Option<Vec<Asset>>,
    ) -> Result<graphql_client::QueryBody<get_assets_nonces::Variables>> {
        let asset_list = assets
            .ok_or(ProtocolError(
                "Client must be initialized with list of assets first",
            ))?
            .iter()
            .map(|asset| {
                // TODO: schema bug, ask backend to fix
                Some(asset.name().to_string())
            })
            .collect();
        let mut asset_nonce_args = get_assets_nonces::Variables {
            payload: get_assets_nonces::GetAssetsNoncesParams {
                timestamp: current_time_as_i64(),
                assets: Some(asset_list),
            },
            signature: RequestPayloadSignature::empty().into(),
        };
        let sig_payload = asset_nonces_canonical_string(&asset_nonce_args);
        let sig = signer.sign_canonical_string(&sig_payload);
        asset_nonce_args.signature = sig.into();
        Ok(graphql::GetAssetsNonces::build_query(asset_nonce_args))
    }
}

/// Generate canonical string for on request to get asset nonces
pub fn asset_nonces_canonical_string(variables: &get_assets_nonces::Variables) -> String {
    let serialized_all = serde_json::to_string(variables).unwrap();
    general_canonical_string(
        "get_assets_nonces".to_string(),
        serde_json::from_str(&serialized_all).unwrap(),
        vec![],
    )
}

/// Convert ugly generated `get_assets_nonces::Signature` type into common signature
impl From<RequestPayloadSignature> for get_assets_nonces::Signature {
    fn from(sig: RequestPayloadSignature) -> Self {
        get_assets_nonces::Signature {
            signed_digest: sig.signed_digest,
            public_key: sig.public_key,
        }
    }
}
