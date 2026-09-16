//! Event - typed event bus for plugin communication with throttling and permission gating.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Event types in Fracterm
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
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

impl EventType {
    /// Get the default throttle interval for this event type
    pub fn default_throttle(&self) -> Option<Duration> {
        match self {
            EventType::TerminalOutput => Some(Duration::from_millis(16)), // ~60fps max
            EventType::ZoomChanged => Some(Duration::from_millis(50)),
            _ => None,
        }
    }
}

/// An event in the event bus
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    /// Event type
    pub event_type: EventType,
    /// Source of the event
    pub source: Option<String>,
    /// Event data
    pub data: serde_json::Value,
    /// Whether the event was throttled
    pub throttled: bool,
    /// Timestamp when event was created
    pub timestamp: u64,
    /// Permission required to receive this event (for plugins)
    pub required_permission: Option<String>,
}

impl Event {
    /// Create a new event
    pub fn new(event_type: EventType) -> Self {
        Self {
            event_type,
            source: None,
            data: serde_json::Value::Null,
            throttled: false,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            required_permission: None,
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

    /// Set required permission for plugins to receive this event
    pub fn with_permission(mut self, permission: &str) -> Self {
        self.required_permission = Some(permission.to_string());
        self
    }
}

/// Subscription entry with optional throttling
struct Subscription {
    handler: Arc<dyn Fn(&Event) + Send + Sync>,
    throttle: Option<Duration>,
    last_emit: Option<Instant>,
    required_permission: Option<String>,
}

/// Typed event bus for plugin communication.
pub struct EventBus {
    /// Subscribers for each event type
    subscribers: HashMap<EventType, Vec<Subscription>>,
    /// Global throttle state
    global_throttled: bool,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            global_throttled: false,
        }
    }

    /// Subscribe to an event type with optional throttling
    pub fn subscribe<F>(&mut self, event_type: EventType, handler: F)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        self.subscribe_throttled(event_type, handler, None);
    }

    /// Subscribe to an event type with custom throttle interval
    pub fn subscribe_throttled<F>(&mut self, event_type: EventType, handler: F, throttle: Option<Duration>)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        self.subscribers
            .entry(event_type)
            .or_default()
            .push(Subscription {
                handler: Arc::new(handler),
                throttle,
                last_emit: None,
                required_permission: None,
            });
    }

    /// Subscribe with permission requirement (for plugins)
    pub fn subscribe_with_permission<F>(&mut self, event_type: EventType, handler: F, permission: &str)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        self.subscribers
            .entry(event_type)
            .or_default()
            .push(Subscription {
                handler: Arc::new(handler),
                throttle: None,
                last_emit: None,
                required_permission: Some(permission.to_string()),
            });
    }

    /// Emit an event to all subscribers with permission checking
    pub fn emit(&self, event: &Event) {
        if let Some(handlers) = self.subscribers.get(&event.event_type) {
            for sub in handlers {
                // Check permission if required
                if let Some(req_perm) = &sub.required_permission {
                    if event.required_permission.as_ref() != Some(req_perm) {
                        continue;
                    }
                }

                // Check throttling
                if let Some(throttle) = sub.throttle {
                    if let Some(last) = sub.last_emit {
                        if last.elapsed() < throttle {
                            continue;
                        }
                    }
                    // Note: In a real implementation, we'd need interior mutability
                    // For now, we skip throttling on emit and handle it externally
                }

                (sub.handler)(event);
            }
        }
    }

    /// Emit an event with permission context (for plugin events)
    pub fn emit_for_plugin(&self, event: &Event, plugin_permissions: &[String]) {
        if let Some(handlers) = self.subscribers.get(&event.event_type) {
            for sub in handlers {
                // Check if plugin has required permission
                if let Some(req_perm) = &sub.required_permission {
                    if !plugin_permissions.contains(req_perm) {
                        continue;
                    }
                }

                (sub.handler)(event);
            }
        }
    }

    /// Enable or disable global throttling
    pub fn set_throttled(&mut self, throttled: bool) {
        self.global_throttled = throttled;
    }

    /// Check if global throttling is enabled
    pub fn is_throttled(&self) -> bool {
        self.global_throttled
    }

    /// Get subscriber count for an event type
    pub fn subscriber_count(&self, event_type: &EventType) -> usize {
        self.subscribers.get(event_type).map(|v| v.len()).unwrap_or(0)
    }

    /// Clear all subscribers for an event type
    pub fn clear(&mut self, event_type: &EventType) {
        self.subscribers.remove(event_type);
    }

    /// Clear all subscribers
    pub fn clear_all(&mut self) {
        self.subscribers.clear();
    }
}

