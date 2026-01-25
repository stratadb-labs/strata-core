//! Request/Response envelope encoding for Strata wire protocol
//!
//! Defines the wire format for API requests and responses:
//! - Request: `{id, op, params}`
//! - Success response: `{id, ok: true, result}`
//! - Error response: `{id, ok: false, error: {code, message, details}}`

use super::encode::{encode_json, encode_string};
use super::decode::{parse_json_object, DecodeError};
use strata_core::Value;
use std::collections::HashMap;

/// Wire protocol request
#[derive(Debug, Clone)]
pub struct Request {
    /// Request ID (echoed in response)
    pub id: String,
    /// Operation name (e.g., "kv.set", "json.get")
    pub op: String,
    /// Operation parameters
    pub params: RequestParams,
}

/// Request parameters for different operations
#[derive(Debug, Clone)]
pub enum RequestParams {
    /// Ping (no params)
    Ping,
    /// KV Get operation
    KvGet {
        /// Run ID
        run_id: String,
        /// Key to get
        key: String,
    },
    /// KV Set operation
    KvSet {
        /// Run ID
        run_id: String,
        /// Key to set
        key: String,
        /// Value to set
        value: Value,
    },
    /// Generic params as Value
    Generic(Value),
}

/// Wire protocol response
#[derive(Debug, Clone)]
pub struct Response {
    /// Request ID (from request)
    pub id: String,
    /// Success or failure
    pub ok: bool,
    /// Result (if ok=true)
    pub result: Option<Value>,
    /// Error (if ok=false)
    pub error: Option<ApiError>,
}

/// API error structure
#[derive(Debug, Clone)]
pub struct ApiError {
    /// Error code (e.g., "NotFound", "WrongType")
    pub code: String,
    /// Human-readable message
    pub message: String,
    /// Additional error details
    pub details: Option<Value>,
}

