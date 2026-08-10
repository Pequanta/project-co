//! Telegram conversation router: maps incoming updates to application service
//! calls and replies through the gateway. This layer contains no business
//! logic of its own — sessions, plans, progress and authorization all live in
//! `app`.

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::app::{PlanService, ProgressService, SessionService, UserService};
use crate::domain::{Session, User};
use crate::error::AppError;
use crate::repo::ConversationRepo;
use crate::telegram::conversation::ConvState;
use crate::telegram::gateway::TelegramGateway;
use crate::telegram::types::{CallbackQuery, InlineKeyboardMarkup, Message, Update};
use crate::text;

const PREFIX_STATUS: &str = "st:";
const PREFIX_PLANS: &str = "pl:";
const PREFIX_MEMBERS: &str = "mb:";
const PREFIX_PROGRESS: &str = "pr:";
const PREFIX_PLAN_NEW: &str = "pn:";
const PREFIX_COMPLETE_LIST: &str = "cm:";
const PREFIX_COMPLETE: &str = "cp:";
const PREFIX_LEAVE: &str = "lv:";
const PREFIX_LEAVE_YES: &str = "cy:";
const PREFIX_LEAVE_NO: &str = "cx:";

pub struct BotRouter {
    db: PgPool,
    users: Arc<UserService>,
    sessions: Arc<SessionService>,
    plans: Arc<PlanService>,
    progress: Arc<ProgressService>,
    conversations: Arc<dyn ConversationRepo>,
    gateway: Arc<dyn TelegramGateway>,
}

