use acp_thread::{
    AcpThread, AgentThreadEntry, AssistantMessageChunk, ThreadStatus, ToolCall, ToolCallStatus,
};
use agent_client_protocol::v2::schema as acp_v2;
use chrono::{DateTime, Local, TimeZone as _, Utc};
use collections::{HashMap, HashSet};
use db::kvp::KeyValueStore;
use gpui::{AnyElement, App, FontWeight, Hsla, MouseButton, SharedString, hsla, px, relative};
use project::AgentId;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use ui::{Color, Icon, IconName, IconSize, Label, LabelSize, LineHeightStyle, Tooltip, prelude::*};
use util::ResultExt as _;

static HIDE_THINKING_AND_TOOLS: AtomicBool = AtomicBool::new(false);
const TURN_TIMINGS_KVP_NAMESPACE: &str = "custom_turn_timings";

gpui::actions!(
    agent,
    [
        /// Toggles hiding thinking blocks and completed tool calls in the conversation view.
        ToggleHideThinkingAndTools,
    ]
);

pub(crate) fn hide_thinking_and_tools_enabled() -> bool {
    HIDE_THINKING_AND_TOOLS.load(Ordering::Relaxed)
}

pub(crate) fn toggle_hide_thinking_and_tools() -> bool {
    let previous = HIDE_THINKING_AND_TOOLS.fetch_xor(true, Ordering::Relaxed);
    !previous
}

