//! Host-owned dual-channel Bus (Pub/Sub + Request/Response).
//! Surfaces talk only through a scoped `BusProxy` — never a raw global Bus.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::manifest::{BusContract, ManifestPermissions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusError {
    PermissionDenied { action: &'static str, topic: String },
    UnknownTopic(String),
    ContractViolation(String),
    NoHandler(String),
    NotLoaded(String),
}

impl std::fmt::Display for BusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusError::PermissionDenied { action, topic } => {
                write!(f, "Bus permission denied: cannot {action} `{topic}`")
            }
            BusError::UnknownTopic(t) => write!(f, "unknown Bus topic `{t}`"),
            BusError::ContractViolation(m) => write!(f, "Bus contract violation: {m}"),
            BusError::NoHandler(t) => write!(f, "no handler for `{t}`"),
            BusError::NotLoaded(id) => write!(f, "Plugin not loaded on Bus: {id}"),
        }
    }
}

type EventHandler = Arc<dyn Fn(Value) + Send + Sync>;
type CallHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

#[derive(Default)]
struct BusState {
    /// topic → subscribers
    subscribers: HashMap<String, Vec<EventHandler>>,
    /// topic → request handler
    callers: HashMap<String, CallHandler>,
}

/// Internal Bus router. Not exposed to Plugin windows — only via `BusProxy`.
pub struct Bus {
    inner: Mutex<BusState>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(BusState::default()),
        }
    }

    pub fn proxy(
        self: &Arc<Self>,
        plugin_id: impl Into<String>,
        permissions: ManifestPermissions,
        contracts: HashMap<String, BusContract>,
    ) -> BusProxy {
        BusProxy {
            plugin_id: plugin_id.into(),
            permissions,
            contracts,
            bus: Arc::clone(self),
        }
    }

    fn emit_raw(&self, topic: &str, payload: Value) {
        let handlers = {
            let state = self.inner.lock().expect("bus");
            state.subscribers.get(topic).cloned().unwrap_or_default()
        };
        for handler in handlers {
            handler(payload.clone());
        }
    }

    fn subscribe_raw(&self, topic: &str, handler: EventHandler) {
        let mut state = self.inner.lock().expect("bus");
        state
            .subscribers
            .entry(topic.to_string())
            .or_default()
            .push(handler);
    }

    fn serve_raw(&self, topic: &str, handler: CallHandler) {
        let mut state = self.inner.lock().expect("bus");
        state.callers.insert(topic.to_string(), handler);
    }

    fn call_raw(&self, topic: &str, payload: Value) -> Result<Value, BusError> {
        let handler = {
            let state = self.inner.lock().expect("bus");
            state.callers.get(topic).cloned()
        };
        match handler {
            Some(h) => h(payload).map_err(BusError::ContractViolation),
            None => Err(BusError::NoHandler(topic.to_string())),
        }
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

/// Scoped Bus handle for one Plugin surface (window UI or Sidecar).
#[derive(Clone)]
pub struct BusProxy {
    plugin_id: String,
    permissions: ManifestPermissions,
    contracts: HashMap<String, BusContract>,
    bus: Arc<Bus>,
}

impl BusProxy {
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn emit(&self, topic: &str, payload: Value) -> Result<(), BusError> {
        self.ensure_permission("emit", topic, &self.permissions.emit)?;
        self.validate_event(topic, &payload)?;
        self.bus.emit_raw(topic, payload);
        Ok(())
    }

    pub fn subscribe<F>(&self, topic: &str, handler: F) -> Result<(), BusError>
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        self.ensure_permission("subscribe", topic, &self.permissions.subscribe)?;
        if !self.contracts.contains_key(topic) {
            return Err(BusError::UnknownTopic(topic.to_string()));
        }
        self.bus.subscribe_raw(topic, Arc::new(handler));
        Ok(())
    }

    pub fn serve<F>(&self, topic: &str, handler: F) -> Result<(), BusError>
    where
        F: Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
    {
        self.ensure_permission("call", topic, &self.permissions.call)?;
        if !self.contracts.contains_key(topic) {
            return Err(BusError::UnknownTopic(topic.to_string()));
        }
        let contracts = self.contracts.clone();
        let topic_owned = topic.to_string();
        self.bus.serve_raw(
            topic,
            Arc::new(move |req| {
                if let Some(contract) = contracts.get(&topic_owned) {
                    validate_against_schema(&req, &contract.request)
                        .map_err(|m| format!("request: {m}"))?;
                    let res = handler(req)?;
                    validate_against_schema(&res, &contract.response)
                        .map_err(|m| format!("response: {m}"))?;
                    Ok(res)
                } else {
                    handler(req)
                }
            }),
        );
        Ok(())
    }

    pub fn call(&self, topic: &str, payload: Value) -> Result<Value, BusError> {
        self.ensure_permission("call", topic, &self.permissions.call)?;
        self.validate_request(topic, &payload)?;
        let result = self.bus.call_raw(topic, payload)?;
        self.validate_response(topic, &result)?;
        Ok(result)
    }

    fn ensure_permission(
        &self,
        action: &'static str,
        topic: &str,
        allowed: &[String],
    ) -> Result<(), BusError> {
        if allowed.iter().any(|t| t == topic) {
            Ok(())
        } else {
            Err(BusError::PermissionDenied {
                action,
                topic: topic.to_string(),
            })
        }
    }

    fn validate_event(&self, topic: &str, payload: &Value) -> Result<(), BusError> {
        let Some(contract) = self.contracts.get(topic) else {
            return Err(BusError::UnknownTopic(topic.to_string()));
        };
        // Event contracts use `request` schema as the event payload shape.
        validate_against_schema(payload, &contract.request)
            .map_err(BusError::ContractViolation)
    }

    fn validate_request(&self, topic: &str, payload: &Value) -> Result<(), BusError> {
        let Some(contract) = self.contracts.get(topic) else {
            return Err(BusError::UnknownTopic(topic.to_string()));
        };
        validate_against_schema(payload, &contract.request).map_err(BusError::ContractViolation)
    }

    fn validate_response(&self, topic: &str, payload: &Value) -> Result<(), BusError> {
        let Some(contract) = self.contracts.get(topic) else {
            return Err(BusError::UnknownTopic(topic.to_string()));
        };
        validate_against_schema(payload, &contract.response).map_err(BusError::ContractViolation)
    }
}

