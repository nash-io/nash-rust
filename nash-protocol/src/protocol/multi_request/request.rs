use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// The form in which queries are sent over HTTP in most implementations. This will be built using the [`GraphQLQuery`] trait normally.
#[derive(Debug, Serialize, Deserialize)]
pub struct DynamicQueryBody {
    /// The values for the variables. They must match those declared in the queries. This should be the `Variables` struct from the generated module corresponding to the query.
    pub variables: HashMap<String, serde_json::Value>,
    /// The GraphQL query, as a string.
    pub query: String,
    /// The GraphQL operation name, as a string.
    #[serde(rename = "operationName")]
    pub operation_name: &'static str,
}