pub(crate) fn should_hide_tool_call(tool_call: &ToolCall, hide_enabled: bool) -> bool {
    if !hide_enabled {
        return false;
    }
    matches!(
        tool_call.status(),
        ToolCallStatus::Completed | ToolCallStatus::Canceled
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TurnTimingRecord {
    pub user_sent_at_ms: Option<i64>,
    pub ai_started_at_ms: Option<i64>,
    pub ai_finished_at_ms: Option<i64>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct AssistantTimingDisplay {
    pub started_label: Option<SharedString>,
    pub duration_label: Option<SharedString>,
    pub is_running: bool,
}

pub(crate) struct TurnTimingTracker {
    session_key: String,
    records: HashMap<usize, TurnTimingRecord>,
    known_user_entries: HashSet<usize>,
    active_run: Option<(usize, Instant)>,
}

impl TurnTimingTracker {
    pub(crate) fn new(session_id: &acp_v2::SessionId, thread: &AcpThread, cx: &App) -> Self {
        let session_key = session_id.0.to_string();
        let records = load_turn_timings(&session_key, cx);
        let mut known_user_entries = HashSet::default();
        for (entry_ix, entry) in thread.entries().iter().enumerate() {
            if thread.is_user_authored_scroll_target(entry) {
                known_user_entries.insert(entry_ix);
            }
        }

        let mut tracker = Self {
            session_key,
            records,
            known_user_entries,
            active_run: None,
        };

        if thread.status() == ThreadStatus::Generating
            && let Some(last_user_ix) = last_user_entry_index(thread)
        {
            let started_instant = thread
                .activity_started_at()
                .unwrap_or_else(Instant::now);
            tracker.active_run = Some((last_user_ix, started_instant));
            let record = tracker.records.entry(last_user_ix).or_default();
            if record.ai_started_at_ms.is_none() {
                let elapsed_ms = started_instant.elapsed().as_millis() as i64;
                record.ai_started_at_ms = Some(Utc::now().timestamp_millis() - elapsed_ms);
            }
        }

        tracker
    }

    /// Synchronizes per-turn send time, AI start time, and execution duration with
    /// the latest thread state. Returns `true` while a turn is actively running.
    pub(crate) fn sync_with_thread(&mut self, thread: &gpui::Entity<AcpThread>, cx: &mut App) -> bool {
        let now_ms = Utc::now().timestamp_millis();
        let mut dirty = false;
        let (user_entries, entry_count, is_generating, activity_started_at) = {
            let thread = thread.read(cx);
            let user_entries: Vec<usize> = thread
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(entry_ix, entry)| {
                    thread
                        .is_user_authored_scroll_target(entry)
                        .then_some(entry_ix)
                })
                .collect();
            (
                user_entries,
                thread.entries().len(),
                thread.status() == ThreadStatus::Generating,
                thread.activity_started_at(),
            )
        };
        let last_user_ix = user_entries.last().copied();

        for entry_ix in user_entries {
            if self.known_user_entries.insert(entry_ix) {
                let record = self.records.entry(entry_ix).or_default();
                if record.user_sent_at_ms.is_none() {
                    record.user_sent_at_ms = Some(now_ms);
                    dirty = true;
                }
            }
        }

        // Prune records if entries were truncated (e.g. rewind/edit).
        let before_len = self.records.len();
        self.records.retain(|&ix, _| ix < entry_count);
        self.known_user_entries.retain(|&ix| ix < entry_count);
        if self.records.len() != before_len {
            dirty = true;
        }

        if is_generating {
            if let Some(target_user_ix) = last_user_ix {
                let needs_new_run = match self.active_run {
                    Some((active_ix, _)) => active_ix != target_user_ix,
                    None => true,
                };
                if needs_new_run {
                    if let Some((prev_ix, prev_started)) = self.active_run.take() {
                        let prev_record = self.records.entry(prev_ix).or_default();
                        prev_record.ai_finished_at_ms = Some(now_ms);
                        prev_record.duration_ms = Some(prev_started.elapsed().as_millis() as u64);
                    }
                    let started_instant = activity_started_at.unwrap_or_else(Instant::now);
                    self.active_run = Some((target_user_ix, started_instant));
                    let record = self.records.entry(target_user_ix).or_default();
                    if record.user_sent_at_ms.is_none() {
                        record.user_sent_at_ms = Some(now_ms);
                    }
                    let elapsed_ms = started_instant.elapsed().as_millis() as i64;
                    record.ai_started_at_ms = Some(now_ms - elapsed_ms);
                    record.ai_finished_at_ms = None;
                    record.duration_ms = None;
                    dirty = true;
                }
            }
        } else if let Some((run_ix, started_at)) = self.active_run.take() {
            let record = self.records.entry(run_ix).or_default();
            record.ai_finished_at_ms = Some(now_ms);
            record.duration_ms = Some(started_at.elapsed().as_millis() as u64);
            dirty = true;
        }

        if dirty {
            save_turn_timings(&self.session_key, &self.records, cx);
        }

        self.active_run.is_some()
    }

    pub(crate) fn user_sent_time_label(&self, user_entry_ix: usize) -> Option<SharedString> {
        let timestamp_ms = self.records.get(&user_entry_ix)?.user_sent_at_ms?;
        format_local_timestamp(timestamp_ms, Local::now()).map(SharedString::from)
    }

    pub(crate) fn assistant_timing_for_entry(
        &self,
        entries: &[AgentThreadEntry],
        assistant_entry_ix: usize,
    ) -> Option<AssistantTimingDisplay> {
        let user_entry_ix = enclosing_user_entry_index(entries, assistant_entry_ix)?;
        self.assistant_timing_for_turn(user_entry_ix)
    }

    pub(crate) fn assistant_timing_for_turn(
        &self,
        user_entry_ix: usize,
    ) -> Option<AssistantTimingDisplay> {
        let record = self.records.get(&user_entry_ix);
        let is_running = self
            .active_run
            .is_some_and(|(active_ix, _)| active_ix == user_entry_ix);

        let started_label = record
            .and_then(|r| r.ai_started_at_ms)
            .and_then(|ms| format_local_timestamp(ms, Local::now()))
            .map(SharedString::from);

        let duration_label = if let Some((active_ix, started_instant)) = self.active_run
            && active_ix == user_entry_ix
        {
            Some(SharedString::from(format_duration(started_instant.elapsed())))
        } else {
            record
                .and_then(|r| r.duration_ms)
                .map(|ms| SharedString::from(format_duration(Duration::from_millis(ms))))
        };

        if started_label.is_none() && duration_label.is_none() {
            return None;
        }

        Some(AssistantTimingDisplay {
            started_label,
            duration_label,
            is_running,
        })
    }

    pub(crate) fn current_running_duration(&self) -> Option<Duration> {
        self.active_run.map(|(_, started)| started.elapsed())
    }

    pub(crate) fn total_session_duration(&self) -> Option<Duration> {
        let active_ix = self.active_run.map(|(ix, _)| ix);
        let mut total_ms: u64 = 0;
        let mut has_any = false;

        for (&entry_ix, record) in &self.records {
            if Some(entry_ix) == active_ix {
                continue;
            }
            if let Some(duration_ms) = record.duration_ms {
                total_ms = total_ms.saturating_add(duration_ms);
                has_any = true;
            }
        }

        if let Some((_, started)) = self.active_run {
            total_ms = total_ms.saturating_add(started.elapsed().as_millis() as u64);
            has_any = true;
        }

        has_any.then(|| Duration::from_millis(total_ms))
    }
}

fn last_user_entry_index(thread: &AcpThread) -> Option<usize> {
    thread
        .entries()
        .iter()
        .rposition(|entry| thread.is_user_authored_scroll_target(entry))
}

fn enclosing_user_entry_index(
    entries: &[AgentThreadEntry],
    entry_ix: usize,
) -> Option<usize> {
    entries
        .get(..=entry_ix)?
        .iter()
        .rposition(|entry| matches!(entry, AgentThreadEntry::UserMessage(msg) if !msg.content.source_blocks().is_empty()))
}

fn load_turn_timings(session_key: &str, cx: &App) -> HashMap<usize, TurnTimingRecord> {
    if !cx.has_global::<db::AppDatabase>() {
        return HashMap::default();
    }
    KeyValueStore::global(cx)
        .scoped(TURN_TIMINGS_KVP_NAMESPACE)
        .read(session_key)
        .log_err()
        .flatten()
        .and_then(|json| serde_json::from_str::<HashMap<usize, TurnTimingRecord>>(&json).ok())
        .unwrap_or_default()
}

fn save_turn_timings(
    session_key: &str,
    records: &HashMap<usize, TurnTimingRecord>,
    cx: &mut App,
) {
    if !cx.has_global::<db::AppDatabase>() {
        return;
    }
    let Ok(serialized) = serde_json::to_string(records) else {
        return;
    };
    let key = session_key.to_string();
    let kvp = KeyValueStore::global(cx);
    cx.background_spawn(async move {
        kvp.scoped(TURN_TIMINGS_KVP_NAMESPACE)
            .write(key, serialized)
            .await
            .log_err();
    })
    .detach();
}

pub(crate) fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    if total_secs == 0 {
        let tenths = duration.subsec_millis() / 100;
        if tenths == 0 {
            "<1s".to_string()
        } else {
            format!("0.{tenths}s")
        }
    } else if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{mins}m {secs:02}s")
    } else {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        format!("{hours}h {mins:02}m")
    }
}

