//! Icon-vocabulary tests: within one Ribbon group every icon appears once, so
//! two commands sharing a visual field are never told apart only by luck.

use std::collections::HashMap;

use plotx_core::state::PlotxApp;

use super::{CommandId, command_ids, describe, ribbon_placement};

/// A parameterized variant is one visual family: its members deliberately
/// share an icon and are told apart by label (canvas presets, themes, export
/// formats). Tools and property groups are independent commands, never a
/// family.
fn same_family(a: CommandId, b: CommandId) -> bool {
    !matches!(a, CommandId::PropertyGroup(_) | CommandId::Tool(_))
        && std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

#[test]
fn ribbon_group_icons_are_unique_within_the_group() {
    let app = PlotxApp::new_with_settings(plotx_core::settings::Settings::default());
    let mut groups: HashMap<(&'static str, &'static str), Vec<(CommandId, &'static str)>> =
        HashMap::new();
    for id in command_ids(0) {
        let Some(placement) = ribbon_placement(id) else {
            continue;
        };
        let Some(icon) = describe(&app, id).icon else {
            continue;
        };
        groups
            .entry((placement.tab.label(), placement.group))
            .or_default()
            .push((id, icon));
    }
    for ((tab, group), entries) in groups {
        for (index, &(id, icon)) in entries.iter().enumerate() {
            for &(other, other_icon) in &entries[index + 1..] {
                assert!(
                    icon != other_icon || same_family(id, other),
                    "({tab:?}, {group}) shows {id:?} and {other:?} with the same icon; \
                     assign a distinct glyph in command_identity or the group declaration"
                );
            }
        }
    }
}
