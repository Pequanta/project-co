//! Pure text rendering shared by the Telegram adapter (interactive replies)
//! and the notification service (backend-triggered broadcasts).

use chrono::{DateTime, Utc};

use crate::domain::{progress_calc::progress_bar, Plan, PlanStatus, Session, SessionMode};

pub fn bar(pct: u8) -> String {
    progress_bar(pct, 10)
}

/// Whole days from `now` until `deadline` (0 if passed).
pub fn days_remaining(deadline: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    let secs = (deadline - now).num_seconds().max(0);
    (secs + 86_399) / 86_400
}

pub fn welcome_text(name: &str) -> String {
    format!(
        "👋 Hi {name}!\n\nI help small teams track progress in a shared collaboration session.\n\nUse the buttons below, or type /help for all commands."
    )
}

/// Prompt shown during /create when choosing the session mode.
pub fn mode_prompt() -> &'static str {
    "How should progress be tracked?\n\n\
     • `study` — everyone completes every task (each member has their own progress)\n\
     • `collab` — split the work; each task is done by one member\n\n\
     Reply `study` or `collab`."
}

/// Short human label for a mode.
pub fn mode_label(mode: SessionMode) -> &'static str {
    match mode {
        SessionMode::Study => "📚 Study (everyone does every task)",
        SessionMode::Collaboration => "🤝 Collaboration (split the work)",
    }
}

pub fn help_text() -> String {
    "Here's what I can do:\n\n\
     /create — start a new collaboration session\n\
     /join — join a session with a key\n\
     /sessions — open one of your existing sessions\n\
     /status — session dashboard (deadline, progress, plans, activity)\n\
     /progress — submit a progress update\n\
     /plan — add a new plan\n\
     /plans — view all plans\n\
     /complete — mark a plan as completed\n\
     /members — who's in the session\n\
     /leave — leave a session\n\
     /help — this message\n\n\
     Tip: you can also just type an update as a plain message when you're in exactly one session."
        .to_string()
}

pub fn created_confirmation(session: &Session) -> String {
    format!(
        "✅ Your collaboration session has been created.\n\n\
         Project: {}\n\
         Mode: {}\n\
         Deadline: {}\n\n\
         Session key:\n`{}`\n\n\
         Share this key with the people who should join your collaboration.",
        session.project_name,
        mode_label(session.mode),
        session.deadline.format("%Y-%m-%d"),
        crate::domain::format_key(&session.session_key),
    )
}

pub fn joined_confirmation(session: &Session) -> String {
    format!(
        "🎉 You joined *{}*!\n\n\
         📁 {}\n🗓 Deadline: {}\n\nUse /status to see the dashboard.",
        session.project_name,
        session.project_description.as_deref().unwrap_or_default(),
        session.deadline.format("%Y-%m-%d"),
    )
}

/// Grouped plan listing: Completed / In Progress / Not Started.
pub fn plans_text(project_name: &str, plans: &[Plan]) -> String {
    let (mut done, mut in_progress, mut planned, mut cancelled): (Vec<_>, Vec<_>, Vec<_>, Vec<_>) =
        (vec![], vec![], vec![], vec![]);
    for p in plans {
        match p.status {
            PlanStatus::Completed => done.push(p),
            PlanStatus::InProgress => in_progress.push(p),
            PlanStatus::Planned => planned.push(p),
            PlanStatus::Cancelled => cancelled.push(p),
        }
    }

    let mut out = format!("📋 *{}* — Plans\n", project_name);
    if plans.is_empty() {
        out.push_str("\n_No plans yet. Use /plan to add one._\n");
        return out;
    }
    if !done.is_empty() {
        out.push_str("\n✅ *Completed*\n");
        for p in &done {
            out.push_str(&format!("  • {}\n", p.title));
        }
    }
    if !in_progress.is_empty() {
        out.push_str("\n🔄 *In progress*\n");
        for p in &in_progress {
            out.push_str(&format!("  • {}\n", p.title));
        }
    }
    if !planned.is_empty() {
        out.push_str("\n⬜ *Not started*\n");
        for p in &planned {
            out.push_str(&format!("  • {}\n", p.title));
        }
    }
    if !cancelled.is_empty() {
        out.push_str("\n❌ *Cancelled*\n");
        for p in &cancelled {
            out.push_str(&format!("  • {}\n", p.title));
        }
    }
    out
}

/// The `/status` dashboard.
#[allow(clippy::too_many_arguments)]
pub fn dashboard_text(
    project_name: &str,
    project_description: Option<&str>,
    deadline: DateTime<Utc>,
    now: DateTime<Utc>,
    mode: SessionMode,
    pct: u8,
    completed: i64,
    in_progress: i64,
    remaining: i64,
    cancelled: i64,
    member_names: &[String],
    member_progress: &[(String, u8)], // (member, percent)
    recent: &[(String, String)],      // (author, message)
) -> String {
    let days = days_remaining(deadline, now);
    let days_line = if days == 0 {
        "⚠️ *Deadline reached*".to_string()
    } else {
        format!(
            "🗓 Deadline: {} — *{} days remaining*",
            deadline.format("%Y-%m-%d"),
            days
        )
    };

    let overall_label = match mode {
        SessionMode::Study => "Overall (avg)",
        SessionMode::Collaboration => "Progress",
    };
    let mut out = format!(
        "📊 *{project_name}*\n{desc}\n\n{mode_line}\n{days_line}\n\n\
         {overall_label}: *{pct}%*\n{bar}\n",
        desc = project_description.unwrap_or("").trim(),
        mode_line = mode_label(mode),
        bar = bar(pct),
    );

    // Per-member progress: the headline in study mode, contribution shares in
    // collaboration mode.
    if !member_progress.is_empty() {
        let heading = match mode {
            SessionMode::Study => "\n🧑‍🤝‍🧑 Each member",
            SessionMode::Collaboration => "\n🧑‍🤝‍🧑 Contributions",
        };
        out.push_str(heading);
        out.push('\n');
        for (name, p) in member_progress {
            out.push_str(&format!("{} {p}% — {name}\n", bar(*p)));
        }
    }

    out.push_str(&format!(
        "\n📋 Plans\n\
         ✅ Completed: {completed}\n\
         🔄 In progress: {in_progress}\n\
         ⬜ Remaining: {remaining}\n",
    ));
    if cancelled > 0 {
        out.push_str(&format!("❌ Cancelled: {cancelled}\n"));
    }
    if !member_names.is_empty() {
        out.push_str(&format!("\n👥 Members\n  {}\n", member_names.join("\n  ")));
    }
    if !recent.is_empty() {
        out.push_str("\n🕒 Recent activity\n");
        for (author, msg) in recent {
            out.push_str(&format!("👤 {author} — {msg}\n"));
        }
    }
    out
}
