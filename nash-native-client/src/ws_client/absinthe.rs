//! Websocket bindings to Absinthe server

use nash_protocol::errors::{ProtocolError, Result as ProtocolResult};
use serde::de;
use serde::de::{Deserializer, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::Debug;

/// This struct will serialize and deserialize into the kind of message that
/// Absinthe expects, e.g., ["1","1","__absinthe__:control","phx_join",{}].
/// We can derive deserialize automatically from the impls of the constituent
/// types. This is not the case for seralize as we want it to return an array
/// form and not an object structure.

// TODO: generalize shared fields between request/response

pub struct AbsintheWSRequest {
    // Not 100% what this is, but think in relation to id of joined client
    ref_join_id: Option<AbsintheInt>,
    // This is a unique identifier on request, used to map to response
    ref_id: Option<AbsintheInt>,
    // Phoenix channels exist over a topic
    topic: AbsintheTopic,
    // Events are things that can happen on a topic
    event: AbsintheEvent,
    // Payload contains optional serialized JSON data
    payload: Option<serde_json::Value>,
}

impl AbsintheWSRequest {
    pub fn new(
        join_id: u64,
        ref_id: u64,
        topic: AbsintheTopic,
        event: AbsintheEvent,
        payload: Option<serde_json::Value>,
    ) -> Self {
        Self {
            ref_join_id: Some(AbsintheInt::new(join_id)),
            ref_id: Some(AbsintheInt::new(ref_id)),
            topic,
            event,
            payload,
        }
    }
    // constructor for a message to initialize connection
    pub fn init_msg(join_id: u64, ref_id: u64) -> Self {
        Self {
            ref_join_id: Some(AbsintheInt::new(join_id)),
            ref_id: Some(AbsintheInt::new(ref_id)),
            topic: AbsintheTopic::Control,
            event: AbsintheEvent::Join,
            payload: None,
        }
    }
}

/// Response has a very similar structure to request. We split them
/// apart because ResponsePayload only needs to be Deserializable, and
/// this is important for some of the GraphQL type implementations

#[derive(Clone, Deserialize, Debug)]
pub struct AbsintheWSResponse {
    ref_join_id: Option<AbsintheInt>,
    ref_id: Option<AbsintheInt>,
    topic: AbsintheTopic,
    event: AbsintheEvent,
    pub payload: Option<ResponsePayload>,
}

impl AbsintheWSResponse {
    pub fn message_id(&self) -> Option<u64> {
        self.ref_id.as_ref().map(|x| x.value as u64)
    }
    pub fn subscription_id(&self) -> Option<String> {
        match &self.topic {
            AbsintheTopic::Doc(id) => Some(id.clone()),
            _ => None,
        }
    }
    pub fn subscription_setup_id(&self) -> Option<String> {
        match &self.payload {
            Some(ResponsePayload::SubSetup(data)) => Some(data.response.subscription_id.clone()),
            _ => None,
        }
    }
    pub fn json_payload(self) -> ProtocolResult<serde_json::Value> {
        if let Some(payload) = self.payload {
            Ok(payload.graphql()?.response)
        } else {
            Err(ProtocolError("No response to unpack"))
        }
    }
    pub fn subscription_json_payload(self) -> ProtocolResult<serde_json::Value> {
        if let Some(payload) = self.payload {
            Ok(payload.subscription()?.result)
        } else {
            Err(ProtocolError("No response to unpack"))
        }
    }
}

// Helper types. E.g., Absinthe wants integer IDs as strings...

#[derive(Debug, Clone)]
pub enum AbsintheTopic {
    Control,
    Doc(String),
    Phoenix,
}

#[derive(Debug, Clone)]
pub enum AbsintheEvent {
    Join,
    Reply,
    Leave,
    Doc, // why did they choose this name? it is for queries/subscriptions/mutations
    SubscriptionData,
    Heartbeat, // to maintain connection
    Error,
}

#[derive(Debug, Clone)]
pub struct AbsintheInt {
    value: u64,
}
impl AbsintheInt {
    pub fn new(value: u64) -> Self {
        Self { value }
    }
}

// We can similarly create an enum for the responses. This is important to
// have a single type for responses we will push back up over the channel.

#[derive(Clone, Deserialize, Debug)]
#[serde(untagged)]
pub enum ResponsePayload {
    SubSetup(SubscriptionSetupResponse), // for subscription setup
    GraphQL(QueryResponse),
    Subscription(SubscriptionResponse),
}

impl ResponsePayload {
    pub fn sub_setup(self) -> ProtocolResult<SubscriptionSetupResponse> {
        match self {
            Self::SubSetup(data) => Ok(data),
            _ => Err(ProtocolError("Not a subscription setup response")),
        }
    }
    pub fn graphql(self) -> ProtocolResult<QueryResponse> {
        match self {
            Self::GraphQL(data) => Ok(data),
            _ => Err(ProtocolError("Not a graphql query response")),
        }
    }
    pub fn subscription(self) -> ProtocolResult<SubscriptionResponse> {
        match self {
            Self::Subscription(data) => Ok(data),
            _ => Err(ProtocolError("Not a subscription response")),
        }
    }
}

// here is format for query response
#[derive(Clone, Deserialize, Debug)]
pub struct QueryResponse {
    pub response: serde_json::Value,
    pub status: String,
}

// here is format for subscription response
#[derive(Clone, Deserialize, Debug)]
pub struct SubscriptionResponse {
    result: serde_json::Value,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionSetup {
    subscription_id: String,
}
#[derive(Clone, Deserialize, Debug)]
pub struct SubscriptionSetupResponse {
    response: SubscriptionSetup,
    status: String,
}

// Serialization methods which cannot be derived given how dumb the protocol is
// These are manual implementations of traits that would normally be derived
// automatically via serde.

impl Serialize for AbsintheWSRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(5))?;
        seq.serialize_element(&self.ref_join_id)?;
        seq.serialize_element(&self.ref_id)?;
        seq.serialize_element(&self.topic)?;
        seq.serialize_element(&self.event)?;
        seq.serialize_element(&self.payload)?;
        seq.end()
    }
}