impl Response {
    /// Create a success response
    pub fn success(id: &str, result: Value) -> Self {
        Response {
            id: id.to_string(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: &str, error: ApiError) -> Self {
        Response {
            id: id.to_string(),
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

/// Encode request parameters
fn encode_params(params: &RequestParams) -> String {
    match params {
        RequestParams::Ping => "{}".to_string(),
        RequestParams::KvGet { run_id, key } => {
            format!(
                r#"{{"run_id":{},"key":{}}}"#,
                encode_string(run_id),
                encode_string(key)
            )
        }
        RequestParams::KvSet { run_id, key, value } => {
            format!(
                r#"{{"run_id":{},"key":{},"value":{}}}"#,
                encode_string(run_id),
                encode_string(key),
                encode_json(value)
            )
        }
        RequestParams::Generic(v) => encode_json(v),
    }
}

/// Encode a request to JSON
pub fn encode_request(request: &Request) -> String {
    format!(
        r#"{{"id":{},"op":{},"params":{}}}"#,
        encode_string(&request.id),
        encode_string(&request.op),
        encode_params(&request.params),
    )
}

/// Encode a response to JSON
pub fn encode_response(response: &Response) -> String {
    if response.ok {
        format!(
            r#"{{"id":{},"ok":true,"result":{}}}"#,
            encode_string(&response.id),
            encode_json(response.result.as_ref().unwrap_or(&Value::Null)),
        )
    } else {
        // Default error for malformed responses (ok=false but no error provided)
        let default_error = ApiError {
            code: "Internal".to_string(),
            message: "Unknown error".to_string(),
            details: None,
        };
        let error = response.error.as_ref().unwrap_or(&default_error);
        format!(
            r#"{{"id":{},"ok":false,"error":{{"code":{},"message":{},"details":{}}}}}"#,
            encode_string(&response.id),
            encode_string(&error.code),
            encode_string(&error.message),
            error
                .details
                .as_ref()
                .map(encode_json)
                .unwrap_or_else(|| "null".to_string()),
        )
    }
}

/// Decode a request from JSON
pub fn decode_request(json: &str) -> Result<Request, DecodeError> {
    let obj = parse_json_object(json)?;

    let id = match obj.get("id") {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(DecodeError::InvalidJson("Missing or invalid 'id'".to_string())),
    };

    let op = match obj.get("op") {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(DecodeError::InvalidJson("Missing or invalid 'op'".to_string())),
    };

    let params = match obj.get("params") {
        Some(v) => RequestParams::Generic(v.clone()),
        None => RequestParams::Generic(Value::Object(HashMap::new())),
    };

    Ok(Request { id, op, params })
}

/// Decode a response from JSON
pub fn decode_response(json: &str) -> Result<Response, DecodeError> {
    let obj = parse_json_object(json)?;

    let id = match obj.get("id") {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(DecodeError::InvalidJson("Missing or invalid 'id'".to_string())),
    };

    let ok = match obj.get("ok") {
        Some(Value::Bool(b)) => *b,
        _ => return Err(DecodeError::InvalidJson("Missing or invalid 'ok'".to_string())),
    };

    if ok {
        let result = obj.get("result").cloned();
        Ok(Response {
            id,
            ok: true,
            result,
            error: None,
        })
    } else {
        let error_obj = match obj.get("error") {
            Some(Value::Object(m)) => m,
            _ => return Err(DecodeError::InvalidJson("Missing or invalid 'error'".to_string())),
        };

        let code = match error_obj.get("code") {
            Some(Value::String(s)) => s.clone(),
            _ => return Err(DecodeError::InvalidJson("Missing error 'code'".to_string())),
        };

        let message = match error_obj.get("message") {
            Some(Value::String(s)) => s.clone(),
            _ => return Err(DecodeError::InvalidJson("Missing error 'message'".to_string())),
        };

        let details = error_obj.get("details").cloned();

        Ok(Response {
            id,
            ok: false,
            result: None,
            error: Some(ApiError {
                code,
                message,
                details,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Request Envelope ===

    #[test]
    fn test_request_envelope_structure() {
        let request = Request {
            id: "req-123".to_string(),
            op: "kv.get".to_string(),
            params: RequestParams::KvGet {
                run_id: "default".to_string(),
                key: "mykey".to_string(),
            },
        };

        let json = encode_request(&request);

        // Must have id, op, params
        assert!(json.contains(r#""id":"req-123""#));
        assert!(json.contains(r#""op":"kv.get""#));
        assert!(json.contains(r#""params":"#));
    }

    #[test]
    fn test_request_envelope_id_string() {
        let request = Request {
            id: "test-id".to_string(),
            op: "ping".to_string(),
            params: RequestParams::Ping,
        };

        let json = encode_request(&request);

        // ID must be a string, not a number
        assert!(json.contains(r#""id":"test-id""#));
    }

    #[test]
    fn test_request_ping() {
        let request = Request {
            id: "1".to_string(),
            op: "ping".to_string(),
            params: RequestParams::Ping,
        };

        let json = encode_request(&request);
        assert!(json.contains(r#""params":{}"#));
    }

    // === Success Response ===

    #[test]
    fn test_success_response_structure() {
        let response = Response::success("req-123", Value::Int(42));

        let json = encode_response(&response);

        // Must have id, ok=true, result
        assert!(json.contains(r#""id":"req-123""#));
        assert!(json.contains(r#""ok":true"#));
        assert!(json.contains(r#""result":42"#));
    }

    #[test]
    fn test_success_response_ok_is_bool() {
        let response = Response::success("test", Value::Null);

        let json = encode_response(&response);

        // ok must be boolean true, not 1 or "true"
        assert!(json.contains(r#""ok":true"#));
        assert!(!json.contains(r#""ok":1"#));
        assert!(!json.contains(r#""ok":"true""#));
    }

    #[test]
    fn test_success_response_with_null() {
        let response = Response::success("test", Value::Null);
        let json = encode_response(&response);
        assert!(json.contains(r#""result":null"#));
    }

    // === Error Response ===

    #[test]
    fn test_error_response_structure() {
        let response = Response::error(
            "req-123",
            ApiError {
                code: "NotFound".to_string(),
                message: "Key not found".to_string(),
                details: None,
            },
        );

        let json = encode_response(&response);

        // Must have id, ok=false, error
        assert!(json.contains(r#""id":"req-123""#));
        assert!(json.contains(r#""ok":false"#));
        assert!(json.contains(r#""error":"#));
    }

    #[test]
    fn test_error_response_ok_is_bool_false() {
        let response = Response::error(
            "test",
            ApiError {
                code: "Error".to_string(),
                message: "msg".to_string(),
                details: None,
            },
        );

        let json = encode_response(&response);

        // ok must be boolean false, not 0 or "false"
        assert!(json.contains(r#""ok":false"#));
        assert!(!json.contains(r#""ok":0"#));
        assert!(!json.contains(r#""ok":"false""#));
    }

    #[test]
    fn test_error_response_error_structure() {
        let response = Response::error(
            "req-123",
            ApiError {
                code: "WrongType".to_string(),
                message: "Expected Int".to_string(),
                details: Some(Value::Object({
                    let mut m = HashMap::new();
                    m.insert("expected".to_string(), Value::String("Int".into()));
                    m.insert("actual".to_string(), Value::String("Float".into()));
                    m
                })),
            },
        );

        let json = encode_response(&response);

        // error must have code, message, details
        assert!(json.contains(r#""code":"WrongType""#));
        assert!(json.contains(r#""message":"Expected Int""#));
        assert!(json.contains(r#""details":"#));
    }

    #[test]
    fn test_error_response_details_null() {
        let response = Response::error(
            "test",
            ApiError {
                code: "Error".to_string(),
                message: "msg".to_string(),
                details: None,
            },
        );

        let json = encode_response(&response);
        assert!(json.contains(r#""details":null"#));
    }

    // === ID Preservation ===

    #[test]
    fn test_request_id_preserved_in_response() {
        let request_id = "unique-request-id-12345";

        let request = Request {
            id: request_id.to_string(),
            op: "ping".to_string(),
            params: RequestParams::Ping,
        };

        let response = Response::success(&request.id, Value::Null);

        assert_eq!(response.id, request_id);
    }

    // === Round-trip ===

    #[test]
    fn test_request_roundtrip() {
        let request = Request {
            id: "test-123".to_string(),
            op: "kv.set".to_string(),
            params: RequestParams::Generic(Value::Object({
                let mut m = HashMap::new();
                m.insert("key".to_string(), Value::String("foo".to_string()));
                m.insert("value".to_string(), Value::Int(42));
                m
            })),
        };

        let json = encode_request(&request);
        let decoded = decode_request(&json).unwrap();

        assert_eq!(decoded.id, request.id);
        assert_eq!(decoded.op, request.op);
    }

    #[test]
    fn test_success_response_roundtrip() {
        let response = Response::success("req-456", Value::String("hello".to_string()));

        let json = encode_response(&response);
        let decoded = decode_response(&json).unwrap();

        assert_eq!(decoded.id, response.id);
        assert_eq!(decoded.ok, true);
        assert_eq!(decoded.result, response.result);
    }

    #[test]
    fn test_error_response_roundtrip() {
        let response = Response::error(
            "req-789",
            ApiError {
                code: "NotFound".to_string(),
                message: "Key not found".to_string(),
                details: Some(Value::Object({
                    let mut m = HashMap::new();
                    m.insert("key".to_string(), Value::String("missing".to_string()));
                    m
                })),
            },
        );

        let json = encode_response(&response);
        let decoded = decode_response(&json).unwrap();

        assert_eq!(decoded.id, response.id);
        assert_eq!(decoded.ok, false);
        let error = decoded.error.unwrap();
        assert_eq!(error.code, "NotFound");
        assert_eq!(error.message, "Key not found");
    }
}
