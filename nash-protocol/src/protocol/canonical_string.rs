//! Transform GraphQL JSON request into a consistent canonical string for signing

use inflector::cases::snakecase::to_snake_case;
use serde_json::{map::Map, Value};
use std::collections::HashMap;

/// Given an operation name, unstructured JSON object under the payload key (common to all
/// mutations that require signatures) and list of fields to remove, return a canonical
/// string representation for signing the mutation
pub fn general_canonical_string(
    operation_name: String,
    mut json_value: Value,
    without_fields: Vec<String>,
) -> String {
    let json_map = json_value.as_object_mut().unwrap();
    let payload_map = json_map["payload"].as_object_mut().unwrap();
    recursively_snake_case(payload_map, without_fields);
    let payload_string = serde_json::to_string(&payload_map).unwrap();
    let out = format!("{},{}", operation_name, payload_string);
    out
}

fn recursively_snake_case(map: &mut Map<String, Value>, without: Vec<String>) {
    let mut remove_key_map = HashMap::new();
    for key in without {
        remove_key_map.insert(key, true);
    }
    let key_list: Vec<String> = map.keys().cloned().collect();
    for key in key_list {
        let mut value = map.remove(&key).unwrap();
        // replace key with snake case as long as it is not in list to remove
        if remove_key_map.get(&to_snake_case(&key)) == None {
            // is value is object, snake case those keys also
            if value.is_object() {
                let mut_next_map = value.as_object_mut().unwrap();
                recursively_snake_case(mut_next_map, vec![]);
            } else if value.is_string() {
                // convert string values to lowercase
                value = value.as_str().unwrap().to_ascii_lowercase().into();
            } else if value.is_null() {
                // don't serialize null fields
                continue;
            }
            map.insert(to_snake_case(&key), value);
        }
    }
}
