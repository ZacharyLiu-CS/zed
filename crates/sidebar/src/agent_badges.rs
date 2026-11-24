use agent::ZED_AGENT_ID;
use agent_ui::terminal_thread_metadata_store::TerminalThreadMetadata;
use gpui::{App, Entity, Hsla, SharedString, hsla};
use project::{AgentId, AgentRegistryStore, AgentServerStore};
use std::collections::HashSet;
use std::mem;
use theme::ActiveTheme as _;
use ui::{IconName, ThreadItemAgentBadge};
use workspace::ProjectGroupKey;

pub(crate) fn stable_color_hash(input: &str) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 2166136261;
    const FNV_PRIME: u32 = 16777619;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in input.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub(crate) fn project_abbreviation(label: &str) -> SharedString {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_is_lower = false;

    for ch in label.chars() {
        if ch == '_' || ch == '-' || ch == '.' || ch == '/' || ch == '\\' || ch.is_whitespace() {
            if !current.is_empty() {
                segments.push(mem::take(&mut current));
            }
            prev_is_lower = false;
            continue;
        }

        if prev_is_lower && ch.is_uppercase() && !current.is_empty() {
            segments.push(mem::take(&mut current));
        }
        prev_is_lower = ch.is_lowercase();
        current.push(ch);
    }
    if !current.is_empty() {
        segments.push(current);
    }

    let mut abbreviation = String::new();
    if segments.len() >= 2 {
        for segment in segments.iter().take(2) {
            if let Some(ch) = segment.chars().next() {
                abbreviation.extend(ch.to_uppercase());
            }
        }
    } else if let Some(segment) = segments.first() {
        for ch in segment.chars().take(2) {
            abbreviation.extend(ch.to_uppercase());
        }
    }

    if abbreviation.is_empty() {
        "?".into()
    } else {
        abbreviation.into()
    }
}

