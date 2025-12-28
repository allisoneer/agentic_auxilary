//! Main application widget

use makepad_widgets::*;
use std::sync::{Arc, Mutex};

use anthropic_async::config::AnthropicConfig;
use claude_auth::{ApiKeyManager, KeyringStore, OAuthPkceManager};
use claude_core::{
    controller::ChatController,
    database::Database,
    repository::Repository,
    state::Conversation,
    DEFAULT_MODEL, AVAILABLE_MODELS,
};

use claude_core::state::Message;

/// Actions posted from async tasks to notify the UI thread
#[derive(Clone, DefaultNone)]
pub enum AppAction {
    None,
    /// Controller state was updated (new message content)
    ControllerUpdated,
    /// OAuth status changed (login succeeded/failed)
    OAuthStatusChanged,
    /// Database and repository initialized successfully
    DatabaseReady(Arc<Database>, Arc<Repository>),
    /// Conversations loaded from database
    ConversationsLoaded(Vec<Conversation>),
    /// Messages loaded for a conversation (used by on_conversation_clicked)
    #[allow(dead_code)]
    MessagesLoaded(String, Vec<Message>),
    /// Current conversation ID set (after creation)
    SetCurrentConversation(String),
    /// An error occurred that should be displayed
    Error(String),
}

impl std::fmt::Debug for AppAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppAction::None => write!(f, "None"),
            AppAction::ControllerUpdated => write!(f, "ControllerUpdated"),
            AppAction::OAuthStatusChanged => write!(f, "OAuthStatusChanged"),
            AppAction::DatabaseReady(_, _) => write!(f, "DatabaseReady(...)"),
            AppAction::ConversationsLoaded(c) => write!(f, "ConversationsLoaded({} items)", c.len()),
            AppAction::MessagesLoaded(id, m) => write!(f, "MessagesLoaded({}, {} msgs)", id, m.len()),
            AppAction::SetCurrentConversation(id) => write!(f, "SetCurrentConversation({})", id),
            AppAction::Error(msg) => write!(f, "Error({})", msg),
        }
    }
}

