//! Builds the text for backend-triggered event notifications.

use crate::domain::{DomainEvent, Session};
use crate::text;

pub struct NotifyCtx<'a> {
    pub event: &'a DomainEvent,
    pub session: &'a Session,
    pub actor_name: Option<String>,
    pub plan_title: Option<String>,
    pub progress: u8,
    pub remaining_active: i64,
    pub days_to_deadline: i64,
    pub update_message: Option<String>,
}

pub fn event_text(ctx: &NotifyCtx<'_>) -> String {
    let session = ctx.session;
    match ctx.event {
        DomainEvent::SessionCreated { .. } => String::new(), // creator already sees confirmation
        DomainEvent::MemberJoinedSession { .. } => {
            format!(
                "👋 *{}* joined the session.\n\n📁 {}\nProgress: *{}%*\n{}",
                ctx.actor_name.as_deref().unwrap_or("Someone"),
                session.project_name,
                ctx.progress,
                text::bar(ctx.progress),
            )
        }
        DomainEvent::PlanCreated { .. } => {
            format!(
                "📌 New plan: *{}*\n\n📁 {}\nProgress: *{}%*\n{}",
                ctx.plan_title.as_deref().unwrap_or("?"),
                session.project_name,
                ctx.progress,
                text::bar(ctx.progress),
            )
        }
        DomainEvent::PlanStarted { .. } => {
            format!(
                "▶️ *{}* started: {}\n\n📁 {}",
                ctx.actor_name.as_deref().unwrap_or("Someone"),
                ctx.plan_title.as_deref().unwrap_or("?"),
                session.project_name,
            )
        }
        DomainEvent::PlanCompleted { .. } => {
            format!(
                "✅ *{}* completed: {}\n\n📁 {}\nProgress: *{}%*\n{}",
                ctx.actor_name.as_deref().unwrap_or("Someone"),
                ctx.plan_title.as_deref().unwrap_or("?"),
                session.project_name,
                ctx.progress,
                text::bar(ctx.progress),
            )
        }
        DomainEvent::PlanCancelled { .. } => {
            format!(
                "❌ *{}* cancelled: {}\n\n📁 {}",
                ctx.actor_name.as_deref().unwrap_or("Someone"),
                ctx.plan_title.as_deref().unwrap_or("?"),
                session.project_name,
            )
        }
        DomainEvent::ProgressUpdated { .. } => {
            format!(
                "📈 *Progress Update*\n\n*{}*:\n{}\n\n📁 {}\nProgress: *{}%*\n{}\n🗓 Deadline: {}",
                ctx.actor_name.as_deref().unwrap_or("Someone"),
                ctx.update_message.as_deref().unwrap_or("…"),
                session.project_name,
                ctx.progress,
                text::bar(ctx.progress),
                session.deadline.format("%Y-%m-%d"),
            )
        }
        DomainEvent::DeadlineApproaching { .. } => {
            format!(
                "⏰ *Deadline Reminder*\n\nYour project \"{}\" is due in *{} day(s)*.\n\nCurrent progress: *{}%*\nRemaining plans: {}",
                session.project_name,
                ctx.days_to_deadline,
                ctx.progress,
                ctx.remaining_active,
            )
        }
        DomainEvent::DeadlineReached { .. } => {
            format!(
                "⏰ *Deadline Reached*\n\nYour project \"{}\" deadline has arrived.\nCurrent progress: *{}%*",
                session.project_name,
                ctx.progress,
            )
        }
    }
}