/// Minimal JSON-object contract: `{ "type": "object", "required": [...], "properties": { k: { "type": "string"|... } } }`
pub fn validate_against_schema(value: &Value, schema: &Value) -> Result<(), String> {
    let Some(obj_schema) = schema.as_object() else {
        return Ok(());
    };
    let expected_type = obj_schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("object");
    match expected_type {
        "object" => {
            let Some(map) = value.as_object() else {
                return Err("expected object".into());
            };
            if let Some(required) = obj_schema.get("required").and_then(|r| r.as_array()) {
                for key in required {
                    let Some(name) = key.as_str() else { continue };
                    if !map.contains_key(name) {
                        return Err(format!("missing required field `{name}`"));
                    }
                }
            }
            if let Some(properties) = obj_schema.get("properties").and_then(|p| p.as_object()) {
                for (key, prop_schema) in properties {
                    if let Some(field) = map.get(key) {
                        validate_against_schema(field, prop_schema)?;
                    }
                }
            }
            Ok(())
        }
        "string" => {
            if value.is_string() {
                Ok(())
            } else {
                Err("expected string".into())
            }
        }
        "number" => {
            if value.is_number() {
                Ok(())
            } else {
                Err("expected number".into())
            }
        }
        "boolean" => {
            if value.is_boolean() {
                Ok(())
            } else {
                Err("expected boolean".into())
            }
        }
        other => Err(format!("unsupported schema type `{other}`")),
    }
}
