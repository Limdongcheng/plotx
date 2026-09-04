//! Ribbon placement and gating of the Arrange grid popover and the on-plot
//! phase tool.

use super::tests::{app_with_nmr, app_with_table};
use super::*;

fn app() -> PlotxApp {
    PlotxApp::new_with_settings(plotx_core::settings::Settings::default())
}

/// The Ribbon's Layout group offers the grid popover in place of fixed
/// preset tiles; the presets keep their menu and palette entries.
#[test]
fn ribbon_offers_the_grid_popover_instead_of_preset_tiles() {
    assert_eq!(
        ribbon_placement(CommandId::ArrangeGridCustom),
        Some(RibbonPlacement {
            tab: WorkflowTab::Arrange,
            group: "Layout",
            priority: 0,
            applicability: Applicability::Always,
        })
    );
    for &(_, rows, cols) in plotx_core::layout::GRID_PRESETS {
        assert_eq!(ribbon_placement(CommandId::ArrangeGrid(rows, cols)), None);
        assert!(
            catalog(&app())
                .iter()
                .any(|command| command.id == CommandId::ArrangeGrid(rows, cols))
        );
    }
    let empty = describe(&app(), CommandId::ArrangeGridCustom);
    assert!(!empty.enabled);
    assert_eq!(
        empty.disabled_reason,
        Some("Open a canvas before arranging its plots.")
    );
    assert!(describe(&app_with_nmr(), CommandId::ArrangeGridCustom).enabled);
}

/// The on-plot phase tool is gated like the Phase settings tile: a table or
/// an empty document cannot be phased, and the tooltip says what can.
#[test]
fn manual_phase_needs_a_spectrum_with_a_phase_step() {
    let reason = "Select an NMR spectrum with a phase processing step before phasing it by hand.";
    for app in [app(), app_with_table(1)] {
        let command = describe(&app, CommandId::Tool(Tool::ManualPhase));
        assert!(!command.enabled);
        assert_eq!(command.disabled_reason, Some(reason));
    }
    assert!(describe(&app_with_nmr(), CommandId::Tool(Tool::ManualPhase)).enabled);
}
