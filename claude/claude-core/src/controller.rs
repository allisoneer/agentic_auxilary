//! Chat controller with streaming support

use crate::{
    amortize::AmortizedString,
    plugin::{ChatControllerPlugin, PluginId},
    snapshot::SnapshotAccumulator,
    state::{ChatState, ChatStateMutation, Message},
    CoreError, DEFAULT_MODEL,
};
use anthropic_async::{
    config::AnthropicConfig,
    types::{
        content::{MessageParam, MessageRole},
        messages::MessagesCreateRequest,
    },
    Client,
};
use futures::StreamExt;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use uuid::Uuid;

/// Accessor for the chat controller via weak reference
pub struct ChatControllerAccessor(Weak<Mutex<ChatController>>);

impl ChatControllerAccessor {
    /// Execute a function with the controller locked
    pub fn lock_with<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut ChatController) -> R,
    {
        let arc = self.0.upgrade()?;
        let mut guard = arc.lock().ok()?;
        Some(f(&mut guard))
    }
}

impl Clone for ChatControllerAccessor {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Main chat controller with streaming support
pub struct ChatController {
    /// Current chat state
    pub state: ChatState,
    plugins: HashMap<PluginId, Box<dyn ChatControllerPlugin>>,
    accessor: ChatControllerAccessor,
    client: Client<AnthropicConfig>,
}

impl ChatController {
    /// Create a new chat controller with the given configuration
    pub fn new(config: AnthropicConfig) -> Arc<Mutex<Self>> {
        Arc::new_cyclic(|weak| {
            Mutex::new(Self {
                state: ChatState {
                    model: DEFAULT_MODEL.to_string(),
                    ..Default::default()
                },
                plugins: HashMap::new(),
                accessor: ChatControllerAccessor(weak.clone()),
                client: Client::with_config(config),
            })
        })
    }

    /// Get an accessor for this controller
    pub fn accessor(&self) -> ChatControllerAccessor {
        self.accessor.clone()
    }

    /// Add a plugin and return its ID
    pub fn add_plugin(&mut self, plugin: Box<dyn ChatControllerPlugin>) -> PluginId {
        let id = PluginId::new();
        self.plugins.insert(id, plugin);
        id
    }

    /// Remove a plugin by ID
    pub fn remove_plugin(&mut self, id: PluginId) {
        self.plugins.remove(&id);
    }

    /// Dispatch a single mutation
    pub fn dispatch_mutation(&mut self, mutation: ChatStateMutation) {
        // Notify plugins before applying
        for plugin in self.plugins.values_mut() {
            plugin.on_state_mutation(&mutation, &self.state);
        }

        // Apply mutation
        let mutations = vec![mutation.clone()];
        mutation.apply(&mut self.state);

        // Notify plugins after applying
        for plugin in self.plugins.values_mut() {
            plugin.on_state_ready(&self.state, &mutations);
        }
    }

    /// Dispatch multiple mutations
    pub fn dispatch_mutations(&mut self, mutations: Vec<ChatStateMutation>) {
        for m in &mutations {
            for plugin in self.plugins.values_mut() {
                plugin.on_state_mutation(m, &self.state);
            }
            m.clone().apply(&mut self.state);
        }
        for plugin in self.plugins.values_mut() {
            plugin.on_state_ready(&self.state, &mutations);
        }
    }

    /// Send a message and stream the response
    pub async fn send_message(
        controller: Arc<Mutex<Self>>,
        prompt: String,
    ) -> Result<(), CoreError> {
        let (model, messages_context) = {
            let mut ctrl = controller.lock().unwrap();

            // Add user message
            let user_msg = Message {
                id: Uuid::new_v4().to_string(),
                role: "user".to_string(),
                content: prompt.clone(),
                created_at: chrono::Utc::now().timestamp(),
            };
            ctrl.dispatch_mutation(ChatStateMutation::PushMessage(user_msg));
            ctrl.dispatch_mutation(ChatStateMutation::SetIsStreaming(true));

            // Add placeholder assistant message
            let assistant_msg = Message {
                id: Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: String::new(),
                created_at: chrono::Utc::now().timestamp(),
            };
            ctrl.dispatch_mutation(ChatStateMutation::PushMessage(assistant_msg));

            // Build context
            let messages: Vec<MessageParam> = ctrl
                .state
                .messages
                .iter()
                .filter(|m| !m.content.is_empty())
                .map(|m| MessageParam {
                    role: if m.role == "user" {
                        MessageRole::User
                    } else {
                        MessageRole::Assistant
                    },
                    content: m.content.clone().into(),
                })
                .collect();

            (ctrl.state.model.clone(), messages)
        };

        // Create request
        let req = MessagesCreateRequest {
            model,
            max_tokens: 4096,
            messages: messages_context,
            stream: Some(true),
            ..Default::default()
        };

        // Get client (clone to avoid holding lock across await)
        let client = {
            let ctrl = controller.lock().unwrap();
            ctrl.client.clone()
        };

        // Get stream
        let stream = client
            .messages()
            .create_stream(req)
            .await
            .map_err(|e| CoreError::Api(e.to_string()))?;

        let mut stream = std::pin::pin!(stream);
        let mut acc = SnapshotAccumulator::new();
        let mut amort = AmortizedString::default();

        while let Some(event_result) = stream.next().await {
            let event = event_result.map_err(|e| CoreError::Api(e.to_string()))?;
            let (is_complete, snapshot_text) = acc.apply(&event)?;

            amort.update(snapshot_text);

            for chunk in &mut amort {
                let mut ctrl = controller.lock().unwrap();
                if let Some(last) = ctrl.state.messages.last() {
                    let updated = Message {
                        id: last.id.clone(),
                        role: "assistant".to_string(),
                        content: chunk,
                        created_at: last.created_at,
                    };
                    ctrl.dispatch_mutation(ChatStateMutation::UpdateLastMessage(updated));
                }
            }

            if is_complete {
                break;
            }
        }

        // Mark streaming complete
        {
            let mut ctrl = controller.lock().unwrap();
            ctrl.dispatch_mutation(ChatStateMutation::SetIsStreaming(false));
        }

        Ok(())
    }