fn format_local_timestamp(timestamp_ms: i64, now_local: DateTime<Local>) -> Option<String> {
    let utc = Utc.timestamp_millis_opt(timestamp_ms).single()?;
    let local = utc.with_timezone(&Local);
    if local.date_naive() == now_local.date_naive() {
        Some(local.format("%H:%M:%S").to_string())
    } else {
        Some(local.format("%m-%d %H:%M").to_string())
    }
}

pub(crate) fn count_hideable_steps(entries: &[AgentThreadEntry], cx: &App) -> usize {
    let mut count = 0;
    for entry in entries {
        match entry {
            AgentThreadEntry::AssistantMessage(message) => {
                for chunk in &message.chunks {
                    if let AssistantMessageChunk::Thought { block, .. } = chunk
                        && block.visible_content(cx)
                    {
                        count += 1;
                    }
                }
            }
            AgentThreadEntry::ToolCall(tool_call) => {
                if matches!(
                    tool_call.status(),
                    ToolCallStatus::Completed | ToolCallStatus::Canceled
                ) {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

fn assistant_message_has_visible_reply(chunks: &[AssistantMessageChunk], cx: &App) -> bool {
    chunks.iter().any(|chunk| {
        matches!(chunk, AssistantMessageChunk::Message { block, .. } if block.visible_content(cx))
    })
}

/// Returns true when the assistant message at `entry_ix` starts a visible AI reply
/// (either the first visible reply after a user prompt, or a reply resuming after
/// visible tool calls).
pub(crate) fn should_show_assistant_reply_header(
    entries: &[AgentThreadEntry],
    entry_ix: usize,
    hide_steps: bool,
    cx: &App,
) -> bool {
    let Some(AgentThreadEntry::AssistantMessage(current)) = entries.get(entry_ix) else {
        return false;
    };
    if !assistant_message_has_visible_reply(&current.chunks, cx) {
        return false;
    }

    for entry in entries[..entry_ix].iter().rev() {
        match entry {
            AgentThreadEntry::UserMessage(message) => {
                if !message.content.source_blocks().is_empty() {
                    return true;
                }
            }
            AgentThreadEntry::ToolCall(tool_call) => {
                if !should_hide_tool_call(tool_call, hide_steps) {
                    return true;
                }
            }
            AgentThreadEntry::AssistantMessage(previous) => {
                if assistant_message_has_visible_reply(&previous.chunks, cx) {
                    return false;
                }
            }
            _ => {}
        }
    }

    true
}

fn reply_header_brand_color(agent_id: &str, display_name: &str, cx: &App) -> Hsla {
    let id_lower = agent_id.to_lowercase();
    let name_lower = display_name.to_lowercase();
    let matches_any = |needle: &str| id_lower.contains(needle) || name_lower.contains(needle);
    let is_light = cx.theme().appearance().is_light();

    if matches_any("claude") || matches_any("anthropic") {
        if is_light {
            hsla(15.0 / 360.0, 0.78, 0.42, 1.0)
        } else {
            hsla(15.0 / 360.0, 0.78, 0.62, 1.0)
        }
    } else if matches_any("codex") || matches_any("openai") {
        if is_light {
            hsla(162.0 / 360.0, 0.68, 0.34, 1.0)
        } else {
            hsla(162.0 / 360.0, 0.60, 0.56, 1.0)
        }
    } else if matches_any("codebuddy") {
        if is_light {
            hsla(268.0 / 360.0, 0.68, 0.46, 1.0)
        } else {
            hsla(268.0 / 360.0, 0.76, 0.69, 1.0)
        }
    } else if matches_any("cursor") {
        if is_light {
            hsla(210.0 / 360.0, 0.80, 0.42, 1.0)
        } else {
            hsla(210.0 / 360.0, 0.80, 0.65, 1.0)
        }
    } else if matches_any("gemini") {
        if is_light {
            hsla(224.0 / 360.0, 0.78, 0.44, 1.0)
        } else {
            hsla(224.0 / 360.0, 0.84, 0.67, 1.0)
        }
    } else if matches_any("opencode") || matches_any("open-code") {
        if is_light {
            hsla(38.0 / 360.0, 0.82, 0.38, 1.0)
        } else {
            hsla(38.0 / 360.0, 0.82, 0.62, 1.0)
        }
    } else {
        cx.theme().colors().text_accent
    }
}

pub(crate) fn render_assistant_reply_header(
    agent_id: &AgentId,
    agent_display_name: &SharedString,
    agent_icon: IconName,
    agent_icon_from_external_svg: Option<&SharedString>,
    timing: Option<AssistantTimingDisplay>,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let brand_color = reply_header_brand_color(agent_id.as_ref(), agent_display_name.as_ref(), cx);
    let builtin_icon = agent_servers::builtin_icon_for_agent_id(agent_id.as_ref());

    let icon_element = if let Some(builtin) = builtin_icon {
        Icon::new(builtin)
            .size(IconSize::XSmall)
            .color(Color::Custom(brand_color))
            .into_any_element()
    } else if let Some(svg) = agent_icon_from_external_svg {
        Icon::from_external_svg(svg.clone())
            .size(IconSize::XSmall)
            .color(Color::Custom(brand_color))
            .into_any_element()
    } else {
        let fallback_icon = if agent_icon == IconName::Terminal {
            IconName::Sparkle
        } else {
            agent_icon
        };
        Icon::new(fallback_icon)
            .size(IconSize::XSmall)
            .color(Color::Custom(brand_color))
            .into_any_element()
    };

    let display_label = if agent_display_name.trim().is_empty() {
        agent_id.0.clone()
    } else {
        agent_display_name.clone()
    };

    let timing_element = timing.map(|timing| {
        let mut parts: Vec<String> = Vec::new();
        if let Some(started) = timing.started_label {
            parts.push(started.to_string());
        }
        if let Some(duration) = timing.duration_label {
            if timing.is_running {
                parts.push(format!("{duration}…"));
            } else {
                parts.push(duration.to_string());
            }
        }
        let text = parts.join(" · ");
        let text_color = if timing.is_running {
            Color::Accent
        } else {
            Color::Muted
        };

        Label::new(text)
            .size(LabelSize::XSmall)
            .weight(if timing.is_running {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .line_height_style(LineHeightStyle::UiLabel)
            .color(text_color)
            .flex_none()
            .into_any_element()
    });

    h_flex()
        .w_full()
        .items_center()
        .gap_1p5()
        .pb_1p5()
        .child(
            h_flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(18.))
                .rounded(px(5.))
                .bg(brand_color.opacity(0.14))
                .child(icon_element),
        )
        .child(
            Label::new(display_label)
                .size(LabelSize::Small)
                .weight(FontWeight::SEMIBOLD)
                .line_height_style(LineHeightStyle::UiLabel)
                .color(Color::Muted),
        )
        .child(
            div()
                .flex_1()
                .h(px(1.))
                .mx_1()
                .bg(colors.border_variant.opacity(0.45)),
        )
        .when_some(timing_element, |this, element| this.child(element))
        .into_any_element()
}

pub(crate) fn render_conversation_top_bar(
    total_session_duration: Option<Duration>,
    running_duration: Option<Duration>,
    turn_count: usize,
    hide_steps_toggle: Option<AnyElement>,
    cx: &App,
) -> Option<AnyElement> {
    if total_session_duration.is_none() && hide_steps_toggle.is_none() {
        return None;
    }

    let colors = cx.theme().colors();
    let session_stats = total_session_duration.map(|total| {
        let is_running = running_duration.is_some();
        let text = if let Some(running) = running_duration {
            format!(
                "Running {} · Session {}",
                format_duration(running),
                format_duration(total)
            )
        } else if turn_count > 0 {
            format!(
                "Session {} · {} {}",
                format_duration(total),
                turn_count,
                if turn_count == 1 { "turn" } else { "turns" }
            )
        } else {
            format!("Session {}", format_duration(total))
        };
        let stat_color = if is_running {
            Color::Accent
        } else {
            Color::Muted
        };

        h_flex()
            .items_center()
            .gap_1()
            .child(
                Icon::new(IconName::HistoryRerun)
                    .size(IconSize::XSmall)
                    .color(stat_color),
            )
            .child(
                Label::new(text)
                    .size(LabelSize::XSmall)
                    .weight(if is_running {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .line_height_style(LineHeightStyle::UiLabel)
                    .color(stat_color),
            )
            .into_any_element()
    });

    Some(
        h_flex()
            .w_full()
            .flex_none()
            .px_3()
            .py_1()
            .gap_2()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(colors.border_variant.opacity(0.5))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .items_center()
                    .when_some(session_stats, |this, stats| this.child(stats)),
            )
            .when_some(hide_steps_toggle, |this, toggle| this.child(toggle))
            .into_any_element(),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConversationTurn {
    pub turn_index: usize,
    pub user_entry_ix: usize,
    pub prompt_preview: SharedString,
    pub reply_preview: Option<SharedString>,
}

fn clean_preview_text(raw: &str, max_chars: usize) -> String {
    let mut cleaned = String::new();
    let mut in_code_fence = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence || trimmed.is_empty() {
            continue;
        }
        let stripped = trimmed
            .trim_start_matches('#')
            .trim_start_matches('>')
            .trim_start_matches(['-', '*', '•'])
            .trim();
        if stripped.is_empty() {
            continue;
        }
        if !cleaned.is_empty() {
            cleaned.push(' ');
        }
        cleaned.push_str(stripped);
        if cleaned.chars().count() > max_chars + 16 {
            break;
        }
    }

    truncate_chars(cleaned.trim(), max_chars)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{collected}…")
    } else {
        collected
    }
}

pub(crate) fn collect_conversation_turns(thread: &AcpThread, cx: &App) -> Vec<ConversationTurn> {
    let entries = thread.entries();
    let mut turns: Vec<ConversationTurn> = Vec::new();
    let mut current_reply_parts: Vec<String> = Vec::new();

    let flush_reply = |turns: &mut [ConversationTurn], reply_parts: &mut Vec<String>| {
        if let Some(last_turn) = turns.last_mut()
            && !reply_parts.is_empty()
        {
            let combined = reply_parts.join("\n");
            let preview = clean_preview_text(&combined, 130);
            if !preview.is_empty() {
                last_turn.reply_preview = Some(preview.into());
            }
        }
        reply_parts.clear();
    };

    for (entry_ix, entry) in entries.iter().enumerate() {
        match entry {
            AgentThreadEntry::UserMessage(message)
                if thread.is_user_authored_scroll_target(entry) =>
            {
                flush_reply(&mut turns, &mut current_reply_parts);
                let raw_prompt = message.content.to_markdown(cx);
                let prompt_preview = clean_preview_text(&raw_prompt, 56);
                let prompt_preview = if prompt_preview.is_empty() {
                    SharedString::from(format!("Prompt #{}", turns.len() + 1))
                } else {
                    prompt_preview.into()
                };
                turns.push(ConversationTurn {
                    turn_index: turns.len(),
                    user_entry_ix: entry_ix,
                    prompt_preview,
                    reply_preview: None,
                });
            }
            AgentThreadEntry::AssistantMessage(message) if !turns.is_empty() => {
                for chunk in &message.chunks {
                    if let AssistantMessageChunk::Message { block, .. } = chunk
                        && block.visible_content(cx)
                    {
                        current_reply_parts.push(block.to_markdown(cx));
                    }
                }
            }
            _ => {}
        }
    }

    flush_reply(&mut turns, &mut current_reply_parts);
    turns
}

pub(crate) fn active_turn_index(turns: &[ConversationTurn], top_entry_ix: usize) -> Option<usize> {
    if turns.is_empty() {
        return None;
    }
    turns
        .iter()
        .rposition(|turn| turn.user_entry_ix <= top_entry_ix)
        .or(Some(0))
}

fn tick_width_for_distance(
    turn_ix: usize,
    hovered_turn: Option<usize>,
    active_turn: Option<usize>,
) -> f32 {
    if let Some(hovered) = hovered_turn {
        match turn_ix.abs_diff(hovered) {
            0 => 20.0,
            1 => 14.0,
            2 => 10.0,
            _ => {
                if active_turn == Some(turn_ix) {
                    11.0
                } else {
                    7.0
                }
            }
        }
    } else if active_turn == Some(turn_ix) {
        15.0
    } else {
        7.0
    }
}

pub(crate) fn render_hide_steps_toggle(
    hide_enabled: bool,
    hideable_steps: usize,
    on_toggle: impl Fn(&mut gpui::Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    if !hide_enabled && hideable_steps == 0 {
        return None;
    }

    let colors = cx.theme().colors();
    let (icon, label_text, tooltip_text) = if hide_enabled {
        (
            IconName::EyeOff,
            if hideable_steps > 0 {
                format!("Steps hidden ({hideable_steps})")
            } else {
                "Steps hidden".to_string()
            },
            "Show thinking blocks and completed tool calls",
        )
    } else {
        (
            IconName::Eye,
            format!("Hide steps ({hideable_steps})"),
            "Hide thinking blocks and completed tool calls to focus on prompts and replies",
        )
    };

    let bg = if hide_enabled {
        colors
            .surface_background
            .blend(colors.text_accent.opacity(0.14))
    } else {
        colors.element_background.opacity(0.6)
    };
    let border = if hide_enabled {
        colors.text_accent.opacity(0.38)
    } else {
        colors.border_variant
    };
    let content_color = if hide_enabled {
        Color::Accent
    } else {
        Color::Muted
    };

    Some(
        h_flex()
            .id("toggle-hide-thinking-and-tools")
            .cursor_pointer()
            .items_center()
            .gap_1()
            .h(px(22.))
            .px_2()
            .rounded_md()
            .bg(bg)
            .border_1()
            .border_color(border)
            .hover(|style| style.bg(colors.element_hover))
            .active(|style| style.bg(colors.element_active))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_toggle(window, cx);
            })
            .tooltip(Tooltip::text(tooltip_text))
            .child(Icon::new(icon).size(IconSize::XSmall).color(content_color))
            .child(
                Label::new(label_text)
                    .size(LabelSize::XSmall)
                    .weight(FontWeight::MEDIUM)
                    .line_height_style(LineHeightStyle::UiLabel)
                    .color(content_color),
            )
            .into_any_element(),
    )
}

/// Renders the left-side vertical tick rail inside its own dedicated flex column
/// so it never overlaps conversation text even at narrow panel widths.
pub(crate) fn render_left_turn_rail(
    turns: &[ConversationTurn],
    active_turn: Option<usize>,
    hovered_turn: Option<usize>,
    on_hover: impl Fn(Option<usize>, &mut gpui::Window, &mut App) + Clone + 'static,
    on_select: impl Fn(usize, &mut gpui::Window, &mut App) + Clone + 'static,
    cx: &App,
) -> Option<AnyElement> {
    if turns.len() < 2 {
        return None;
    }

    let colors = cx.theme().colors();
    let tick_row_height = if turns.len() > 30 { px(7.) } else { px(10.) };
    let on_hover_rail_leave = on_hover.clone();

    Some(
        v_flex()
            .id("conversation-turn-navigator-rail")
            .w(px(28.))
            .flex_none()
            .h_full()
            .pl_1p5()
            .py_2()
            .justify_center()
            .items_start()
            .on_hover(move |is_hovered, window, cx| {
                if !*is_hovered {
                    on_hover_rail_leave(None, window, cx);
                }
            })
            .children(turns.iter().map(|turn| {
                let turn_ix = turn.turn_index;
                let entry_ix = turn.user_entry_ix;
                let width = tick_width_for_distance(turn_ix, hovered_turn, active_turn);
                let is_hovered = hovered_turn == Some(turn_ix);
                let is_active = active_turn == Some(turn_ix);

                let bar_color = if is_hovered {
                    colors.text
                } else if is_active {
                    colors.text_accent
                } else if let Some(hovered) = hovered_turn {
                    match turn_ix.abs_diff(hovered) {
                        1 => colors.text_muted.opacity(0.78),
                        2 => colors.text_muted.opacity(0.55),
                        _ => colors.text_muted.opacity(0.32),
                    }
                } else {
                    colors.text_muted.opacity(0.35)
                };

                let on_hover_tick = on_hover.clone();
                let on_select_tick = on_select.clone();

                h_flex()
                    .id(("turn-navigator-tick", turn_ix))
                    .cursor_pointer()
                    .h(tick_row_height)
                    .w(px(22.))
                    .items_center()
                    .justify_start()
                    .on_hover(move |hovered, window, cx| {
                        if *hovered {
                            on_hover_tick(Some(turn_ix), window, cx);
                        }
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_select_tick(entry_ix, window, cx);
                    })
                    .child(div().h(px(2.)).w(px(width)).rounded_full().bg(bar_color))
            }))
            .into_any_element(),
    )
}

/// Renders the floating preview card to the right of the left tick rail while
/// a tick is hovered.
pub(crate) fn render_turn_preview_card(
    turns: &[ConversationTurn],
    hovered_turn: Option<usize>,
    timing_tracker: &TurnTimingTracker,
    cx: &App,
) -> Option<AnyElement> {
    let turn = hovered_turn.and_then(|ix| turns.get(ix))?;
    let colors = cx.theme().colors();
    let total_turns = turns.len();
    let reply_text = turn
        .reply_preview
        .clone()
        .unwrap_or_else(|| SharedString::from("Thinking or running tools…"));

    let mut meta_parts = vec![format!("{}/{}", turn.turn_index + 1, total_turns)];
    if let Some(sent_time) = timing_tracker.user_sent_time_label(turn.user_entry_ix) {
        meta_parts.push(sent_time.to_string());
    }
    if let Some(ai_timing) = timing_tracker.assistant_timing_for_turn(turn.user_entry_ix)
        && let Some(duration) = ai_timing.duration_label
    {
        meta_parts.push(if ai_timing.is_running {
            format!("{duration}…")
        } else {
            duration.to_string()
        });
    }
    let meta_text = meta_parts.join(" · ");

    Some(
        v_flex()
            .w(px(276.))
            .max_w(relative(0.82))
            .p_3()
            .gap_1p5()
            .rounded(px(10.))
            .bg(colors.elevated_surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .shadow_lg()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.text)
                            .child(turn.prompt_preview.clone()),
                    )
                    .child(
                        Label::new(meta_text)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .flex_none(),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(reply_text),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_preview_text_strips_markdown_and_code_fences() {
        let input =
            "\n## 标题说明\n\n```rust\nfn hidden() {}\n```\n- 第一点核心结论\n- 第二点补充说明";
        let cleaned = clean_preview_text(input, 40);
        assert_eq!(cleaned, "标题说明 第一点核心结论 第二点补充说明");
    }

    #[test]
    fn test_truncate_chars_handles_multibyte_chinese_safely() {
        assert_eq!(truncate_chars("你好世界测试文本", 4), "你好世界…");
        assert_eq!(truncate_chars("短文本", 10), "短文本");
    }

    #[test]
    fn test_format_duration_human_readable() {
        assert_eq!(format_duration(Duration::from_millis(50)), "<1s");
        assert_eq!(format_duration(Duration::from_millis(650)), "0.6s");
        assert_eq!(format_duration(Duration::from_secs(14)), "14s");
        assert_eq!(format_duration(Duration::from_secs(135)), "2m 15s");
        assert_eq!(format_duration(Duration::from_secs(3900)), "1h 05m");
    }

    #[test]
    fn test_active_turn_index_tracks_scroll_position() {
        let turns = vec![
            ConversationTurn {
                turn_index: 0,
                user_entry_ix: 0,
                prompt_preview: "q1".into(),
                reply_preview: None,
            },
            ConversationTurn {
                turn_index: 1,
                user_entry_ix: 5,
                prompt_preview: "q2".into(),
                reply_preview: None,
            },
            ConversationTurn {
                turn_index: 2,
                user_entry_ix: 12,
                prompt_preview: "q3".into(),
                reply_preview: None,
            },
        ];

        assert_eq!(active_turn_index(&turns, 0), Some(0));
        assert_eq!(active_turn_index(&turns, 4), Some(0));
        assert_eq!(active_turn_index(&turns, 5), Some(1));
        assert_eq!(active_turn_index(&turns, 11), Some(1));
        assert_eq!(active_turn_index(&turns, 12), Some(2));
        assert_eq!(active_turn_index(&turns, 99), Some(2));
    }

    #[test]
    fn test_tick_width_wave_emphasizes_hovered_and_neighbors() {
        assert_eq!(tick_width_for_distance(3, Some(3), Some(0)), 20.0);
        assert_eq!(tick_width_for_distance(2, Some(3), Some(0)), 14.0);
        assert_eq!(tick_width_for_distance(4, Some(3), Some(0)), 14.0);
        assert_eq!(tick_width_for_distance(1, Some(3), Some(0)), 10.0);
        assert_eq!(tick_width_for_distance(0, Some(3), Some(0)), 11.0);
        assert_eq!(tick_width_for_distance(6, Some(3), Some(0)), 7.0);
        assert_eq!(tick_width_for_distance(2, None, Some(2)), 15.0);
        assert_eq!(tick_width_for_distance(1, None, Some(2)), 7.0);
    }
}
