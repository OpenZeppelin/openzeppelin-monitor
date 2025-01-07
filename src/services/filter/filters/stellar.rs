use async_trait::async_trait;
use base64::Engine;
use log::{info, warn};
use serde_json::Value;
use stellar_xdr::curr::{OperationBody, TransactionEnvelope};

use crate::{
    models::{
        BlockType, EventCondition, FunctionCondition, MatchConditions, Monitor, MonitorMatch,
        Network, StellarEvent, StellarMatchArguments, StellarMatchParamEntry,
        StellarMatchParamsMap, StellarMonitorMatch, StellarTransaction, TransactionCondition,
        TransactionStatus,
    },
    services::{
        blockchain::BlockChainClientEnum,
        filter::helpers::stellar::{
            are_same_signature, normalize_address, parse_xdr_value, process_invoke_host_function,
        },
    },
};

use super::{BlockFilter, FilterError};

#[derive(Debug)]
struct EventMap {
    pub event: StellarMatchParamsMap,
    pub tx_hash: String,
}

pub struct StellarBlockFilter {}

impl StellarBlockFilter {
    fn find_matching_transaction(
        &self,
        transaction: &StellarTransaction,
        monitor: &Monitor,
        matched_transactions: &mut Vec<TransactionCondition>,
    ) {
        let tx_status: TransactionStatus = match transaction.status.as_str() {
            "SUCCESS" => TransactionStatus::Success,
            "FAILED" => TransactionStatus::Failure,
            "NOT FOUND" => TransactionStatus::Failure,
            _ => TransactionStatus::Any,
        };

        struct TxOperation {
            _operation_type: String,
            sender: String,
            receiver: String,
            value: Option<String>,
        }

        let mut tx_operations: Vec<TxOperation> = vec![];

        if let Some(decoded) = transaction.decoded() {
            if let Some(envelope) = &decoded.envelope {
                match envelope {
                    TransactionEnvelope::Tx(tx) => {
                        let from = tx.tx.source_account.to_string();
                        for operation in tx.tx.operations.iter() {
                            match &operation.body {
                                OperationBody::Payment(payment) => {
                                    let operation = TxOperation {
                                        _operation_type: "payment".to_string(),
                                        sender: from.clone(),
                                        receiver: payment.destination.to_string(),
                                        value: Some(payment.amount.to_string()),
                                    };
                                    tx_operations.push(operation);
                                }
                                OperationBody::InvokeHostFunction(invoke_host_function) => {
                                    let parsed_operation =
                                        process_invoke_host_function(invoke_host_function);
                                    let operation = TxOperation {
                                        _operation_type: "invoke_host_function".to_string(),
                                        sender: from.clone(),
                                        receiver: parsed_operation.contract_address.clone(),
                                        value: None,
                                    };
                                    tx_operations.push(operation);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check transaction match conditions
        if monitor.match_conditions.transactions.is_empty() {
            // Match all transactions
            matched_transactions.push(TransactionCondition {
                expression: None,
                status: TransactionStatus::Any,
            });
        } else {
            // Check each transaction condition
            for condition in &monitor.match_conditions.transactions {
                // First check if status matches (if specified)
                let status_matches = match &condition.status {
                    TransactionStatus::Any => true,
                    required_status => *required_status == tx_status.clone(),
                };

                if status_matches {
                    if let Some(expr) = &condition.expression {
                        for operation in &tx_operations {
                            // Create a vector of transaction parameters
                            let tx_params = vec![
                                StellarMatchParamEntry {
                                    name: "value".to_string(),
                                    value: operation.value.clone().unwrap_or("0".to_string()),
                                    kind: "i64".to_string(),
                                    indexed: false,
                                },
                                StellarMatchParamEntry {
                                    name: "from".to_string(),
                                    value: operation.sender.clone(),
                                    kind: "address".to_string(),
                                    indexed: false,
                                },
                                StellarMatchParamEntry {
                                    name: "to".to_string(),
                                    value: operation.receiver.clone(),
                                    kind: "address".to_string(),
                                    indexed: false,
                                },
                            ];

                            if self.evaluate_expression(expr, &Some(tx_params)) {
                                matched_transactions.push(TransactionCondition {
                                    expression: Some(expr.clone()),
                                    status: tx_status,
                                });
                                break;
                            }
                        }
                    } else {
                        // No expression but status matched
                        matched_transactions.push(TransactionCondition {
                            expression: None,
                            status: tx_status,
                        });
                        break;
                    }
                }
            }
        }
    }

    fn find_matching_functions_for_transaction(
        &self,
        monitored_addresses: &[String],
        transaction: &StellarTransaction,
        monitor: &Monitor,
        matched_functions: &mut Vec<FunctionCondition>,
        matched_on_args: &mut StellarMatchArguments,
    ) {
        if let Some(decoded) = transaction.decoded() {
            if let Some(envelope) = &decoded.envelope {
                match envelope {
                    TransactionEnvelope::Tx(tx) => {
                        for operation in tx.tx.operations.iter() {
                            match &operation.body {
                                OperationBody::InvokeHostFunction(invoke_host_function) => {
                                    let parsed_operation =
                                        process_invoke_host_function(invoke_host_function);

                                    // Skip if contract address doesn't match
                                    if !monitored_addresses.contains(&normalize_address(
                                        &parsed_operation.contract_address,
                                    )) {
                                        continue;
                                    }

                                    // Convert parsed operation arguments into param entries
                                    let param_entries = self
                                        .convert_arguments_to_match_param_entry(
                                            &parsed_operation.arguments,
                                        );

                                    if monitor.match_conditions.functions.is_empty() {
                                        // Match on all functions
                                        matched_functions.push(FunctionCondition {
                                            signature: parsed_operation.function_signature.clone(),
                                            expression: None,
                                        });
                                    } else {
                                        // Check function conditions
                                        for condition in &monitor.match_conditions.functions {
                                            // Check if function signature matches
                                            if are_same_signature(
                                                &condition.signature,
                                                &parsed_operation.function_signature,
                                            ) {
                                                // Evaluate expression if it exists
                                                if let Some(expr) = &condition.expression {
                                                    if self.evaluate_expression(
                                                        expr,
                                                        &Some(param_entries.clone()),
                                                    ) {
                                                        matched_functions.push(FunctionCondition {
                                                            signature: parsed_operation
                                                                .function_signature
                                                                .clone(),
                                                            expression: Some(expr.clone()),
                                                        });
                                                        if let Some(functions) =
                                                            &mut matched_on_args.functions
                                                        {
                                                            functions.push(StellarMatchParamsMap {
                                                                signature: parsed_operation
                                                                    .function_signature
                                                                    .clone(),
                                                                args: Some(param_entries.clone()),
                                                            });
                                                        }
                                                        break;
                                                    }
                                                } else {
                                                    // If no expression, match on function name alone
                                                    matched_functions.push(FunctionCondition {
                                                        signature: parsed_operation
                                                            .function_signature
                                                            .clone(),
                                                        expression: None,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn find_matching_events_for_transaction(
        &self,
        events: &Vec<EventMap>,
        transaction: &StellarTransaction,
        monitor: &Monitor,
        matched_events: &mut Vec<EventCondition>,
        matched_on_args: &mut StellarMatchArguments,
    ) {
        let events_for_transaction = events
            .iter()
            .filter(|event| event.tx_hash == *transaction.hash())
            .map(|event| event.event.clone())
            .collect::<Vec<_>>();

        // Check event conditions
        for event in &events_for_transaction {
            if monitor.match_conditions.events.is_empty() {
                // Match all events
                matched_events.push(EventCondition {
                    signature: event.signature.clone(),
                    expression: None,
                });
                if let Some(events) = &mut matched_on_args.events {
                    events.push(event.clone());
                }
            } else {
                // Find all matching conditions for this event
                let matching_conditions =
                    monitor.match_conditions.events.iter().filter(|condition| {
                        are_same_signature(&condition.signature, &event.signature)
                    });

                for condition in matching_conditions {
                    match &condition.expression {
                        Some(expr) => {
                            if self.evaluate_expression(expr, &Some(event.args.clone().unwrap())) {
                                matched_events.push(EventCondition {
                                    signature: event.signature.clone(),
                                    expression: Some(expr.clone()),
                                });
                                if let Some(events) = &mut matched_on_args.events {
                                    events.push(event.clone());
                                }
                            }
                        }
                        None => {
                            matched_events.push(EventCondition {
                                signature: event.signature.clone(),
                                expression: None,
                            });
                        }
                    }
                }
            }
        }
    }

    async fn decode_events(
        &self,
        events: &Vec<StellarEvent>,
        monitored_addresses: &[String],
    ) -> Vec<EventMap> {
        let mut decoded_events = Vec::new();
        for event in events {
            // Skip if contract address doesn't match
            if !monitored_addresses.contains(&normalize_address(&event.contract_id)) {
                continue;
            }

            let topics = match &event.topic_xdr {
                Some(topics) => topics,
                None => {
                    warn!("No topics found in event");
                    continue;
                }
            };

            // Decode base64 event name
            let event_name = match base64::engine::general_purpose::STANDARD.decode(&topics[0]) {
                Ok(bytes) => {
                    // Skip the first 4 bytes (size) and the next 4 bytes (type)
                    if bytes.len() >= 8 {
                        match String::from_utf8(bytes[8..].to_vec()) {
                            Ok(name) => name.trim_matches(char::from(0)).to_string(),
                            Err(e) => {
                                warn!("Failed to decode event name as UTF-8: {}", e);
                                continue;
                            }
                        }
                    } else {
                        warn!("Event name bytes too short: {}", bytes.len());
                        continue;
                    }
                }
                Err(e) => {
                    warn!("Failed to decode base64 event name: {}", e);
                    continue;
                }
            };

            // Process indexed parameters from topics
            let mut indexed_args = Vec::new();
            for topic in topics.iter().skip(1) {
                match base64::engine::general_purpose::STANDARD.decode(topic) {
                    Ok(bytes) => {
                        if let Some(param_entry) = parse_xdr_value(&bytes, true) {
                            indexed_args.push(param_entry);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to decode base64 topic: {}", e);
                        continue;
                    }
                }
            }

            // Process non-indexed parameters from value field
            let mut value_args = Vec::new();
            if let Some(value_xdr) = &event.value_xdr {
                match base64::engine::general_purpose::STANDARD.decode(value_xdr) {
                    Ok(bytes) => {
                        if let Some(entry) = parse_xdr_value(&bytes, false) {
                            value_args.push(entry);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to decode base64 event value: {}", e);
                        continue;
                    }
                }
            }

            let event_signature = format!(
                "{}({}{})",
                event_name,
                indexed_args
                    .iter()
                    .map(|arg| arg.kind.clone())
                    .collect::<Vec<String>>()
                    .join(","),
                if !value_args.is_empty() {
                    // Only add a comma if there were indexed args
                    if !indexed_args.is_empty() {
                        format!(
                            ",{}",
                            value_args
                                .iter()
                                .map(|arg| arg.kind.clone())
                                .collect::<Vec<String>>()
                                .join(",")
                        )
                    } else {
                        // No comma needed if there were no indexed args
                        value_args
                            .iter()
                            .map(|arg| arg.kind.clone())
                            .collect::<Vec<String>>()
                            .join(",")
                    }
                } else {
                    String::new()
                }
            );

            let decoded_event = StellarMatchParamsMap {
                signature: event_signature,
                args: Some(
                    [&indexed_args[..], &value_args[..]]
                        .concat()
                        .iter()
                        .enumerate()
                        .map(|(i, arg)| StellarMatchParamEntry {
                            kind: arg.kind.clone(),
                            value: arg.value.clone(),
                            indexed: arg.indexed,
                            name: i.to_string(),
                        })
                        .collect(),
                ),
            };

            decoded_events.push(EventMap {
                event: decoded_event,
                tx_hash: event.transaction_hash.clone(),
            });
        }

        return decoded_events;
    }

    fn compare_values(
        &self,
        param_type: &str,
        param_value: &str,
        operator: &str,
        compare_value: &str,
    ) -> bool {
        match param_type {
            "Bool" | "bool" => self.compare_bool(param_value, operator, compare_value),
            "U32" | "u32" => self.compare_u32(param_value, operator, compare_value),
            "U64" | "u64" | "Timepoint" | "timepoint" | "Duration" | "duration" => {
                self.compare_u64(param_value, operator, compare_value)
            }
            "I32" | "i32" => self.compare_i32(param_value, operator, compare_value),
            "I64" | "i64" => self.compare_i64(param_value, operator, compare_value),
            "U128" | "u128" => self.compare_u128(param_value, operator, compare_value),
            "I128" | "i128" => self.compare_i128(param_value, operator, compare_value),
            "U256" | "u256" | "I256" | "i256" => {
                self.compare_i256(param_value, operator, compare_value)
            }
            "Vec" | "vec" => self.compare_vec(param_value, operator, compare_value),
            "Map" | "map" => self.compare_map(param_value, operator, compare_value),
            "String" | "string" | "Symbol" | "symbol" | "Address" | "address" | "Bytes"
            | "bytes" => self.compare_string(param_value, operator, compare_value),
            _ => {
                warn!("Unsupported parameter type: {}", param_type);
                false
            }
        }
    }

    fn compare_bool(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let Ok(param_value) = param_value.parse::<bool>() else {
            warn!("Failed to parse bool parameter value: {}", param_value);
            return false;
        };
        let Ok(compare_value) = compare_value.parse::<bool>() else {
            warn!("Failed to parse bool comparison value: {}", compare_value);
            return false;
        };
        match operator {
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!("Unsupported operator for bool type: {}", operator);
                false
            }
        }
    }

    fn compare_u64(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let Ok(param_value) = param_value.parse::<u64>() else {
            warn!("Failed to parse u64 parameter value: {}", param_value);
            return false;
        };
        let Ok(compare_value) = compare_value.parse::<u64>() else {
            warn!("Failed to parse u64 comparison value: {}", compare_value);
            return false;
        };
        match operator {
            ">" => param_value > compare_value,
            ">=" => param_value >= compare_value,
            "<" => param_value < compare_value,
            "<=" => param_value <= compare_value,
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!("Unsupported operator: {}", operator);
                false
            }
        }
    }

    fn compare_u32(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let Ok(param_value) = param_value.parse::<u32>() else {
            warn!("Failed to parse u32 parameter value: {}", param_value);
            return false;
        };
        let Ok(compare_value) = compare_value.parse::<u32>() else {
            warn!("Failed to parse u32 comparison value: {}", compare_value);
            return false;
        };
        match operator {
            ">" => param_value > compare_value,
            ">=" => param_value >= compare_value,
            "<" => param_value < compare_value,
            "<=" => param_value <= compare_value,
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!("Unsupported operator: {}", operator);
                false
            }
        }
    }

    fn compare_i32(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let Ok(param_value) = param_value.parse::<i32>() else {
            warn!("Failed to parse i32 parameter value: {}", param_value);
            return false;
        };
        let Ok(compare_value) = compare_value.parse::<i32>() else {
            warn!("Failed to parse i32 comparison value: {}", compare_value);
            return false;
        };
        match operator {
            ">" => param_value > compare_value,
            ">=" => param_value >= compare_value,
            "<" => param_value < compare_value,
            "<=" => param_value <= compare_value,
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!("Unsupported operator: {}", operator);
                false
            }
        }
    }

    fn compare_i64(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let Ok(param_value) = param_value.parse::<i64>() else {
            warn!("Failed to parse i64 parameter value: {}", param_value);
            return false;
        };
        let Ok(compare_value) = compare_value.parse::<i64>() else {
            warn!("Failed to parse i64 comparison value: {}", compare_value);
            return false;
        };
        match operator {
            ">" => param_value > compare_value,
            ">=" => param_value >= compare_value,
            "<" => param_value < compare_value,
            "<=" => param_value <= compare_value,
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!("Unsupported operator: {}", operator);
                false
            }
        }
    }

    fn compare_u128(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let Ok(param_value) = param_value.parse::<u128>() else {
            warn!("Failed to parse u128 parameter value: {}", param_value);
            return false;
        };
        let Ok(compare_value) = compare_value.parse::<u128>() else {
            warn!("Failed to parse u128 comparison value: {}", compare_value);
            return false;
        };
        match operator {
            ">" => param_value > compare_value,
            ">=" => param_value >= compare_value,
            "<" => param_value < compare_value,
            "<=" => param_value <= compare_value,
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!("Unsupported operator: {}", operator);
                false
            }
        }
    }

    fn compare_i128(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let Ok(param_value) = param_value.parse::<i128>() else {
            warn!("Failed to parse i128 parameter value: {}", param_value);
            return false;
        };
        let Ok(compare_value) = compare_value.parse::<i128>() else {
            warn!("Failed to parse i128 comparison value: {}", compare_value);
            return false;
        };
        match operator {
            ">" => param_value > compare_value,
            ">=" => param_value >= compare_value,
            "<" => param_value < compare_value,
            "<=" => param_value <= compare_value,
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!("Unsupported operator: {}", operator);
                false
            }
        }
    }

    fn compare_i256(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        match operator {
            "==" => param_value == compare_value,
            "!=" => param_value != compare_value,
            _ => {
                warn!(
                    "Only == and != operators are supported for i256: {}",
                    operator
                );
                false
            }
        }
    }

    fn compare_string(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        let normalized_param = param_value.trim().to_lowercase();
        let normalized_compare = compare_value.trim().to_lowercase();
        match operator {
            "==" => normalized_param == normalized_compare,
            "!=" => normalized_param != normalized_compare,
            _ => {
                warn!(
                    "Only == and != operators are supported for string types: {}",
                    operator
                );
                false
            }
        }
    }

    fn compare_vec(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        // Split by comma and trim whitespace
        let values: Vec<&str> = param_value.split(',').map(|s| s.trim()).collect();

        // arguments[0] contains "some_value"
        // arguments[0] == "value1,value2,value3"
        match operator {
            "contains" => values.contains(&compare_value),
            "==" => param_value == compare_value, // For exact array match
            "!=" => param_value != compare_value,
            _ => {
                warn!(
                    "Only contains, == and != operators are supported for vec type: {}",
                    operator
                );
                false
            }
        }
    }

    fn compare_map(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
        // Parse the map from JSON string
        let Ok(map_value) = serde_json::from_str::<serde_json::Value>(&param_value) else {
            warn!("Failed to parse map value: {}", param_value);
            return false;
        };

        // arguments[0].bool_key == true AND arguments[0].number_key > 100 AND arguments[0].array_key contains \"value\"
        // Determine the type based on the JSON value
        let param_type = match map_value {
            Value::Bool(_) => "Bool",
            Value::Number(ref n) => {
                if n.is_u64() {
                    if n.as_u64().unwrap() <= u32::MAX as u64 {
                        "U32"
                    } else {
                        "U64"
                    }
                } else if n.is_i64() {
                    if n.as_i64().unwrap() >= i32::MIN as i64
                        && n.as_i64().unwrap() <= i32::MAX as i64
                    {
                        "I32"
                    } else {
                        "I64"
                    }
                } else {
                    "String" // Fallback for other number types
                }
            }
            Value::Array(_) => "Vec",
            Value::Object(_) => "Map",
            _ => "String", // Default to string for other types
        };

        // Compare using the appropriate type comparison function
        self.compare_values(param_type, &map_value.to_string(), operator, compare_value)
    }

    fn evaluate_expression(
        &self,
        expression: &str,
        args: &Option<Vec<StellarMatchParamEntry>>,
    ) -> bool {
        let Some(args) = args else {
            return false;
        };

        // Split by OR to get highest level conditions
        let or_conditions: Vec<&str> = expression.split(" OR ").collect();

        // For OR logic, any condition being true makes the whole expression true
        for or_condition in or_conditions {
            // Split each OR condition by AND
            let and_conditions: Vec<&str> = or_condition.trim().split(" AND ").collect();

            // All AND conditions must be true
            let and_result = and_conditions.iter().all(|condition| {
                // Remove surrounding parentheses and trim
                let clean_condition = condition.trim().trim_matches(|c| c == '(' || c == ')');

                // Split into parts (param operator value)
                let parts: Vec<&str> = clean_condition.split_whitespace().collect();
                if parts.len() != 3 {
                    warn!("Invalid expression format: {}", clean_condition);
                    return false;
                }

                let [param_expr, operator, value] = [parts[0], parts[1], parts[2]];

                // Find the parameter and its type
                if param_expr.contains('[') {
                    // Array indexing: arguments[0][0]
                    let indices: Vec<usize> = param_expr
                        .split('[')
                        .skip(1)
                        .filter_map(|s| s.trim_end_matches(']').parse::<usize>().ok())
                        .collect();

                    if indices.len() != 2 || indices[0] >= args.len() {
                        warn!("Invalid array indices: {:?}", indices);
                        return false;
                    }

                    let param = &args[indices[0]];
                    let array_values: Vec<&str> = param.value.split(',').collect();
                    if indices[1] >= array_values.len() {
                        warn!("Array index out of bounds: {}", indices[1]);
                        return false;
                    }

                    self.compare_values(
                        &param.kind,
                        array_values[indices[1]].trim(),
                        operator,
                        value,
                    )
                } else if param_expr.contains('.') {
                    // Map access: map.key
                    let parts: Vec<&str> = param_expr.split('.').collect();
                    if parts.len() != 2 {
                        warn!("Invalid map access format: {}", param_expr);
                        return false;
                    }

                    let [map_name, key] = [parts[0], parts[1]];
                    let Some(param) = args.iter().find(|p| p.value == map_name) else {
                        warn!("Map {} not found", map_name);
                        return false;
                    };

                    // Parse the map and get the value for the key
                    let Ok(map_value) = serde_json::from_str::<serde_json::Value>(&param.value)
                    else {
                        warn!("Failed to parse map: {}", param.value);
                        return false;
                    };

                    let Some(key_value) = map_value.get(key) else {
                        warn!("Key {} not found in map", key);
                        return false;
                    };

                    self.compare_values(&param.kind, &key_value.to_string(), operator, value)
                } else {
                    // Regular parameter
                    let Some(param) = args.iter().find(|p| p.name == param_expr) else {
                        warn!("Parameter {} not found", param_expr);
                        return false;
                    };

                    self.compare_values(&param.kind, &param.value, operator, value)
                }
            });

            if and_result {
                return true;
            }
        }

        false
    }

    fn convert_arguments_to_match_param_entry(
        &self,
        arguments: &Vec<Value>,
    ) -> Vec<StellarMatchParamEntry> {
        let mut params = Vec::new();
        for (index, arg) in arguments.iter().enumerate() {
            match arg {
                Value::Array(array) => {
                    // Handle nested arrays
                    params.push(StellarMatchParamEntry {
                        name: index.to_string(),
                        kind: "Vec".to_string(),
                        value: serde_json::to_string(array).unwrap_or_default(),
                        indexed: false,
                    });
                }
                Value::Object(map) => {
                    // Check for the new structure
                    if let (Some(Value::String(type_str)), Some(Value::String(value))) =
                        (map.get("type"), map.get("value"))
                    {
                        // Handle the new structure
                        params.push(StellarMatchParamEntry {
                            name: index.to_string(),
                            kind: type_str.clone(),
                            value: value.clone(),
                            indexed: false,
                        });
                    } else {
                        // Handle generic objects
                        params.push(StellarMatchParamEntry {
                            name: index.to_string(),
                            kind: "Map".to_string(),
                            value: serde_json::to_string(map).unwrap_or_default(),
                            indexed: false,
                        });
                    }
                }
                _ => {
                    // Handle primitive values
                    params.push(StellarMatchParamEntry {
                        name: index.to_string(),
                        kind: match arg {
                            Value::Number(n) if n.is_u64() => "U64".to_string(),
                            Value::Number(n) if n.is_i64() => "I64".to_string(),
                            Value::Bool(_) => "Bool".to_string(),
                            _ => "String".to_string(),
                        },
                        value: arg.as_str().unwrap_or("").to_string(),
                        indexed: false,
                    });
                }
            }
        }

        params
    }
}

#[async_trait]
impl BlockFilter for StellarBlockFilter {
    async fn filter_block(
        &self,
        client: &BlockChainClientEnum,
        _network: &Network,
        block: &BlockType,
        monitors: &[Monitor],
    ) -> Result<Vec<MonitorMatch>, FilterError> {
        let stellar_block = match block {
            BlockType::Stellar(block) => block,
            _ => {
                return Err(FilterError::block_type_mismatch(
                    "Expected Stellar block".to_string(),
                ))
            }
        };

        let mut matching_results = Vec::new();

        let stellar_client = match client {
            BlockChainClientEnum::Stellar(client) => client,
            _ => {
                return Err(FilterError::internal_error(
                    "Expected Stellar client".to_string(),
                ));
            }
        };

        let transactions = stellar_client
            .get_transactions(stellar_block.sequence, None)
            .await
            .map_err(|e| {
                FilterError::network_error(format!("Failed to get transactions: {}", e))
            })?;

        if transactions.is_empty() {
            info!("No transactions found for block {}", stellar_block.sequence);
            return Ok(vec![]);
        }

        info!("Processing {} transaction(s)", transactions.len());

        let events = stellar_client
            .get_events(stellar_block.sequence, None)
            .await
            .map_err(|e| FilterError::network_error(format!("Failed to get events: {}", e)))?;

        info!("Processing {} event(s)", events.len());

        // Process each monitor first
        for monitor in monitors {
            info!("Processing monitor: {}", monitor.name);

            let monitored_addresses = monitor
                .addresses
                .iter()
                .map(|addr| normalize_address(&addr.address))
                .collect::<Vec<String>>();

            let decoded_events = self.decode_events(&events, &monitored_addresses).await;

            // Then process transactions for this monitor
            for transaction in &transactions {
                let mut matched_transactions = Vec::<TransactionCondition>::new();
                let mut matched_functions = Vec::<FunctionCondition>::new();
                let mut matched_events = Vec::<EventCondition>::new();
                let mut matched_on_args = StellarMatchArguments {
                    events: Some(Vec::new()),
                    functions: Some(Vec::new()),
                };

                info!("Processing transaction: {:?}", transaction.hash());

                self.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

                // Decoded events already account for monitored addresses, so no need to pass in monitored_addresses
                self.find_matching_events_for_transaction(
                    &decoded_events,
                    &transaction,
                    &monitor,
                    &mut matched_events,
                    &mut matched_on_args,
                );

                self.find_matching_functions_for_transaction(
                    &monitored_addresses,
                    &transaction,
                    &monitor,
                    &mut matched_functions,
                    &mut matched_on_args,
                );

                let monitor_conditions = &monitor.match_conditions;
                let has_event_match =
                    !monitor_conditions.events.is_empty() && !matched_events.is_empty();
                let has_function_match =
                    !monitor_conditions.functions.is_empty() && !matched_functions.is_empty();
                let has_transaction_match =
                    !monitor_conditions.transactions.is_empty() && !matched_transactions.is_empty();

                let should_match = match (
                    monitor_conditions.events.is_empty(),
                    monitor_conditions.functions.is_empty(),
                    monitor_conditions.transactions.is_empty(),
                ) {
                    // Case 1: No conditions defined, match everything
                    (true, true, true) => true,

                    // Case 2: Only transaction conditions defined
                    (true, true, false) => has_transaction_match,

                    // Case 3: No transaction conditions, match based on events/functions
                    (_, _, true) => has_event_match || has_function_match,

                    // Case 4: Transaction conditions exist, they must be satisfied along with events/functions
                    _ => (has_event_match || has_function_match) && has_transaction_match,
                };

                if should_match {
                    matching_results.push(MonitorMatch::Stellar(StellarMonitorMatch {
                        monitor: monitor.clone(),
                        transaction: StellarTransaction::from(transaction.clone()),
                        ledger: stellar_block.clone(),
                        matched_on: MatchConditions {
                            events: matched_events
                                .clone()
                                .into_iter()
                                .filter(|_| has_event_match)
                                .collect(),
                            functions: matched_functions
                                .clone()
                                .into_iter()
                                .filter(|_| has_function_match)
                                .collect(),
                            transactions: matched_transactions
                                .clone()
                                .into_iter()
                                .filter(|_| has_transaction_match)
                                .collect(),
                        },
                        matched_on_args: Some(StellarMatchArguments {
                            events: if has_event_match {
                                matched_on_args.events.clone()
                            } else {
                                None
                            },
                            functions: if has_function_match {
                                matched_on_args.functions.clone()
                            } else {
                                None
                            },
                        }),
                    }));
                }
            }
        }
        Ok(matching_results)
    }
}