impl BotRouter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: PgPool,
        users: Arc<UserService>,
        sessions: Arc<SessionService>,
        plans: Arc<PlanService>,
        progress: Arc<ProgressService>,
        conversations: Arc<dyn ConversationRepo>,
        gateway: Arc<dyn TelegramGateway>,
    ) -> Self {
        Self {
            db,
            users,
            sessions,
            plans,
            progress,
            conversations,
            gateway,
        }
    }

    pub async fn handle_update(&self, update: &Update) -> Result<(), AppError> {
        if let Some(msg) = &update.message {
            self.handle_message(msg).await?;
        }
        if let Some(cq) = &update.callback_query {
            self.handle_callback(cq).await?;
        }
        Ok(())
    }

    async fn ensure_user(&self, tg: &crate::telegram::types::User) -> Result<User, AppError> {
        self.users
            .ensure_registered(
                tg.id,
                tg.username.clone(),
                Some(&tg.first_name),
                tg.last_name.as_deref(),
            )
            .await
    }

    // --- Messages ---------------------------------------------------------

    async fn handle_message(&self, msg: &Message) -> Result<(), AppError> {
        let Some(from) = &msg.from else { return Ok(()) };
        if msg.chat.chat_type != "private" {
            let _ = self
                .gateway
                .send_message(
                    msg.chat.id,
                    "I only work in private chats. Open a chat with me to use sessions.",
                    None,
                )
                .await;
            return Ok(());
        }

        let user = self.ensure_user(from).await?;
        let text = msg.text.clone().unwrap_or_default();

        if let Some(cmd) = text.strip_prefix('/') {
            self.run_command(&user, msg.chat.id, cmd).await?;
            return Ok(());
        }

        let conv = self.get_conv(user.id).await?;
        if let Some(c) = conv {
            let state = ConvState::from_str(&c.state);
            if state != ConvState::Idle {
                self.continue_flow(&user, msg.chat.id, state, &text).await?;
                return Ok(());
            }
        }

        // Natural-language fallback: interpret plain text as a progress update.
        let list = self.sessions.list_sessions(user.id).await?;
        match list.len() {
            0 => {
                self.gateway
                    .send_message(msg.chat.id, &text::help_text(), Some(self.main_menu()))
                    .await?;
            }
            1 => {
                let sid = list[0].id;
                self.set_conv(
                    user.id,
                    msg.chat.id,
                    ConvState::AwaitingProgressMessage,
                    json!({"session_id": sid}),
                )
                .await?;
                self.gateway
                    .send_message(
                        msg.chat.id,
                        &format!("📁 *{}*\n\nWhat did you accomplish?", list[0].project_name),
                        None,
                    )
                    .await?;
            }
            _ => {
                self.send_chooser(
                    msg.chat.id,
                    "Which session is this update for?",
                    &list,
                    PREFIX_PROGRESS,
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn run_command(&self, user: &User, chat_id: i64, cmd: &str) -> Result<(), AppError> {
        let name = cmd
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match name.as_str() {
            "start" => {
                self.set_conv(user.id, chat_id, ConvState::Idle, json!({}))
                    .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        &text::welcome_text(&user.display_name),
                        Some(self.main_menu()),
                    )
                    .await?;
            }
            "help" => {
                self.gateway
                    .send_message(chat_id, &text::help_text(), Some(self.main_menu()))
                    .await?;
            }
            "create" => {
                self.set_conv(user.id, chat_id, ConvState::AwaitingProjectName, json!({}))
                    .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "Let's create a session.\n\nWhat's the *project name*?",
                        None,
                    )
                    .await?;
            }
            "join" => {
                self.set_conv(user.id, chat_id, ConvState::AwaitingSessionKey, json!({}))
                    .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "Enter your *session key* (e.g. `AB7K-X92P`):",
                        None,
                    )
                    .await?;
            }
            "status" | "plans" | "members" | "progress" | "plan" | "complete" | "leave" => {
                self.require_session(user, chat_id, &name).await?;
            }
            _ => {
                self.gateway
                    .send_message(chat_id, "Unknown command. Try /help.", None)
                    .await?;
            }
        }
        Ok(())
    }

    /// Commands that need a session: resolve directly if the user has exactly
    /// one, otherwise show a chooser with the matching intent prefix.
    async fn require_session(&self, user: &User, chat_id: i64, cmd: &str) -> Result<(), AppError> {
        let list = self.sessions.list_sessions(user.id).await?;
        if list.is_empty() {
            self.gateway
                .send_message(
                    chat_id,
                    "You're not in any sessions yet. Use /create to start one, or /join with a key.",
                    Some(self.main_menu()),
                )
                .await?;
            return Ok(());
        }
        if list.len() == 1 {
            self.act_on_session(user, chat_id, cmd, &list[0]).await?;
            return Ok(());
        }
        let prefix = match cmd {
            "status" => PREFIX_STATUS,
            "plans" => PREFIX_PLANS,
            "members" => PREFIX_MEMBERS,
            "progress" => PREFIX_PROGRESS,
            "plan" => PREFIX_PLAN_NEW,
            "complete" => PREFIX_COMPLETE_LIST,
            "leave" => PREFIX_LEAVE,
            _ => unreachable!(),
        };
        self.send_chooser(chat_id, "Which session?", &list, prefix)
            .await
    }

    async fn act_on_session(
        &self,
        user: &User,
        chat_id: i64,
        cmd: &str,
        session: &Session,
    ) -> Result<(), AppError> {
        match cmd {
            "status" => self.send_status(user, chat_id, session).await,
            "plans" => self.send_plans(user, chat_id, session).await,
            "members" => self.send_members(user, chat_id, session).await,
            "progress" => {
                self.set_conv(
                    user.id,
                    chat_id,
                    ConvState::AwaitingProgressMessage,
                    json!({"session_id": session.id}),
                )
                .await?;
                self.gateway
                    .send_message(chat_id, "What did you accomplish?", None)
                    .await?;
                Ok(())
            }
            "plan" => {
                self.set_conv(
                    user.id,
                    chat_id,
                    ConvState::AwaitingPlanTitle,
                    json!({"session_id": session.id}),
                )
                .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "What do you want to accomplish? Send me the *plan title*:",
                        None,
                    )
                    .await?;
                Ok(())
            }
            "complete" => self.send_complete_list(user, chat_id, session).await,
            "leave" => self.send_leave_confirm(chat_id, session).await,
            _ => unreachable!(),
        }
    }

    async fn send_status(
        &self,
        user: &User,
        chat_id: i64,
        session: &Session,
    ) -> Result<(), AppError> {
        let overview = self.sessions.status(user.id, session.id).await?;
        let member_names: Vec<String> = overview
            .members
            .iter()
            .map(|m| {
                let icon = if m.role == crate::domain::MemberRole::Owner {
                    "👑"
                } else {
                    "👤"
                };
                format!("{icon} {}", m.user.display_name)
            })
            .collect();
        let recent: Vec<(String, String)> = overview
            .recent_updates
            .iter()
            .map(|r| (r.author_name.clone(), r.message.clone()))
            .collect();
        let msg = text::dashboard_text(
            &overview.session.project_name,
            overview.session.project_description.as_deref(),
            overview.session.deadline,
            Utc::now(),
            overview.progress,
            overview.counts.completed,
            overview.counts.in_progress,
            overview.counts.planned,
            overview.counts.cancelled,
            &member_names,
            &recent,
        );
        self.gateway.send_message(chat_id, &msg, None).await?;
        Ok(())
    }

    async fn send_plans(
        &self,
        user: &User,
        chat_id: i64,
        session: &Session,
    ) -> Result<(), AppError> {
        let plans = self.plans.list_plans(user.id, session.id).await?;
        self.gateway
            .send_message(
                chat_id,
                &text::plans_text(&session.project_name, &plans),
                None,
            )
            .await?;
        Ok(())
    }

    async fn send_members(
        &self,
        user: &User,
        chat_id: i64,
        session: &Session,
    ) -> Result<(), AppError> {
        let overview = self.sessions.status(user.id, session.id).await?;
        let mut out = format!("👥 *{}* — Members\n", session.project_name);
        for m in &overview.members {
            let icon = if m.role == crate::domain::MemberRole::Owner {
                "👑"
            } else {
                "👤"
            };
            let handle = m
                .user
                .telegram_username
                .as_ref()
                .map(|u| format!(" (@{u})"))
                .unwrap_or_default();
            out.push_str(&format!("{icon} {}{handle}\n", m.user.display_name));
        }
        self.gateway.send_message(chat_id, &out, None).await?;
        Ok(())
    }

    /// List active (planned/in_progress) plans as tappable "complete" buttons.
    async fn send_complete_list(
        &self,
        user: &User,
        chat_id: i64,
        session: &Session,
    ) -> Result<(), AppError> {
        let plans = self.plans.list_plans(user.id, session.id).await?;
        let active: Vec<_> = plans.iter().filter(|p| p.status.is_active()).collect();
        if active.is_empty() {
            self.gateway
                .send_message(
                    chat_id,
                    &format!("No active plans in *{}*. 🎉", session.project_name),
                    None,
                )
                .await?;
            return Ok(());
        }
        let buttons = active
            .iter()
            .map(|p| {
                (
                    format!("✅ {}", p.title),
                    format!("{PREFIX_COMPLETE}{}", p.id),
                )
            })
            .collect();
        let markup = InlineKeyboardMarkup::single_row(buttons);
        self.gateway
            .send_message(
                chat_id,
                &format!(
                    "Tap a plan to mark it *completed*:\n\n📁 {}",
                    session.project_name
                ),
                Some(markup),
            )
            .await?;
        Ok(())
    }

    async fn send_leave_confirm(&self, chat_id: i64, session: &Session) -> Result<(), AppError> {
        let markup = InlineKeyboardMarkup::two_columns(vec![vec![
            (
                "Yes, leave".to_string(),
                format!("{PREFIX_LEAVE_YES}{}", session.id),
            ),
            ("Cancel".to_string(), PREFIX_LEAVE_NO.to_string()),
        ]]);
        self.gateway
            .send_message(
                chat_id,
                &format!(
                    "Leave *{}*? You'll stop receiving updates.",
                    session.project_name
                ),
                Some(markup),
            )
            .await?;
        Ok(())
    }

    fn main_menu(&self) -> InlineKeyboardMarkup {
        InlineKeyboardMarkup::two_columns(vec![
            vec![
                ("➕ Create session".to_string(), "create".to_string()),
                ("🔑 Join session".to_string(), "join".to_string()),
            ],
            vec![
                ("📊 Status".to_string(), "status".to_string()),
                ("📈 Progress".to_string(), "progress".to_string()),
                ("📋 Plans".to_string(), "plans".to_string()),
            ],
            vec![
                ("👥 Members".to_string(), "members".to_string()),
                ("🚪 Leave".to_string(), "leave".to_string()),
                ("❓ Help".to_string(), "help".to_string()),
            ],
        ])
    }

    // --- Callbacks --------------------------------------------------------

    async fn handle_callback(&self, cq: &CallbackQuery) -> Result<(), AppError> {
        let user = self.ensure_user(&cq.from).await?;
        let data = cq.data.clone().unwrap_or_default();
        let chat_id = cq.message.as_ref().map(|m| m.chat.id).unwrap_or(cq.from.id);
        let result = self.dispatch_callback(&user, chat_id, &data).await;
        if let Err(e) = &result {
            tracing::warn!(error = %e, "callback handler error");
        }
        let _ = self.gateway.answer_callback_query(&cq.id, None).await;
        result
    }

    async fn dispatch_callback(
        &self,
        user: &User,
        chat_id: i64,
        data: &str,
    ) -> Result<(), AppError> {
        match data {
            "menu" => {
                self.set_conv(user.id, chat_id, ConvState::Idle, json!({}))
                    .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "What would you like to do?",
                        Some(self.main_menu()),
                    )
                    .await?;
            }
            "create" => {
                self.set_conv(user.id, chat_id, ConvState::AwaitingProjectName, json!({}))
                    .await?;
                self.gateway
                    .send_message(chat_id, "What's the *project name*?", None)
                    .await?;
            }
            "join" => {
                self.set_conv(user.id, chat_id, ConvState::AwaitingSessionKey, json!({}))
                    .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "Enter your *session key* (e.g. `AB7K-X92P`):",
                        None,
                    )
                    .await?;
            }
            "help" => {
                self.gateway
                    .send_message(chat_id, &text::help_text(), Some(self.main_menu()))
                    .await?;
            }
            "status" | "plans" | "members" | "progress" | "plan" | "complete" | "leave" => {
                self.require_session(user, chat_id, data).await?
            }
            _ => self.dispatch_callback_payload(user, chat_id, data).await?,
        }
        Ok(())
    }

    async fn dispatch_callback_payload(
        &self,
        user: &User,
        chat_id: i64,
        data: &str,
    ) -> Result<(), AppError> {
        if let Some(sid) = data.strip_prefix(PREFIX_STATUS) {
            if let Some(session) = self.resolve_session(user, sid).await? {
                self.send_status(user, chat_id, &session).await?;
            }
            return Ok(());
        }
        if let Some(sid) = data.strip_prefix(PREFIX_PLANS) {
            if let Some(session) = self.resolve_session(user, sid).await? {
                self.send_plans(user, chat_id, &session).await?;
            }
            return Ok(());
        }
        if let Some(sid) = data.strip_prefix(PREFIX_MEMBERS) {
            if let Some(session) = self.resolve_session(user, sid).await? {
                self.send_members(user, chat_id, &session).await?;
            }
            return Ok(());
        }
        if let Some(sid) = data.strip_prefix(PREFIX_PROGRESS) {
            if let Some(session) = self.resolve_session(user, sid).await? {
                self.set_conv(
                    user.id,
                    chat_id,
                    ConvState::AwaitingProgressMessage,
                    json!({"session_id": session.id}),
                )
                .await?;
                self.gateway
                    .send_message(chat_id, "What did you accomplish?", None)
                    .await?;
            }
            return Ok(());
        }
        if let Some(sid) = data.strip_prefix(PREFIX_PLAN_NEW) {
            if let Some(session) = self.resolve_session(user, sid).await? {
                self.set_conv(
                    user.id,
                    chat_id,
                    ConvState::AwaitingPlanTitle,
                    json!({"session_id": session.id}),
                )
                .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "What do you want to accomplish? Send me the *plan title*:",
                        None,
                    )
                    .await?;
            }
            return Ok(());
        }
        if let Some(sid) = data.strip_prefix(PREFIX_COMPLETE_LIST) {
            if let Some(session) = self.resolve_session(user, sid).await? {
                self.send_complete_list(user, chat_id, &session).await?;
            }
            return Ok(());
        }
        if let Some(pid) = data.strip_prefix(PREFIX_COMPLETE) {
            if let Some(pid) = parse_uuid(pid) {
                match self.plans.complete_plan(user.id, pid).await {
                    Ok(plan) => {
                        self.gateway
                            .send_message(chat_id, &format!("✅ Completed: *{}*", plan.title), None)
                            .await?;
                        if let Ok(session) =
                            self.sessions.get_session(user.id, plan.session_id).await
                        {
                            self.send_complete_list(user, chat_id, &session).await?;
                        }
                    }
                    Err(e) => {
                        self.gateway
                            .send_message(chat_id, &format!("❌ {e}"), None)
                            .await?;
                    }
                }
            }
            return Ok(());
        }
        if let Some(sid) = data.strip_prefix(PREFIX_LEAVE) {
            if let Some(session) = self.resolve_session(user, sid).await? {
                self.send_leave_confirm(chat_id, &session).await?;
            }
            return Ok(());
        }
        if let Some(sid) = data.strip_prefix(PREFIX_LEAVE_YES) {
            if let Some(sid) = parse_uuid(sid) {
                match self.sessions.leave_session(user.id, sid).await {
                    Ok(()) => {
                        self.gateway
                            .send_message(chat_id, "You left the session. 👋", None)
                            .await?;
                    }
                    Err(e) => {
                        self.gateway
                            .send_message(chat_id, &format!("Could not leave: {e}"), None)
                            .await?;
                    }
                }
            }
            return Ok(());
        }
        if data.strip_prefix(PREFIX_LEAVE_NO).is_some() {
            self.gateway
                .send_message(chat_id, "OK, staying. 😊", None)
                .await?;
            return Ok(());
        }
        Ok(())
    }

    // --- Flows ------------------------------------------------------------

    async fn continue_flow(
        &self,
        user: &User,
        chat_id: i64,
        state: ConvState,
        text: &str,
    ) -> Result<(), AppError> {
        let payload = self.get_payload(user.id).await?;
        match state {
            ConvState::AwaitingProjectName => {
                if text.trim().is_empty() {
                    self.gateway
                        .send_message(chat_id, "Please send a *project name*:", None)
                        .await?;
                    return Ok(());
                }
                self.set_conv(
                    user.id,
                    chat_id,
                    ConvState::AwaitingProjectDescription,
                    json!({"project_name": text.trim()}),
                )
                .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "Great. Now a short *description* (or send `-` to skip):",
                        None,
                    )
                    .await?;
            }
            ConvState::AwaitingProjectDescription => {
                let desc = if text.trim() == "-" {
                    String::new()
                } else {
                    text.trim().to_string()
                };
                let name = payload_get(&payload, "project_name").unwrap_or_default();
                self.set_conv(
                    user.id,
                    chat_id,
                    ConvState::AwaitingProjectDeadline,
                    json!({"project_name": name, "project_description": desc}),
                )
                .await?;
                self.gateway
                    .send_message(
                        chat_id,
                        "When is the *deadline*? (format: `YYYY-MM-DD`)",
                        None,
                    )
                    .await?;
            }
            ConvState::AwaitingProjectDeadline => match parse_deadline(text) {
                Some(deadline) => {
                    let name = payload_get(&payload, "project_name").unwrap_or_default();
                    let desc = payload_get(&payload, "project_description").unwrap_or_default();
                    self.set_conv(
                        user.id,
                        chat_id,
                        ConvState::AwaitingInitialPlans,
                        json!({
                            "project_name": name,
                            "project_description": desc,
                            "deadline": deadline.to_rfc3339(),
                            "plans": []
                        }),
                    )
                    .await?;
                    self.gateway
                        .send_message(
                            chat_id,
                            "Want to add *initial plans*? Send them one per line, then `/done` (or just send `/done` to skip).",
                            None,
                        )
                        .await?;
                }
                None => {
                    self.gateway
                        .send_message(chat_id, "That doesn't look like a valid date. Use `YYYY-MM-DD`, e.g. `2026-09-30`.", None)
                        .await?;
                }
            },
            ConvState::AwaitingInitialPlans => {
                if text.trim() == "/done" || text.trim().is_empty() {
                    let name = payload_get(&payload, "project_name").unwrap_or_default();
                    let desc = payload_get(&payload, "project_description").unwrap_or_default();
                    let deadline_raw = payload_get(&payload, "deadline").unwrap_or_default();
                    let deadline = parse_deadline(&deadline_raw)
                        .ok_or_else(|| AppError::Internal("missing deadline".into()))?;
                    let plans = payload_get_array(&payload, "plans");
                    let session = self
                        .sessions
                        .create_session(
                            user,
                            name,
                            if desc.is_empty() { None } else { Some(desc) },
                            deadline,
                            plans,
                        )
                        .await?;
                    self.set_conv(user.id, chat_id, ConvState::Idle, json!({}))
                        .await?;
                    self.gateway
                        .send_message(chat_id, &text::created_confirmation(&session), None)
                        .await?;
                } else {
                    let mut plans = payload_get_array(&payload, "plans");
                    for line in text.lines() {
                        let l = line.trim();
                        if !l.is_empty() && l != "/done" {
                            plans.push(l.to_string());
                        }
                    }
                    let name = payload_get(&payload, "project_name").unwrap_or_default();
                    let desc = payload_get(&payload, "project_description").unwrap_or_default();
                    let deadline = payload_get(&payload, "deadline").unwrap_or_default();
                    self.set_conv(
                        user.id,
                        chat_id,
                        ConvState::AwaitingInitialPlans,
                        json!({
                            "project_name": name,
                            "project_description": desc,
                            "deadline": deadline,
                            "plans": plans
                        }),
                    )
                    .await?;
                    self.gateway
                        .send_message(
                            chat_id,
                            "Got it. Add more, or send `/done` to finish.",
                            None,
                        )
                        .await?;
                }
            }
            ConvState::AwaitingSessionKey => match self.sessions.join_session(user, text).await {
                Ok(session) => {
                    self.set_conv(user.id, chat_id, ConvState::Idle, json!({}))
                        .await?;
                    self.gateway
                        .send_message(
                            chat_id,
                            &text::joined_confirmation(&session),
                            Some(self.main_menu()),
                        )
                        .await?;
                }
                Err(e) => {
                    self.gateway
                        .send_message(
                            chat_id,
                            &format!("❌ {e}\nTry again with your session key:"),
                            None,
                        )
                        .await?;
                }
            },
            ConvState::AwaitingProgressMessage => {
                if let Some(sid) = payload_get_uuid(&payload, "session_id") {
                    match self.progress.submit(user.id, sid, text).await {
                        Ok(update) => {
                            self.set_conv(user.id, chat_id, ConvState::Idle, json!({}))
                                .await?;
                            self.gateway
                                .send_message(
                                    chat_id,
                                    &format!("📈 Progress update recorded:\n\n{}", update.message),
                                    None,
                                )
                                .await?;
                        }
                        Err(e) => {
                            self.gateway
                                .send_message(chat_id, &format!("❌ {e}"), None)
                                .await?;
                        }
                    }
                }
            }
            ConvState::AwaitingPlanTitle => {
                if let Some(sid) = payload_get_uuid(&payload, "session_id") {
                    match self.plans.create_plan(user.id, sid, text, None).await {
                        Ok(plan) => {
                            self.set_conv(user.id, chat_id, ConvState::Idle, json!({}))
                                .await?;
                            self.gateway
                                .send_message(
                                    chat_id,
                                    &format!("📌 Plan added: *{}*", plan.title),
                                    None,
                                )
                                .await?;
                        }
                        Err(e) => {
                            self.gateway
                                .send_message(chat_id, &format!("❌ {e}"), None)
                                .await?;
                        }
                    }
                }
            }
            ConvState::Idle => {}
        }
        Ok(())
    }

    // --- Presentation helpers --------------------------------------------

    async fn send_chooser(
        &self,
        chat_id: i64,
        prompt: &str,
        sessions: &[Session],
        prefix: &str,
    ) -> Result<(), AppError> {
        let buttons = sessions
            .iter()
            .map(|s| (s.project_name.clone(), format!("{prefix}{}", s.id)))
            .collect();
        let markup = InlineKeyboardMarkup::single_row(buttons);
        self.gateway
            .send_message(chat_id, prompt, Some(markup))
            .await?;
        Ok(())
    }

    /// Resolve a session callback to the session object, enforcing membership
    /// (only sessions the actor belongs to are returned by the service).
    async fn resolve_session(&self, user: &User, sid: &str) -> Result<Option<Session>, AppError> {
        let Some(sid) = parse_uuid(sid) else {
            return Ok(None);
        };
        let list = self.sessions.list_sessions(user.id).await?;
        Ok(list.into_iter().find(|s| s.id == sid))
    }

    // --- Conversation store -----------------------------------------------

    async fn get_conv(&self, user_id: Uuid) -> Result<Option<crate::repo::Conversation>, AppError> {
        let mut conn = self.db.acquire().await?;
        Ok(self.conversations.get(&mut conn, user_id).await?)
    }

    async fn get_payload(&self, user_id: Uuid) -> Result<Value, AppError> {
        Ok(self
            .get_conv(user_id)
            .await?
            .map(|c| c.payload)
            .unwrap_or_else(|| json!({})))
    }

    async fn set_conv(
        &self,
        user_id: Uuid,
        chat_id: i64,
        state: ConvState,
        payload: Value,
    ) -> Result<(), AppError> {
        let mut conn = self.db.acquire().await?;
        self.conversations
            .set(&mut conn, user_id, chat_id, state.as_str(), payload)
            .await?;
        Ok(())
    }
}