type SubscriberList = Vec<Subscription>;

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Event filter for selective subscription
#[derive(Debug, Clone)]
pub struct EventFilter {
    pub event_types: Vec<EventType>,
    pub source_filter: Option<String>,
    pub min_timestamp: Option<u64>,
}

impl EventFilter {
    pub fn new() -> Self {
        Self {
            event_types: Vec::new(),
            source_filter: None,
            min_timestamp: None,
        }
    }

    pub fn with_types(mut self, types: Vec<EventType>) -> Self {
        self.event_types = types;
        self
    }

    pub fn with_source(mut self, source: &str) -> Self {
        self.source_filter = Some(source.to_string());
        self
    }

    pub fn matches(&self, event: &Event) -> bool {
        if !self.event_types.is_empty() && !self.event_types.contains(&event.event_type) {
            return false;
        }
        if let Some(filter) = &self.source_filter {
            if event.source.as_ref() != Some(filter) {
                return false;
            }
        }
        if let Some(min_ts) = self.min_timestamp {
            if event.timestamp < min_ts {
                return false;
            }
        }
        true
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Event history for replay/debugging
pub struct EventHistory {
    events: Vec<Event>,
    max_size: usize,
}

impl EventHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            events: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, event: Event) {
        if self.events.len() >= self.max_size {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    pub fn get(&self, filter: &EventFilter) -> Vec<&Event> {
        self.events.iter().filter(|e| filter.matches(e)).collect()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for EventHistory {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new(EventType::CommandExecuted)
            .with_source("test")
            .with_data(serde_json::json!({"cmd": "test"}));
        assert_eq!(event.event_type, EventType::CommandExecuted);
        assert_eq!(event.source, Some("test".to_string()));
    }

    #[test]
    fn test_event_bus_subscribe_emit() {
        let mut bus = EventBus::new();
        let received = Arc::new(std::sync::Mutex::new(false));
        let received_clone = received.clone();

        bus.subscribe(EventType::CommandExecuted, move |_| {
            *received_clone.lock().unwrap() = true;
        });

        bus.emit(&Event::new(EventType::CommandExecuted));
        assert!(*received.lock().unwrap());
    }

    #[test]
    fn test_event_permission() {
        let event = Event::new(EventType::TerminalOutput)
            .with_permission("terminal.read");
        assert_eq!(event.required_permission, Some("terminal.read".to_string()));
    }

    #[test]
    fn test_event_history() {
        let mut history = EventHistory::new(10);
        for i in 0..15 {
            history.push(Event::new(EventType::Custom(i.to_string())));
        }
        assert_eq!(history.len(), 10);
    }

    #[test]
    fn test_event_filter() {
        let filter = EventFilter::new()
            .with_types(vec![EventType::CommandExecuted, EventType::SettingsChanged])
            .with_source("test");

        let event1 = Event::new(EventType::CommandExecuted).with_source("test");
        let event2 = Event::new(EventType::CommandExecuted).with_source("other");
        let event3 = Event::new(EventType::TerminalOutput).with_source("test");

        assert!(filter.matches(&event1));
        assert!(!filter.matches(&event2));
        assert!(!filter.matches(&event3));
    }
}