use super::types::AssetNoncesResponse;
use crate::graphql::get_assets_nonces;
use std::collections::HashMap;

impl From<get_assets_nonces::ResponseData> for AssetNoncesResponse {
    fn from(response: get_assets_nonces::ResponseData) -> Self {
        let mut nonces = HashMap::new();
        for asset_nonce in response.get_assets_nonces {
            let unwrapped_nonces = asset_nonce.nonces.iter().map(|x| *x as u32).collect();
            nonces.insert(asset_nonce.asset, unwrapped_nonces);
        }
        Self { nonces }
    }
}