impl Serialize for AbsintheTopic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Control => serializer.serialize_str(&"__absinthe__:control".to_string()),
            Self::Doc(id) => serializer.serialize_str(&format!("__absinthe__:doc:{}", id)),
            Self::Phoenix => serializer.serialize_str(&"phoenix".to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for AbsintheTopic {
    fn deserialize<D>(deserializer: D) -> Result<AbsintheTopic, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TopicVisitor;

        impl<'de> Visitor<'de> for TopicVisitor {
            type Value = AbsintheTopic;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("'__absinthe__:control' or '__absinthe__:doc:*'")
            }

            fn visit_str<E>(self, value: &str) -> Result<AbsintheTopic, E>
            where
                E: de::Error,
            {
                match value {
                    "__absinthe__:control" => Ok(AbsintheTopic::Control),
                    "phoenix" => Ok(AbsintheTopic::Phoenix),
                    _ => {
                        let pattern = "__absinthe__:doc:".to_string();
                        let pattern_match = value.rfind(&pattern);
                        match pattern_match {
                            Some(_idx) => {
                                // let id = value.split_at(idx);
                                // Ok(AbsintheTopic::Doc((id.1).to_string()))
                                Ok(AbsintheTopic::Doc(value.to_string()))
                            }
                            None => Err(de::Error::custom("Bad AbsintheTopic field value")),
                        }
                    }
                }
            }
        }
        deserializer.deserialize_identifier(TopicVisitor)
    }
}

