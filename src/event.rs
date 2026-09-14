//! Event - typed event bus for plugin communication.

use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Event types in Fracterm
#[derive(Debug, Clone)]
pub enum EventType {
    /// An object was created
    ObjectCreated,
    /// Terminal output event
    TerminalOutput,
    /// Zoom changed
    ZoomChanged,
    /// Plugin loaded
    PluginLoaded,
    /// Plugin unloaded
    PluginUnloaded,
    /// Command executed
    CommandExecuted,
    /// Settings changed
    SettingsChanged,
    /// Custom event with string name
    Custom(String),
}

/// An event in the event bus
#[derive(Debug, Clone)]
pub struct Event {
    /// Event type
    pub event_type: EventType,
    /// Source of the event
    pub source: Option<String>,
    /// Event data
    pub data: serde_json::Value,
    /// Whether the event was throttled
    pub throttled: bool,
}

impl Event {
    /// Create a new event
    pub fn new(event_type: EventType) -> Self {
        Self {
            event_type,
            source: None,
            data: serde_json::Value::Null,
            throttled: false,
        }
    }

    /// Set the source of the event
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// Set the data for the event
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }

    /// Mark the event as throttled
    pub fn throttled(mut self) -> Self {
        self.throttled = true;
        self
    }
}

/// Typed event bus for plugin communication.
pub struct EventBus {
    /// Subscribers for each event type
    subscribers: HashMap<EventType, Vec<Arc<dyn Fn(&Event) + Send + Sync>>>,
    /// Whether events are throttled
    throttled: bool,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            throttled: false,
        }
    }

    /// Subscribe to an event type
    pub fn subscribe<F>(&mut self, event_type: EventType, handler: F)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        self.subscribers
            .entry(event_type)
            .or_default()
            .push(Arc::new(handler));
    }

    /// Emit an event to all subscribers
    pub async fn emit(&self, event: &Event) {
        if let Some(handlers) = self.subscribers.get(&event.event_type) {
            for handler in handlers {
                handler(event);
            }
        }
    }

    /// Enable or disable throttling
    pub fn set_throttled(&mut self, throttled: bool) {
        self.throttled = throttled;
    }

    /// Check if events are throttled
    pub fn is_throttled(&self) -> bool {
        self.throttled
    }
}