    /// Send a message with persistence to the repository
    pub async fn send_message_with_persistence(
        controller: Arc<Mutex<Self>>,
        repository: Arc<crate::repository::Repository>,
        conversation_id: String,
        prompt: String,
    ) -> Result<(), CoreError> {
        let (model, messages_context, user_msg) = {
            let mut ctrl = controller.lock().unwrap();

            // Add user message
            let user_msg = Message {
                id: Uuid::new_v4().to_string(),
                role: "user".to_string(),
                content: prompt.clone(),
                created_at: chrono::Utc::now().timestamp(),
            };
            ctrl.dispatch_mutation(ChatStateMutation::PushMessage(user_msg.clone()));
            ctrl.dispatch_mutation(ChatStateMutation::SetIsStreaming(true));

            // Add placeholder assistant message
            let assistant_msg = Message {
                id: Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: String::new(),
                created_at: chrono::Utc::now().timestamp(),
            };
            ctrl.dispatch_mutation(ChatStateMutation::PushMessage(assistant_msg));

            // Build context
            let messages: Vec<MessageParam> = ctrl
                .state
                .messages
                .iter()
                .filter(|m| !m.content.is_empty())
                .map(|m| MessageParam {
                    role: if m.role == "user" {
                        MessageRole::User
                    } else {
                        MessageRole::Assistant
                    },
                    content: m.content.clone().into(),
                })
                .collect();

            (ctrl.state.model.clone(), messages, user_msg)
        };

        // Persist user message
        repository
            .append_message(&conversation_id, &user_msg)
            .await?;

        // Create request
        let req = MessagesCreateRequest {
            model,
            max_tokens: 4096,
            messages: messages_context,
            stream: Some(true),
            ..Default::default()
        };

        // Get client
        let client = {
            let ctrl = controller.lock().unwrap();
            ctrl.client.clone()
        };

        // Get stream
        let stream = client
            .messages()
            .create_stream(req)
            .await
            .map_err(|e| CoreError::Api(e.to_string()))?;

        let mut stream = std::pin::pin!(stream);
        let mut acc = SnapshotAccumulator::new();
        let mut amort = AmortizedString::default();

        while let Some(event_result) = stream.next().await {
            let event = event_result.map_err(|e| CoreError::Api(e.to_string()))?;
            let (is_complete, snapshot_text) = acc.apply(&event)?;

            amort.update(snapshot_text);

            for chunk in &mut amort {
                let mut ctrl = controller.lock().unwrap();
                if let Some(last) = ctrl.state.messages.last() {
                    let updated = Message {
                        id: last.id.clone(),
                        role: "assistant".to_string(),
                        content: chunk,
                        created_at: last.created_at,
                    };
                    ctrl.dispatch_mutation(ChatStateMutation::UpdateLastMessage(updated));
                }
            }

            if is_complete {
                break;
            }
        }

        // Mark streaming complete and get assistant message for persistence
        let assistant_msg = {
            let mut ctrl = controller.lock().unwrap();
            ctrl.dispatch_mutation(ChatStateMutation::SetIsStreaming(false));
            ctrl.state
                .messages
                .last()
                .filter(|m| m.role == "assistant")
                .cloned()
        };

        // Persist assistant message outside of lock
        if let Some(last_msg) = assistant_msg {
            repository
                .append_message(&conversation_id, &last_msg)
                .await?;
        }

        Ok(())
    }
}