impl Serialize for AbsintheEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Join => serializer.serialize_str(&"phx_join".to_string()),
            Self::Reply => serializer.serialize_str(&"phx_reply".to_string()),
            Self::Leave => serializer.serialize_str(&"phx_leave".to_string()),
            Self::Doc => serializer.serialize_str(&"doc".to_string()),
            Self::SubscriptionData => serializer.serialize_str(&"subscription:data".to_string()),
            Self::Heartbeat => serializer.serialize_str(&"heartbeat".to_string()),
            Self::Error => serializer.serialize_str(&"phx_error".to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for AbsintheEvent {
    fn deserialize<D>(deserializer: D) -> Result<AbsintheEvent, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EventVisitor;

        impl<'de> Visitor<'de> for EventVisitor {
            type Value = AbsintheEvent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("'phx_join' or 'phx_leave'")
            }

            fn visit_str<E>(self, value: &str) -> Result<AbsintheEvent, E>
            where
                E: de::Error,
            {
                match value {
                    "phx_join" => Ok(AbsintheEvent::Join),
                    "phx_leave" => Ok(AbsintheEvent::Leave),
                    "phx_reply" => Ok(AbsintheEvent::Reply),
                    "doc" => Ok(AbsintheEvent::Doc),
                    "heartbeat" => Ok(AbsintheEvent::Heartbeat),
                    "subscription:data" => Ok(AbsintheEvent::SubscriptionData),
                    "phx_error" => Ok(AbsintheEvent::Error),
                    _ => Err(de::Error::custom("Bad AbsintheEvent field value")),
                }
            }
        }
        deserializer.deserialize_identifier(EventVisitor)
    }
}

impl Serialize for AbsintheInt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}", self.value))
    }
}

impl<'de> Deserialize<'de> for AbsintheInt {
    fn deserialize<D>(deserializer: D) -> Result<AbsintheInt, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IntVisitor;

        impl<'de> Visitor<'de> for IntVisitor {
            type Value = AbsintheInt;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("string parsable to u64")
            }

            fn visit_str<E>(self, value: &str) -> Result<AbsintheInt, E>
            where
                E: de::Error,
            {
                let try_parse = value.parse::<u64>();

                match try_parse {
                    Ok(value) => Ok(AbsintheInt { value }),
                    _ => Err(de::Error::custom("Bad AbsintheInt field value")),
                }
            }
        }
        deserializer.deserialize_identifier(IntVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::AbsintheWSResponse;
    use nash_protocol::graphql;
    use nash_protocol::protocol::asset_nonces::AssetNoncesResponse;
    use nash_protocol::protocol::ResponseOrError;
    use nash_protocol::protocol::{
        dh_fill_pool::DhFillPoolResponse, try_response_from_json,
    };

    #[test]
    fn test_empty() {
        let res: AbsintheWSResponse = serde_json::from_str("[\"1\",\"1\",\"__absinthe__:control\",\"phx_reply\",{\"response\":{},\"status\":\"ok\"}]").unwrap();
        println!("{:?}", res);
    }

    #[test]
    fn test_dh_fill() {
        let res: AbsintheWSResponse = serde_json::from_str(
            "[\"1\",\"2\",\"__absinthe__:control\",\"phx_reply\",{\"response\":{\"data\":{\"dhFillPool\":[\"03ec657cc5901fc2214861f50b6557471153085e98977363834e5a4a6978f79273\"]}},\"status\":\"ok\"}]"
        ).unwrap();
        println!("{:?}", res);
        // assert_ne!(res.payload.unwrap().graphql().unwrap().response.data.dh_fill().unwrap().dh_fill_pool, None);
        let _dh_fill: ResponseOrError<DhFillPoolResponse> = try_response_from_json::<
            DhFillPoolResponse,
            graphql::dh_fill_pool::ResponseData,
        >(res.json_payload().unwrap())
        .unwrap();
    }

    #[test]
    fn test_get_assets() {
        let res: AbsintheWSResponse = serde_json::from_str(
            "[\"1\",\"3\",\"__absinthe__:control\",\"phx_reply\",{\"response\":{\"data\":{\"getAssetsNonces\":[{\"asset\":\"usdc\",\"nonces\":[33]},{\"asset\":\"eth\",\"nonces\":[33]}]}},\"status\":\"ok\"}]"
        ).unwrap();
        println!("{:?}", res);
        let assets_nonces: ResponseOrError<AssetNoncesResponse> = try_response_from_json::<
            AssetNoncesResponse,
            graphql::get_assets_nonces::ResponseData,
        >(
            res.json_payload().unwrap()
        )
        .unwrap();
        println!("{:?}", assets_nonces);
    }
}