live_design! {
    use link::theme::*;
    use link::widgets::*;

    App = {{App}} {
        ui: <Window> {
            show_bg: true,
            width: Fill,
            height: Fill,
            draw_bg: { color: #282828 }
            
            body = <View> {
                flow: Right,
                
                // Sidebar for conversations
                sidebar = <View> {
                    width: 260,
                    height: Fill,
                    flow: Down,
                    spacing: 6,
                    padding: 10,
                    draw_bg: { color: #1d2021 }
                    
                    <Label> {
                        text: "Conversations"
                        draw_text: { 
                            color: #ebdbb2
                            text_style: { font_size: 16.0 }
                        }
                    }
                    
                    new_chat_button = <Button> {
                        text: "+ New Chat"
                        width: Fill,
                    }
                    
                    convo_list = <PortalList> {
                        width: Fill,
                        height: Fill
                        
                        ConvoItem = <View> {
                            width: Fill,
                            height: Fit,
                            padding: 8,
                            cursor: Hand,
                            draw_bg: { color: #3c3836 }
                            
                            convo_title = <Label> {
                                width: Fill,
                                text: "Untitled"
                                draw_text: { 
                                    color: #ebdbb2
                                    text_style: { font_size: 13.0 }
                                }
                            }
                        }
                    }
                }
                
                // Main chat area
                main_area = <View> {
                    width: Fill,
                    height: Fill,
                    flow: Down,
                    padding: 20,
                    spacing: 10,
                    
                    // Error banner (hidden by default)
                    error_banner = <View> {
                        visible: false,
                        width: Fill,
                        height: Fit,
                        padding: 10,
                        flow: Right,
                        spacing: 10,
                        align: { y: 0.5 }
                        draw_bg: { color: #cc241d }
                        
                        error_msg = <Label> {
                            width: Fill,
                            text: ""
                            draw_text: {
                                color: #fbf1c7
                                text_style: { font_size: 14.0 }
                            }
                        }
                        
                        error_close = <Button> {
                            text: "X"
                            width: Fit,
                        }
                    }
                    
                    // Header with title and model selector
                    header_row = <View> {
                        flow: Right,
                        height: Fit,
                        spacing: 10,
                        align: { y: 0.5 }
                        
                        <Label> {
                            text: "Claude Client"
                            draw_text: { 
                                text_style: { font_size: 24.0 }
                                color: #ebdbb2
                            }
                        }
                        
                        <View> { width: Fill, height: 1 }
                        
                        model_selector = <DropDown> {
                            width: 200,
                            labels: ["Claude Sonnet 4.5", "Claude Haiku 4.5", "Claude Opus 4.5"]
                        }
                        
                        theme_toggle = <Button> {
                            text: "Theme"
                            width: Fit,
                        }
                    }
                    
                    // Output area with Markdown
                    output_scroll = <ScrollYView> {
                        width: Fill,
                        height: Fill,
                        
                        output_md = <Markdown> {
                            width: Fill,
                            height: Fit,
                            body = {
                                width: Fill,
                            }
                        }
                        
                        // Fallback label for when markdown is empty
                        output_label = <Label> {
                            width: Fill,
                            height: Fit,
                            text: "Welcome! Enter your API key below and send a message."
                            draw_text: {
                                text_style: { font_size: 14.0 }
                                color: #a89984
                                wrap: Word
                            }
                        }
                    }
                    
                    // Input area
                    input_area = <View> {
                        flow: Right,
                        height: Fit,
                        spacing: 10,
                        
                        prompt_input = <TextInput> {
                            width: Fill,
                            height: Fit,
                            empty_text: "Type your message..."
                        }
                        
                        send_button = <Button> {
                            text: "Send"
                            width: Fit,
                        }
                    }
                    
                    // Auth area
                    auth_area = <View> {
                        flow: Right,
                        height: Fit,
                        spacing: 10,
                        align: { y: 0.5 }
                        
                        oauth_button = <Button> {
                            text: "Login with Claude Max"
                            width: Fit,
                        }
                        
                        <Label> {
                            text: "or"
                            draw_text: { color: #888 }
                        }
                        
                        api_key_input = <TextInput> {
                            width: 200,
                            height: Fit,
                            empty_text: "Enter API Key..."
                        }
                        
                        save_key_button = <Button> {
                            text: "Save Key"
                            width: Fit,
                        }
                        
                        backup_button = <Button> {
                            text: "Backup DB"
                            width: Fit,
                        }
                        
                        status_label = <Label> {
                            text: "Not authenticated"
                            draw_text: {
                                color: #888
                            }
                        }
                    }
                }
            }
        }
    }
}

// Register the app entry point at module level
app_main!(App);

#[derive(Live)]
pub struct App {
    #[live]
    ui: WidgetRef,

    #[rust]
    controller: Option<Arc<Mutex<ChatController>>>,

    #[rust]
    api_key_manager: Option<Arc<ApiKeyManager<KeyringStore>>>,

    #[rust]
    oauth_manager: Option<Arc<OAuthPkceManager<KeyringStore>>>,

    #[rust]
    db: Option<Arc<Database>>,

    #[rust]
    repo: Option<Arc<Repository>>,

    #[rust]
    current_conversation_id: Option<String>,

    #[rust]
    conversations: Vec<Conversation>,

    #[rust]
    last_displayed_content: String,

    #[rust]
    selected_model: String,

    #[rust]
    is_dark_theme: bool,
}

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        // MUST register makepad_widgets first
        makepad_widgets::live_design(cx);
    }
}

impl LiveHook for App {
    fn after_new_from_doc(&mut self, _cx: &mut Cx) {
        // Initialize auth managers
        let store = KeyringStore::new("claude-client");
        self.api_key_manager = Some(Arc::new(ApiKeyManager::new(store.clone())));
        self.oauth_manager = Some(Arc::new(OAuthPkceManager::new(store)));
        // Initialize model selection
        self.selected_model = DEFAULT_MODEL.to_string();
        // Default to dark theme
        self.is_dark_theme = true;
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // Initialize database in background
        crate::runtime::spawn(async move {
            match Database::open("claude_client.db").await {
                Ok(db) => {
                    let db = Arc::new(db);
                    match Repository::new(&db) {
                        Ok(repo) => {
                            let repo = Arc::new(repo);
                            tracing::info!("Database and repository ready");
                            Cx::post_action(AppAction::DatabaseReady(db, repo));
                        }
                        Err(e) => {
                            tracing::error!("Failed to create repository: {}", e);
                            Cx::post_action(AppAction::Error(format!("Repository error: {e}")));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to open database: {}", e);
                    Cx::post_action(AppAction::Error(format!("Database error: {e}")));
                }
            }
        });
        
        self.update_auth_status(cx);
    }
    
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        // Handle OAuth login button
        if self.ui.button(id!(oauth_button)).clicked(actions) {
            self.start_oauth_login(cx);
        }

        // Handle API key save button
        if self.ui.button(id!(save_key_button)).clicked(actions) {
            self.save_api_key(cx);
        }

        // Handle send button
        if self.ui.button(id!(send_button)).clicked(actions) {
            self.send_message(cx);
        }

        // Handle backup button
        if self.ui.button(id!(backup_button)).clicked(actions) {
            self.backup_database(cx);
        }

        // Handle new chat button
        if self.ui.button(id!(new_chat_button)).clicked(actions) {
            self.start_new_chat(cx);
        }

        // Handle theme toggle
        if self.ui.button(id!(theme_toggle)).clicked(actions) {
            self.toggle_theme(cx);
        }

        // Handle error banner close
        if self.ui.button(id!(error_close)).clicked(actions) {
            self.hide_error(cx);
        }

        // Handle model selector
        if let Some(dd) = self.ui.drop_down(id!(model_selector)).changed(actions) {
            if let Some((model_id, _name)) = AVAILABLE_MODELS.get(dd) {
                self.selected_model = model_id.to_string();
                tracing::info!("Model changed to: {}", self.selected_model);
                // Update controller if it exists
                if let Some(ctrl) = &self.controller {
                    if let Ok(mut c) = ctrl.lock() {
                        c.state.model = self.selected_model.clone();
                    }
                }
            }
        }

        // Handle async actions from Cx::post_action()
        for action in actions {
            if let Some(app_action) = action.downcast_ref::<AppAction>() {
                match app_action {
                    AppAction::ControllerUpdated => self.update_display(cx),
                    AppAction::OAuthStatusChanged => self.update_auth_status(cx),
                    AppAction::DatabaseReady(db, repo) => {
                        tracing::info!("Database ready, loading conversations");
                        self.db = Some(db.clone());
                        self.repo = Some(repo.clone());
                        // Load conversations
                        let repo = repo.clone();
                        crate::runtime::spawn(async move {
                            match repo.list_conversations().await {
                                Ok(convos) => {
                                    Cx::post_action(AppAction::ConversationsLoaded(convos));
                                }
                                Err(e) => {
                                    tracing::error!("Failed to list conversations: {}", e);
                                    Cx::post_action(AppAction::Error(format!("List conversations failed: {e}")));
                                }
                            }
                        });
                    }
                    AppAction::ConversationsLoaded(convos) => {
                        tracing::info!("Loaded {} conversations", convos.len());
                        self.conversations = convos.clone();
                        // Sidebar will show conversations when PortalList rendering is implemented
                    }
                    AppAction::MessagesLoaded(conv_id, messages) => {
                        tracing::info!("Loaded {} messages for conversation {}", messages.len(), conv_id);
                        self.current_conversation_id = Some(conv_id.clone());
                        // Load messages into controller
                        if self.controller.is_none() {
                            self.create_controller();
                        }
                        if let Some(ctrl) = &self.controller {
                            if let Ok(mut c) = ctrl.lock() {
                                c.state.messages = messages.clone();
                            }
                        }
                        // Update display with last message
                        if let Some(last) = messages.last() {
                            if last.role == "assistant" {
                                self.ui.markdown(id!(output_md)).set_text(cx, &last.content);
                                self.ui.label(id!(output_label)).set_text(cx, &last.content);
                                self.last_displayed_content = last.content.clone();
                            }
                        }
                        self.ui.redraw(cx);
                    }
                    AppAction::SetCurrentConversation(id) => {
                        tracing::info!("Set current conversation: {}", id);
                        self.current_conversation_id = Some(id.clone());
                    }
                    AppAction::Error(msg) => {
                        tracing::error!("App error: {}", msg);
                        self.show_error(cx, msg);
                    }
                    AppAction::None => {}
                }
            }
        }

        // Update display if controller state changed (fallback for direct calls)
        self.update_display(cx);
    }
}

impl App {
    fn start_oauth_login(&mut self, cx: &mut Cx) {
        if let Some(oauth) = &self.oauth_manager {
            let oauth = oauth.clone();
            
            self.ui.label(id!(status_label)).set_text(cx, "Opening browser...");
            
            crate::runtime::spawn(async move {
                match oauth.ensure_logged_in().await {
                    Ok(()) => {
                        tracing::info!("OAuth login successful");
                        Cx::post_action(AppAction::OAuthStatusChanged);
                    }
                    Err(e) => {
                        tracing::error!("OAuth login failed: {}", e);
                        Cx::post_action(AppAction::Error(format!("OAuth login failed: {e}")));
                    }
                }
            });
        }
    }

    fn save_api_key(&mut self, cx: &mut Cx) {
        let key = self.ui.text_input(id!(api_key_input)).text();
        if !key.is_empty() {
            if let Some(akm) = &self.api_key_manager {
                match akm.set_api_key(&key) {
                    Ok(()) => {
                        self.ui.label(id!(status_label))
                            .set_text(cx, "API key saved!");
                        self.ui.text_input(id!(api_key_input)).set_text(cx, "");
                    }
                    Err(e) => {
                        self.ui.label(id!(status_label))
                            .set_text(cx, &format!("Error: {e}"));
                    }
                }
            }
        }
        self.update_auth_status(cx);
    }

    fn update_auth_status(&mut self, cx: &mut Cx) {
        let has_api_key = self
            .api_key_manager
            .as_ref()
            .and_then(|a| a.get_api_key().ok().flatten())
            .is_some();
        
        let has_oauth = self
            .oauth_manager
            .as_ref()
            .map(|o| o.is_logged_in())
            .unwrap_or(false);

        let status = match (has_oauth, has_api_key) {
            (true, true) => "Authenticated (OAuth + API Key)",
            (true, false) => "Authenticated (OAuth)",
            (false, true) => "Authenticated (API Key)",
            (false, false) => "Not authenticated",
        };

        self.ui.label(id!(status_label)).set_text(cx, status);
    }

    fn send_message(&mut self, cx: &mut Cx) {
        let prompt = self.ui.text_input(id!(prompt_input)).text();
        if prompt.is_empty() {
            return;
        }

        // Clear input and show sending status
        self.ui.text_input(id!(prompt_input)).set_text(cx, "");
        self.ui.markdown(id!(output_md)).set_text(cx, "*Sending...*");
        self.ui.label(id!(output_label)).set_text(cx, "Sending...");
        self.ui.redraw(cx);

        // Get or create controller
        if self.controller.is_none() {
            self.create_controller();
        }

        let Some(controller) = &self.controller else { return };
        let Some(repo) = &self.repo else {
            // Fallback to non-persistent send if repo not ready
            let controller = controller.clone();
            let prompt = prompt.to_string();
            crate::runtime::spawn(async move {
                match ChatController::send_message(controller, prompt).await {
                    Ok(()) => Cx::post_action(AppAction::ControllerUpdated),
                    Err(e) => Cx::post_action(AppAction::Error(format!("Send failed: {e}"))),
                }
            });
            return;
        };

        // Ensure a conversation exists
        let conv_id = self.current_conversation_id.clone();
        let controller = controller.clone();
        let repo = repo.clone();
        let prompt_text = prompt.to_string();
        let is_first_message = self.current_conversation_id.is_none();

        crate::runtime::spawn(async move {
            // Create conversation if needed
            let conversation_id = if let Some(id) = conv_id {
                id
            } else {
                match repo.create_conversation("Untitled").await {
                    Ok(conv) => {
                        Cx::post_action(AppAction::SetCurrentConversation(conv.id.clone()));
                        conv.id
                    }
                    Err(e) => {
                        Cx::post_action(AppAction::Error(format!("Create conversation failed: {e}")));
                        return;
                    }
                }
            };

            // Send with persistence
            match ChatController::send_message_with_persistence(
                controller,
                repo.clone(),
                conversation_id.clone(),
                prompt_text.clone(),
            ).await {
                Ok(()) => {
                    // Auto-title from first user message
                    if is_first_message {
                        let title: String = prompt_text
                            .split_whitespace()
                            .take(6)
                            .collect::<Vec<_>>()
                            .join(" ");
                        let title = if title.len() > 60 {
                            format!("{}...", &title[..57])
                        } else {
                            title
                        };
                        if !title.is_empty() {
                            let _ = repo.update_conversation_title(&conversation_id, &title).await;
                        }
                    }
                    Cx::post_action(AppAction::ControllerUpdated);
                    // Refresh conversations list
                    if let Ok(convos) = repo.list_conversations().await {
                        Cx::post_action(AppAction::ConversationsLoaded(convos));
                    }
                }
                Err(e) => {
                    tracing::error!("Send failed: {}", e);
                    Cx::post_action(AppAction::Error(format!("Send failed: {e}")));
                }
            }
        });

        // Store conversation ID for future messages (will be set after creation)
        if self.current_conversation_id.is_none() {
            // We'll need to update this after the async operation completes
            // For now, the async task handles it internally
        }
    }

    fn create_controller(&mut self) {
        let mut config = AnthropicConfig::new();

        // Add API key if available
        if let Some(akm) = &self.api_key_manager {
            if let Ok(Some(key)) = akm.get_api_key() {
                config = config.with_api_key(key);
            }
        }

        let controller = ChatController::new(config);
        // Set the selected model
        if let Ok(mut c) = controller.lock() {
            c.state.model = self.selected_model.clone();
        }
        self.controller = Some(controller);
    }

    fn update_display(&mut self, cx: &mut Cx) {
        if let Some(controller) = &self.controller {
            if let Ok(ctrl) = controller.lock() {
                if let Some(last_msg) = ctrl.state.messages.last() {
                    if last_msg.role == "assistant" && last_msg.content != self.last_displayed_content {
                        // Update both Markdown and fallback label
                        self.ui.markdown(id!(output_md)).set_text(cx, &last_msg.content);
                        self.ui.label(id!(output_label)).set_text(cx, &last_msg.content);
                        self.last_displayed_content = last_msg.content.clone();
                        self.ui.redraw(cx);
                    }
                }
            }
        }
    }

    fn start_new_chat(&mut self, cx: &mut Cx) {
        // Clear current conversation state
        self.current_conversation_id = None;
        self.last_displayed_content.clear();
        
        // Clear controller messages
        if let Some(ctrl) = &self.controller {
            if let Ok(mut c) = ctrl.lock() {
                c.state.messages.clear();
            }
        }
        
        // Clear display
        self.ui.markdown(id!(output_md)).set_text(cx, "");
        self.ui.label(id!(output_label)).set_text(cx, "Start a new conversation...");
        self.ui.redraw(cx);
    }

    fn backup_database(&mut self, cx: &mut Cx) {
        let src = "claude_client.db";
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let dst = format!("claude_client.{}.db", ts);
        match std::fs::copy(src, &dst) {
            Ok(_) => {
                tracing::info!("Backup created: {}", dst);
                self.ui.label(id!(status_label)).set_text(cx, &format!("Backup: {dst}"));
            }
            Err(e) => {
                tracing::error!("Backup failed: {}", e);
                self.show_error(cx, &format!("Backup failed: {e}"));
            }
        }
    }

    fn show_error(&mut self, cx: &mut Cx, msg: &str) {
        self.ui.label(id!(error_msg)).set_text(cx, msg);
        self.ui.view(id!(error_banner)).set_visible(cx, true);
        self.ui.redraw(cx);
    }

    fn hide_error(&mut self, cx: &mut Cx) {
        self.ui.view(id!(error_banner)).set_visible(cx, false);
        self.ui.redraw(cx);
    }

    fn toggle_theme(&mut self, cx: &mut Cx) {
        self.is_dark_theme = !self.is_dark_theme;
        let theme_name = if self.is_dark_theme { "Dark" } else { "Light" };
        tracing::info!("Theme toggled to: {} (visual change requires theme files)", theme_name);
        self.ui.label(id!(status_label)).set_text(cx, &format!("Theme: {theme_name}"));
        // Note: Full theme switching requires cx.link() + cx.reload_ui_dsl()
        // which needs theme files to be properly set up. For now, just log the change.
    }

    /// Handle conversation item click - loads messages for the selected conversation
    /// Note: This is prepared infrastructure for PortalList item click handling
    #[allow(dead_code)]
    fn on_conversation_clicked(&mut self, cx: &mut Cx, id: &str) {
        tracing::info!("Conversation clicked: {}", id);
        let Some(repo) = &self.repo else {
            self.show_error(cx, "Repository not ready");
            return;
        };

        let repo = repo.clone();
        let id_owned = id.to_string();

        crate::runtime::spawn(async move {
            match repo.list_messages(&id_owned).await {
                Ok(msgs) => {
                    Cx::post_action(AppAction::MessagesLoaded(id_owned, msgs));
                }
                Err(e) => {
                    tracing::error!("Failed to load messages: {}", e);
                    Cx::post_action(AppAction::Error(format!("Load messages failed: {e}")));
                }
            }
        });
    }
}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Forward to MatchEvent
        self.match_event(cx, event);
        
        // Forward events to UI
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
