use acp_thread::{AcpThread, AgentThreadEntry, AssistantMessageChunk, ToolCall, ToolCallStatus};
use gpui::{AnyElement, App, FontWeight, MouseButton, SharedString, px};
use std::sync::atomic::{AtomicBool, Ordering};
use ui::{Color, Icon, IconName, IconSize, Label, LabelSize, LineHeightStyle, Tooltip, prelude::*};

static HIDE_THINKING_AND_TOOLS: AtomicBool = AtomicBool::new(false);

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
            0 => 22.0,
            1 => 15.0,
            2 => 11.0,
            _ => {
                if active_turn == Some(turn_ix) {
                    12.0
                } else {
                    8.0
                }
            }
        }
    } else if active_turn == Some(turn_ix) {
        16.0
    } else {
        8.0
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
            .elevated_surface_background
            .blend(colors.text_accent.opacity(0.14))
    } else {
        colors.elevated_surface_background.opacity(0.92)
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
            .h(px(24.))
            .px_2()
            .rounded_md()
            .bg(bg)
            .border_1()
            .border_color(border)
            .shadow_sm()
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

pub(crate) fn render_turn_navigator(
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
    let total_turns = turns.len();
    let tick_row_height = if total_turns > 30 { px(7.) } else { px(10.) };

    let preview_card = hovered_turn.and_then(|ix| turns.get(ix)).map(|turn| {
        let reply_text = turn
            .reply_preview
            .clone()
            .unwrap_or_else(|| SharedString::from("Thinking or running tools…"));

        v_flex()
            .w(px(288.))
            .mr_2p5()
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
                        Label::new(format!("{}/{}", turn.turn_index + 1, total_turns))
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
            .into_any_element()
    });

    let on_hover_rail_leave = on_hover.clone();
    let rail = v_flex()
        .id("conversation-turn-navigator-rail")
        .py_1p5()
        .px_1()
        .items_end()
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
                .w(px(26.))
                .items_center()
                .justify_end()
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
        }));

    Some(
        h_flex()
            .items_center()
            .when_some(preview_card, |this, card| this.child(card))
            .child(rail)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_preview_text_strips_markdown_and_code_fences() {
        let input = "\n## 标题说明\n\n```rust\nfn hidden() {}\n```\n- 第一点核心结论\n- 第二点补充说明";
        let cleaned = clean_preview_text(input, 40);
        assert_eq!(cleaned, "标题说明 第一点核心结论 第二点补充说明");
    }

    #[test]
    fn test_truncate_chars_handles_multibyte_chinese_safely() {
        assert_eq!(truncate_chars("你好世界测试文本", 4), "你好世界…");
        assert_eq!(truncate_chars("短文本", 10), "短文本");
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
        assert_eq!(tick_width_for_distance(3, Some(3), Some(0)), 22.0);
        assert_eq!(tick_width_for_distance(2, Some(3), Some(0)), 15.0);
        assert_eq!(tick_width_for_distance(4, Some(3), Some(0)), 15.0);
        assert_eq!(tick_width_for_distance(1, Some(3), Some(0)), 11.0);
        assert_eq!(tick_width_for_distance(0, Some(3), Some(0)), 12.0);
        assert_eq!(tick_width_for_distance(6, Some(3), Some(0)), 8.0);
        assert_eq!(tick_width_for_distance(2, None, Some(2)), 16.0);
        assert_eq!(tick_width_for_distance(1, None, Some(2)), 8.0);
    }
}