// --- Helpers ---------------------------------------------------------------

fn parse_uuid(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok()
}

fn payload_get(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn payload_get_array(payload: &Value, key: &str) -> Vec<String> {
    payload
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn payload_get_uuid(payload: &Value, key: &str) -> Option<Uuid> {
    payload_get(payload, key).and_then(|s| Uuid::parse_str(&s).ok())
}

fn parse_deadline(s: &str) -> Option<DateTime<Utc>> {
    for fmt in ["%Y-%m-%d", "%d.%m.%Y", "%Y/%m/%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(s.trim(), fmt) {
            return d.and_hms_opt(23, 59, 59).map(|n| n.and_utc());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_parsing() {
        assert!(parse_deadline("2026-09-30").is_some());
        assert!(parse_deadline("30.09.2026").is_some());
        assert!(parse_deadline("not-a-date").is_none());
    }

    #[test]
    fn payload_accessors() {
        let v = json!({
            "a": "x",
            "list": ["one", "two"],
            "sid": "550e8400-e29b-41d4-a716-446655440000"
        });
        assert_eq!(payload_get(&v, "a").as_deref(), Some("x"));
        assert_eq!(
            payload_get_array(&v, "list"),
            vec!["one".to_string(), "two".to_string()]
        );
        assert!(payload_get_uuid(&v, "sid").is_some());
        assert_eq!(payload_get_array(&v, "missing"), Vec::<String>::new());
    }
}
