//! Bridges generic `/proc` lifecycle and IO notifications into AgentFS events.

use std::{any::Any, sync::Arc};

use alan_ap::{
    FileServer, ProcessEvent, ProcessEventSink, ProcessInputEventSink, ProcessIoEventKind,
    ProcessIoEventSink, ProcessOutputEventSink,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use super::State;
use crate::AgentFs;

pub(super) fn agent_event_sink<T>(agent: &Arc<T>) -> Option<Arc<AgentFs>>
where
    T: FileServer + Any + 'static,
{
    let erased: Arc<dyn Any + Send + Sync> = agent.clone();
    Arc::downcast::<AgentFs>(erased).ok()
}

pub(super) fn process_event_sink(state: Arc<Mutex<State>>) -> Arc<dyn ProcessEventSink> {
    Arc::new(AgentProcessEventSink { state })
}

pub(super) fn io_event_sink(state: Arc<Mutex<State>>) -> Arc<dyn ProcessIoEventSink> {
    Arc::new(AgentIoEventSink { state })
}

pub(super) fn input_event_sink(state: Arc<Mutex<State>>) -> Arc<dyn ProcessInputEventSink> {
    Arc::new(AgentInputEventSink { state })
}

pub(super) fn output_event_sink(state: Arc<Mutex<State>>) -> Arc<dyn ProcessOutputEventSink> {
    Arc::new(AgentOutputEventSink { state })
}

async fn registered_agent(state: &Mutex<State>, pid: &str) -> Option<Arc<AgentFs>> {
    state
        .lock()
        .await
        .agents
        .get(pid)
        .and_then(|agent| agent.event_sink.clone())
}

struct AgentProcessEventSink {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl ProcessEventSink for AgentProcessEventSink {
    async fn process_event(&self, pid: &str, event: ProcessEvent) {
        if let Some(agent) = registered_agent(&self.state, pid).await {
            match event {
                ProcessEvent::Input { count } => agent.append_input_event(count).await,
                ProcessEvent::Output { count } => agent.append_output_event(count).await,
                ProcessEvent::Status { status } => agent.append_status_event(&status).await,
            }
        }
    }
}

struct AgentIoEventSink {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl ProcessIoEventSink for AgentIoEventSink {
    async fn io_appended(&self, pid: &str, kind: ProcessIoEventKind, count: u32) {
        if let Some(agent) = registered_agent(&self.state, pid).await {
            match kind {
                ProcessIoEventKind::Input => agent.append_input_event(count).await,
                ProcessIoEventKind::Output => agent.append_output_event(count).await,
            }
        }
    }
}

struct AgentInputEventSink {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl ProcessInputEventSink for AgentInputEventSink {
    async fn input_appended(&self, pid: &str, count: u32) {
        if let Some(agent) = registered_agent(&self.state, pid).await {
            agent.append_input_event(count).await;
        }
    }
}

struct AgentOutputEventSink {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl ProcessOutputEventSink for AgentOutputEventSink {
    async fn output_appended(&self, pid: &str, count: u32) {
        if let Some(agent) = registered_agent(&self.state, pid).await {
            agent.append_output_event(count).await;
        }
    }
}