pub(crate) fn project_color_for_key(
    key: &ProjectGroupKey,
    used_indices: &mut HashSet<usize>,
    cx: &App,
) -> Hsla {
    let accents = cx.theme().accents();
    let palette_len = accents.0.len();
    if palette_len == 0 {
        return cx.theme().colors().text_accent;
    }

    let mut hash_input = String::new();
    if let Some(host) = key.host() {
        hash_input.push_str(&host.display_name());
        hash_input.push(':');
    }
    for path in key.path_list().paths() {
        hash_input.push_str(&path.to_string_lossy());
        hash_input.push('\0');
    }

    let preferred_index = (stable_color_hash(&hash_input) as usize) % palette_len;
    let chosen_index = if used_indices.len() < palette_len {
        (0..palette_len)
            .map(|offset| (preferred_index + offset) % palette_len)
            .find(|index| !used_indices.contains(index))
            .unwrap_or(preferred_index)
    } else {
        preferred_index
    };
    used_indices.insert(chosen_index);

    accents.color_for_index(chosen_index as u32)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KnownAgentKind {
    Zed,
    Claude,
    Codex,
    Cursor,
    CodeBuddy,
    Gemini,
    OpenCode,
    Copilot,
}

fn classify_known_agent(id_lower: &str, display_lower: &str) -> Option<KnownAgentKind> {
    let matches_any = |needle: &str| id_lower.contains(needle) || display_lower.contains(needle);
    if id_lower == "zed agent" || id_lower == "zed-agent" || id_lower == "zed" {
        Some(KnownAgentKind::Zed)
    } else if matches_any("claude") || matches_any("anthropic") {
        Some(KnownAgentKind::Claude)
    } else if matches_any("codex") || matches_any("openai") {
        Some(KnownAgentKind::Codex)
    } else if matches_any("codebuddy") {
        Some(KnownAgentKind::CodeBuddy)
    } else if matches_any("cursor") {
        Some(KnownAgentKind::Cursor)
    } else if matches_any("gemini") {
        Some(KnownAgentKind::Gemini)
    } else if matches_any("opencode") || matches_any("open-code") {
        Some(KnownAgentKind::OpenCode)
    } else if matches_any("copilot") {
        Some(KnownAgentKind::Copilot)
    } else {
        None
    }
}

fn known_agent_color(kind: KnownAgentKind, cx: &App) -> Hsla {
    let is_light = cx.theme().appearance().is_light();
    match kind {
        KnownAgentKind::Zed => cx.theme().colors().text_accent,
        KnownAgentKind::Claude => {
            if is_light {
                hsla(15.0 / 360.0, 0.78, 0.42, 1.0)
            } else {
                hsla(15.0 / 360.0, 0.78, 0.62, 1.0)
            }
        }
        KnownAgentKind::Codex => {
            if is_light {
                hsla(162.0 / 360.0, 0.68, 0.34, 1.0)
            } else {
                hsla(162.0 / 360.0, 0.60, 0.56, 1.0)
            }
        }
        KnownAgentKind::Cursor => {
            if is_light {
                hsla(210.0 / 360.0, 0.80, 0.42, 1.0)
            } else {
                hsla(210.0 / 360.0, 0.80, 0.65, 1.0)
            }
        }
        KnownAgentKind::CodeBuddy => {
            if is_light {
                hsla(268.0 / 360.0, 0.68, 0.46, 1.0)
            } else {
                hsla(268.0 / 360.0, 0.76, 0.69, 1.0)
            }
        }
        KnownAgentKind::Gemini => {
            if is_light {
                hsla(224.0 / 360.0, 0.78, 0.44, 1.0)
            } else {
                hsla(224.0 / 360.0, 0.84, 0.67, 1.0)
            }
        }
        KnownAgentKind::OpenCode => {
            if is_light {
                hsla(38.0 / 360.0, 0.82, 0.38, 1.0)
            } else {
                hsla(38.0 / 360.0, 0.82, 0.62, 1.0)
            }
        }
        KnownAgentKind::Copilot => {
            if is_light {
                hsla(238.0 / 360.0, 0.70, 0.46, 1.0)
            } else {
                hsla(238.0 / 360.0, 0.74, 0.68, 1.0)
            }
        }
    }
}

fn preferred_agent_label(
    raw_id: &str,
    canonical_ids: &[&str],
    canonical_label: &'static str,
) -> SharedString {
    let trimmed = raw_id.trim();
    if trimmed.is_empty() {
        return canonical_label.into();
    }
    let lower = trimmed.to_lowercase();
    let stripped = lower.strip_suffix("-acp").unwrap_or(&lower);
    if stripped.is_empty() || canonical_ids.contains(&stripped) {
        canonical_label.into()
    } else {
        stripped.to_string().into()
    }
}

fn known_agent_badge(
    kind: KnownAgentKind,
    raw_id: &str,
    external_svg: Option<SharedString>,
    tooltip: Option<SharedString>,
    cx: &App,
) -> ThreadItemAgentBadge {
    let color = Some(known_agent_color(kind, cx));
    match kind {
        KnownAgentKind::Zed => ThreadItemAgentBadge {
            label: "zed".into(),
            icon: IconName::ZedAgent,
            custom_icon_from_external_svg: None,
            color,
            tooltip: tooltip.or_else(|| Some("Zed Agent".into())),
        },
        KnownAgentKind::Claude => ThreadItemAgentBadge {
            label: preferred_agent_label(
                raw_id,
                &["claude", "claude-code", "claude-agent", "anthropic"],
                "claude",
            ),
            icon: IconName::AiClaude,
            custom_icon_from_external_svg: None,
            color,
            tooltip: tooltip.or_else(|| Some("Claude".into())),
        },
        KnownAgentKind::Codex => ThreadItemAgentBadge {
            label: preferred_agent_label(raw_id, &["codex", "openai-codex", "openai"], "codex"),
            icon: IconName::AiOpenAi,
            custom_icon_from_external_svg: None,
            color,
            tooltip: tooltip.or_else(|| Some("Codex".into())),
        },
        KnownAgentKind::Cursor => ThreadItemAgentBadge {
            label: preferred_agent_label(raw_id, &["cursor", "cursor-agent"], "cursor"),
            icon: IconName::Sparkle,
            custom_icon_from_external_svg: external_svg,
            color,
            tooltip: tooltip.or_else(|| Some("Cursor".into())),
        },
        KnownAgentKind::CodeBuddy => ThreadItemAgentBadge {
            label: preferred_agent_label(raw_id, &["codebuddy-code"], "codebuddy-code"),
            icon: IconName::Sparkle,
            custom_icon_from_external_svg: external_svg,
            color,
            tooltip: tooltip.or_else(|| Some("CodeBuddy".into())),
        },
        KnownAgentKind::Gemini => ThreadItemAgentBadge {
            label: preferred_agent_label(raw_id, &["gemini", "gemini-cli"], "gemini"),
            icon: IconName::AiGemini,
            custom_icon_from_external_svg: None,
            color,
            tooltip: tooltip.or_else(|| Some("Gemini".into())),
        },
        KnownAgentKind::OpenCode => ThreadItemAgentBadge {
            label: preferred_agent_label(raw_id, &["opencode", "open-code"], "opencode"),
            icon: IconName::AiOpenCode,
            custom_icon_from_external_svg: None,
            color,
            tooltip: tooltip.or_else(|| Some("OpenCode".into())),
        },
        KnownAgentKind::Copilot => ThreadItemAgentBadge {
            label: preferred_agent_label(raw_id, &["copilot", "github-copilot"], "copilot"),
            icon: IconName::Copilot,
            custom_icon_from_external_svg: None,
            color,
            tooltip: tooltip.or_else(|| Some("Copilot".into())),
        },
    }
}

pub(crate) fn resolve_agent_badge(
    agent_id: &AgentId,
    agent_server_store: Option<&Entity<AgentServerStore>>,
    cx: &App,
) -> ThreadItemAgentBadge {
    let registry_store = AgentRegistryStore::try_global(cx);
    let display_name = if agent_id.as_ref() == ZED_AGENT_ID.as_ref() {
        Some(SharedString::from("Zed Agent"))
    } else {
        agent_server_store
            .and_then(|store| store.read(cx).agent_display_name(agent_id))
            .or_else(|| {
                registry_store
                    .as_ref()
                    .and_then(|store| store.read(cx).agent(agent_id))
                    .map(|agent| agent.name().clone())
            })
    };

    let external_svg = agent_server_store
        .and_then(|store| store.read(cx).agent_icon(agent_id))
        .or_else(|| {
            registry_store
                .as_ref()
                .and_then(|store| store.read(cx).agent(agent_id))
                .and_then(|agent| agent.icon_path().cloned())
        });

    let id_lower = agent_id.as_ref().to_lowercase();
    let display_lower = display_name
        .as_ref()
        .map(|name| name.to_lowercase())
        .unwrap_or_default();

    if let Some(kind) = classify_known_agent(&id_lower, &display_lower) {
        return known_agent_badge(kind, agent_id.as_ref(), external_svg, display_name, cx);
    }

    let normalized = id_lower
        .strip_suffix("-acp")
        .filter(|s| !s.is_empty())
        .unwrap_or(&id_lower);
    let label: SharedString = if !normalized.is_empty() {
        normalized.to_string().into()
    } else {
        display_name
            .clone()
            .unwrap_or_else(|| agent_id.0.clone())
    };

    let accents = cx.theme().accents();
    let color = if accents.0.is_empty() {
        None
    } else {
        Some(accents.color_for_index(stable_color_hash(agent_id.as_ref())))
    };

    ThreadItemAgentBadge {
        label,
        icon: IconName::Sparkle,
        custom_icon_from_external_svg: external_svg,
        color,
        tooltip: display_name.or_else(|| Some(agent_id.0.clone())),
    }
}

fn detect_known_agent_in_terminal_title(title: &str) -> Option<(KnownAgentKind, String)> {
    let lower = title.to_lowercase();
    if lower.contains("codebuddy-code") {
        return Some((KnownAgentKind::CodeBuddy, "codebuddy-code".to_string()));
    }

    for word in lower.split(|ch: char| !ch.is_alphanumeric() && ch != '-' && ch != '_') {
        if word.is_empty() {
            continue;
        }
        if word.contains("claude") {
            return Some((KnownAgentKind::Claude, word.to_string()));
        }
        if word.contains("codex") {
            return Some((KnownAgentKind::Codex, word.to_string()));
        }
        if word.contains("codebuddy") {
            return Some((KnownAgentKind::CodeBuddy, word.to_string()));
        }
        match word {
            "cursor" | "tcursor" => return Some((KnownAgentKind::Cursor, word.to_string())),
            "gemini" | "tgemini" => return Some((KnownAgentKind::Gemini, word.to_string())),
            "opencode" | "open-code" => return Some((KnownAgentKind::OpenCode, word.to_string())),
            _ => {}
        }
    }

    None
}

pub(crate) fn resolve_terminal_agent_badge(
    metadata: &TerminalThreadMetadata,
    cx: &App,
) -> ThreadItemAgentBadge {
    resolve_terminal_agent_badge_for_title(metadata.title.as_ref(), cx)
}

fn resolve_terminal_agent_badge_for_title(title: &str, cx: &App) -> ThreadItemAgentBadge {
    if let Some((kind, raw_id)) = detect_known_agent_in_terminal_title(title) {
        return known_agent_badge(kind, &raw_id, None, None, cx);
    }

    ThreadItemAgentBadge {
        label: "terminal".into(),
        icon: IconName::Terminal,
        custom_icon_from_external_svg: None,
        color: None,
        tooltip: Some("Terminal".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use settings::SettingsStore;
    use std::path::PathBuf;
    use util::path_list::PathList;

    fn init_theme(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
        });
    }

    #[test]
    fn test_project_abbreviation_generation() {
        assert_eq!(project_abbreviation("adt_projects").as_ref(), "AP");
        assert_eq!(project_abbreviation("dop_projects").as_ref(), "DP");
        assert_eq!(project_abbreviation("dop-scripts").as_ref(), "DS");
        assert_eq!(project_abbreviation("ceph-agent").as_ref(), "CA");
        assert_eq!(project_abbreviation("dop-manager-dds-phase2").as_ref(), "DM");
        assert_eq!(project_abbreviation("zed").as_ref(), "ZE");
        assert_eq!(project_abbreviation("myProject").as_ref(), "MP");
        assert_eq!(project_abbreviation("a").as_ref(), "A");
        assert_eq!(project_abbreviation("").as_ref(), "?");
    }

    #[gpui::test]
    fn test_project_colors_are_distinct_across_folders(cx: &mut TestAppContext) {
        init_theme(cx);
        cx.update(|cx| {
            let mut used = HashSet::new();
            let keys = [
                ProjectGroupKey::new(None, PathList::new(&[PathBuf::from("/adt_projects")])),
                ProjectGroupKey::new(
                    None,
                    PathList::new(&[PathBuf::from("/dop-manager-dds-phase2")]),
                ),
                ProjectGroupKey::new(None, PathList::new(&[PathBuf::from("/dop-scripts")])),
                ProjectGroupKey::new(None, PathList::new(&[PathBuf::from("/ceph-agent")])),
            ];

            let colors: Vec<Hsla> = keys
                .iter()
                .map(|key| project_color_for_key(key, &mut used, cx))
                .collect();

            for i in 0..colors.len() {
                for j in (i + 1)..colors.len() {
                    assert_ne!(
                        colors[i], colors[j],
                        "projects {i} and {j} should receive distinct colors"
                    );
                }
            }
        });
    }

    #[gpui::test]
    fn test_agent_badge_resolution_for_known_and_custom_agents(cx: &mut TestAppContext) {
        init_theme(cx);
        cx.update(|cx| {
            let claude = resolve_agent_badge(&AgentId::new("claude-acp"), None, cx);
            assert_eq!(claude.label.as_ref(), "claude");
            assert_eq!(claude.icon, IconName::AiClaude);
            assert!(claude.color.is_some());

            let tclaude = resolve_agent_badge(&AgentId::new("tclaude"), None, cx);
            assert_eq!(tclaude.label.as_ref(), "tclaude");
            assert_eq!(tclaude.icon, IconName::AiClaude);
            assert_eq!(tclaude.color, claude.color);

            let codex = resolve_agent_badge(&AgentId::new("codex-acp"), None, cx);
            assert_eq!(codex.label.as_ref(), "codex");
            assert_eq!(codex.icon, IconName::AiOpenAi);
            assert!(codex.color.is_some());

            let tcodex = resolve_agent_badge(&AgentId::new("tcodex"), None, cx);
            assert_eq!(tcodex.label.as_ref(), "tcodex");
            assert_eq!(tcodex.icon, IconName::AiOpenAi);
            assert_eq!(tcodex.color, codex.color);

            let cursor = resolve_agent_badge(&AgentId::new("cursor"), None, cx);
            assert_eq!(cursor.label.as_ref(), "cursor");
            assert!(cursor.color.is_some());

            let codebuddy = resolve_agent_badge(&AgentId::new("codebuddy-code"), None, cx);
            assert_eq!(codebuddy.label.as_ref(), "codebuddy-code");
            assert!(codebuddy.color.is_some());

            let zed = resolve_agent_badge(&ZED_AGENT_ID, None, cx);
            assert_eq!(zed.label.as_ref(), "zed");
            assert_eq!(zed.icon, IconName::ZedAgent);

            let terminal_claude = resolve_terminal_agent_badge_for_title("✳ Claude Code", cx);
            assert_eq!(terminal_claude.label.as_ref(), "claude");
            assert_eq!(terminal_claude.icon, IconName::AiClaude);

            let terminal_tclaude =
                resolve_terminal_agent_badge_for_title("tclaude — adt_projects", cx);
            assert_eq!(terminal_tclaude.label.as_ref(), "tclaude");
            assert_eq!(terminal_tclaude.icon, IconName::AiClaude);

            let terminal_plain =
                resolve_terminal_agent_badge_for_title("block-cache-lazy-loading.md", cx);
            assert_eq!(terminal_plain.label.as_ref(), "terminal");
            assert_eq!(terminal_plain.icon, IconName::Terminal);
        });
    }
}
