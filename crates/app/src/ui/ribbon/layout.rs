//! Width mathematics for the Ribbon: density selection, group measurement,
//! ordering, and the overflow partition. All text is sized through an injected
//! [`Measure`] so the same arithmetic runs under the live font system and in
//! headless tests.

use egui::FontId;
use plotx_core::state::WorkflowTab;

use super::super::commands::CommandDescriptor;
use super::RibbonDensity;
use super::buttons::short_label;

pub(super) const AUTO_COLLAPSE_WIDTH: f32 = 760.0;
/// One shared tile height (Full density) and row height (Compact) keeps every
/// command in a group visually equal-sized.
pub(super) const TILE_HEIGHT: f32 = 46.0;
pub(super) const ROW_HEIGHT: f32 = 26.0;

/// Returns the width of `text` in `font`. Layout decisions and painting must
/// agree on glyph widths (a per-character estimate drifts on CJK and long
/// labels), so production injects the live font system via [`text_measure`]
/// and tests inject a deterministic stand-in.
pub(super) type Measure<'a> = &'a dyn Fn(&str, FontId) -> f32;

/// The production [`Measure`]: galley layout through the context's fonts.
/// Owning a context clone keeps the closure free of `Ui` borrows, so callers
/// can keep mutating the `Ui` they are laying out.
pub(super) fn text_measure(ctx: egui::Context) -> impl Fn(&str, FontId) -> f32 {
    move |text, font| {
        ctx.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
                .size()
                .x
        })
    }
}

/// The richest density whose content actually fits `width`: full icon-and-text
/// tiles whenever the active tab's groups all fit, otherwise the compact icon
/// row (whose own overflow moves whole groups into More). Below the absolute
/// floor even icon rows crowd, so the command area collapses to menus.
pub(super) fn density(
    width: f32,
    expanded: bool,
    groups: &[(&'static str, u8, Vec<&CommandDescriptor>)],
    measure: Measure,
) -> RibbonDensity {
    if !expanded || width < AUTO_COLLAPSE_WIDTH {
        RibbonDensity::Collapsed
    } else if required_width(groups, RibbonDensity::Full, measure) <= width {
        RibbonDensity::Full
    } else {
        RibbonDensity::Compact
    }
}

/// Width the whole tab needs at `density`: every group plus its separator.
/// The same measurement drives the density choice and the overflow budget, so
/// a tab shown Full is guaranteed to fit without a More menu.
pub(super) fn required_width(
    groups: &[(&'static str, u8, Vec<&CommandDescriptor>)],
    density: RibbonDensity,
    measure: Measure,
) -> f32 {
    groups
        .iter()
        .map(|(title, _, entries)| group_width(title, entries, density, measure) + 8.0)
        .sum()
}

pub(super) fn group_width(
    title: &str,
    entries: &[&CommandDescriptor],
    density: RibbonDensity,
    measure: Measure,
) -> f32 {
    let spacing = 4.0 * entries.len().saturating_sub(1) as f32;
    let commands = if density == RibbonDensity::Full {
        tile_width(entries, measure) * entries.len() as f32 + spacing
    } else {
        entries
            .iter()
            .map(|command| button_width(command, measure))
            .sum::<f32>()
            + spacing
    };
    commands.max(measure(title, crate::typography::caption_font()) + 8.0)
}

/// All tiles in a group share the width of the widest short label, so a group
/// reads as one row of even targets instead of a ragged strip.
pub(super) fn tile_width(entries: &[&CommandDescriptor], measure: Measure) -> f32 {
    entries
        .iter()
        .map(|command| measure(&short_label(command), crate::typography::subheadline_font()) + 18.0)
        .fold(58.0, f32::max)
        .min(112.0)
}

pub(super) fn button_width(command: &CommandDescriptor, measure: Measure) -> f32 {
    if command.icon.is_some() {
        ROW_HEIGHT
    } else {
        (measure(&short_label(command), crate::typography::callout_font()) + 16.0)
            .clamp(40.0, 140.0)
    }
}

/// Which groups stay on the Ribbon within `budget`. Groups are admitted in
/// priority order and admission stops at the first group that does not fit, so
/// the visible set is always a highest-priority prefix: nothing in the More
/// menu ever outranks a group that stayed visible.
pub(super) fn shown_groups(priorities: &[u8], widths: &[f32], budget: f32) -> Vec<bool> {
    debug_assert_eq!(priorities.len(), widths.len());
    let mut ranked: Vec<usize> = (0..priorities.len()).collect();
    ranked.sort_by_key(|&index| priorities[index]);
    let mut shown = vec![false; priorities.len()];
    let mut used = 0.0;
    for index in ranked {
        if used + widths[index] > budget {
            break;
        }
        shown[index] = true;
        used += widths[index];
    }
    shown
}

pub(super) fn groups_for_tab(
    catalog: &[CommandDescriptor],
    tab: WorkflowTab,
) -> Vec<(&'static str, u8, Vec<&CommandDescriptor>)> {
    let mut groups: Vec<(&'static str, u8, Vec<&CommandDescriptor>)> = Vec::new();
    for command in catalog {
        let Some(placement) = command.ribbon.filter(|placement| placement.tab == tab) else {
            continue;
        };
        if let Some((_, priority, entries)) = groups
            .iter_mut()
            .find(|(group, _, _)| *group == placement.group)
        {
            *priority = (*priority).min(placement.priority);
            entries.push(command);
        } else {
            groups.push((placement.group, placement.priority, vec![command]));
        }
    }
    groups.sort_by_key(|(group, _, _)| group_order(tab, group));
    groups
}

/// Left-to-right order of every Ribbon group, tab by tab, following each
/// tab's workflow reading. Every (tab, group) pair the placement tables can
/// produce must appear here: an unlisted pair would fall back to catalog
/// iteration order, which is accidental. Guarded by
/// `every_ribbon_group_has_an_explicit_order`.
pub(super) fn group_order(tab: WorkflowTab, group: &str) -> u8 {
    let order: &[&str] = match tab {
        WorkflowTab::Data => &["Import", "Build", "Export"],
        WorkflowTab::Process => &["Processing", "Correct", "Transform", "Recipes"],
        WorkflowTab::Analyze => &[
            "Range",
            "Extract",
            "Regions",
            "Peaks",
            "Review",
            "Align",
            "Peak Fit",
            "Curve Fit",
            "Statistics",
            "Interpret",
        ],
        WorkflowTab::Figure => &["Create", "Chart", "Data", "Style", "Canvas", "Output"],
        WorkflowTab::Arrange => &[
            "Layout",
            "Align",
            "Distribute",
            "Order",
            "Guides",
            "Annotate",
            "Object",
            "Canvas",
        ],
        WorkflowTab::View => &["Navigate", "Display"],
    };
    order
        .iter()
        .position(|&name| name == group)
        .map_or(u8::MAX, |index| index as u8)
}

#[cfg(test)]
mod tests {
    use super::super::super::commands;
    use super::*;
    use plotx_core::state::PlotxApp;

    /// Deterministic stand-in for the live font system: proportional to the
    /// font size like real glyphs, close to the Latin average width.
    fn estimate(text: &str, font: FontId) -> f32 {
        text.chars().count() as f32 * font.size * 0.53
    }

    #[test]
    fn density_follows_the_active_tabs_measured_content() {
        let app = PlotxApp::new_with_settings(plotx_core::settings::Settings::default());
        let catalog = commands::catalog(&app);
        let groups = groups_for_tab(&catalog, WorkflowTab::View);
        let full_need = required_width(&groups, RibbonDensity::Full, &estimate);
        assert!(
            full_need > AUTO_COLLAPSE_WIDTH,
            "test premise: the View tab's full-density content ({full_need}) must exceed the collapse floor"
        );

        // Full the moment the tab's content fits — no fixed window breakpoint.
        assert_eq!(
            density(full_need + 1.0, true, &groups, &estimate),
            RibbonDensity::Full
        );
        assert_eq!(
            density(full_need - 1.0, true, &groups, &estimate),
            RibbonDensity::Compact
        );
        assert_eq!(
            density(700.0, true, &groups, &estimate),
            RibbonDensity::Collapsed
        );
        assert_eq!(
            density(full_need + 1.0, false, &groups, &estimate),
            RibbonDensity::Collapsed
        );
    }

    #[test]
    fn compact_groups_reserve_width_for_single_line_titles() {
        assert!(group_width("Guides", &[], RibbonDensity::Compact, &estimate) > ROW_HEIGHT);
        assert!(group_width("Object", &[], RibbonDensity::Compact, &estimate) > ROW_HEIGHT);
    }

    #[test]
    fn overflow_keeps_the_highest_priority_prefix() {
        let priorities = [2u8, 0, 1, 3];
        let widths = [40.0, 50.0, 30.0, 20.0];
        // 50 + 30 fit; the priority-2 group does not, and it must also block
        // the smaller priority-3 group behind it — a lower-priority group must
        // never appear while a higher-priority one sits in the More menu.
        let shown = shown_groups(&priorities, &widths, 90.0);
        assert_eq!(shown, vec![false, true, true, false]);
    }

    #[test]
    fn every_ribbon_group_has_an_explicit_order() {
        for (tab, group) in commands::ribbon_group_pairs() {
            assert_ne!(
                group_order(tab, group),
                u8::MAX,
                "({tab:?}, {group:?}) has no explicit order; add it to group_order"
            );
        }
    }
}
