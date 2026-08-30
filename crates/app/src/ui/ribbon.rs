//! Task-oriented, collapsible command Ribbon. Its visual vocabulary stays close
//! to PlotX's existing light egui chrome; the task/group hierarchy is the only
//! idea borrowed from the supplied Office reference.

mod buttons;
mod layout;

use egui::{
    Align, Button, Label, Layout, PointerButton, RichText, Sense, TextWrapMode, Ui, UiBuilder,
    Vec2, vec2,
};
use egui_phosphor::regular as icon;
use plotx_core::state::{PlotxApp, ToolGroup, WorkflowTab};

use super::clipboard_table::ClipboardTablePaste;
use super::commands::{self, CommandDescriptor, CommandId};
use buttons::{overflow_item, ribbon_button};
use layout::{ROW_HEIGHT, TILE_HEIGHT};

/// The native metric includes a little more bottom breathing room than the
/// tab highlight needs visually; trim it so the highlight has equal margins.
const MACOS_TITLE_ROW_BOTTOM_TRIM: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RibbonDensity {
    Collapsed,
    Compact,
    Full,
}

pub(crate) fn render(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    chrome: super::RibbonChrome,
) {
    let width = ui.available_width();
    // Density is content-aware: measured against the active tab's groups, not a
    // fixed window-width breakpoint (which UI scaling would silently retune).
    // Measured before `task_row`, so a tab click adopts the new tab's density
    // one frame later — invisible in practice.
    let density = {
        let measure = layout::text_measure(ui.ctx().clone());
        let catalog = commands::catalog(app);
        let groups = layout::groups_for_tab(&catalog, app.session.ui.ribbon_tab);
        layout::density(width, app.session.ui.ribbon_expanded, &groups, &measure)
    };
    task_row(app, clipboard, ui, density, chrome);
    if density != RibbonDensity::Collapsed {
        ui.separator();
        command_row(app, clipboard, ui, density);
    }
}

fn task_row(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    density: RibbonDensity,
    chrome: super::RibbonChrome,
) {
    let Some(traffic_lights) = chrome.macos_traffic_lights else {
        // Windows and Linux retain the original Ribbon task-row layout; their
        // separate custom title bar owns all window chrome and dragging.
        ui.horizontal(|ui| render_task_row_contents(app, clipboard, ui, density, false, None));
        return;
    };

    let row_height =
        (traffic_lights.y - MACOS_TITLE_ROW_BOTTOM_TRIM).max(ui.spacing().interact_size.y);
    let vertical_spacing = ui.spacing().item_spacing.y;
    // The next widget is the rule below the unified title row. Suppress the
    // normal inter-widget gap so the rule sits on the row boundary; otherwise
    // the selected tab appears high despite being centered.
    ui.spacing_mut().item_spacing.y = 0.0;
    let (row_rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::hover());
    ui.spacing_mut().item_spacing.y = vertical_spacing;
    // Register the background first; interactive children below take
    // precedence while every remaining pixel continues to drag the window.
    let drag = ui.interact(
        row_rect,
        ui.id().with("macos_unified_titlebar_drag"),
        Sense::click_and_drag(),
    );
    if drag.drag_started_by(PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    let mut ui = ui.new_child(
        UiBuilder::new()
            .max_rect(row_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let leading = traffic_lights.x + 4.0;
    ui.add_space(leading);
    let compact_controls = super::ribbon_chrome::controls_need_compacting(
        app,
        &ui,
        density,
        leading,
        row_rect.width(),
    );
    let inline_title_width = super::ribbon_chrome::available_title_width(
        app,
        &ui,
        density,
        leading,
        row_rect.width(),
        compact_controls,
    );
    render_task_row_contents(
        app,
        clipboard,
        &mut ui,
        density,
        compact_controls,
        inline_title_width,
    );
}

fn render_task_row_contents(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    density: RibbonDensity,
    compact_controls: bool,
    inline_title_width: Option<f32>,
) {
    ui.spacing_mut().item_spacing.x = if density == RibbonDensity::Full {
        8.0
    } else {
        3.0
    };
    for tab in WorkflowTab::ALL {
        let selected = app.session.ui.ribbon_tab == tab;
        let response = ui.selectable_label(selected, crate::typography::headline(tab.label()));
        if response.clicked() {
            select_workflow_tab(app, tab);
            // Picking a task re-opens a manually collapsed command area;
            // width-driven auto-collapse still wins in `density()`.
            app.session.ui.ribbon_expanded = true;
        }
    }

    if let (Some(title), Some(width)) = (
        super::ribbon_chrome::inline_project_title(app),
        inline_title_width,
    ) {
        ui.separator();
        let title = ui.add_sized(
            [width, ui.spacing().interact_size.y],
            Label::new(RichText::new(title).color(ui.visuals().weak_text_color()))
                .truncate()
                .sense(Sense::click_and_drag()),
        );
        if title.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Stable id scope: siblings earlier in this row (the inline project
        // title, tab labels) come and go with app state; without this every
        // control in the strip changes id when they do, which drops focus
        // and trips egui's rect-changed-id debug overlay.
        let scope = UiBuilder::new()
            .id_salt(egui::Id::new("ribbon_chrome_controls"))
            .global_scope(true);
        ui.scope_builder(scope, |ui| {
            render_chrome_controls(app, clipboard, ui, density, compact_controls)
        });
    });
}

fn render_chrome_controls(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    density: RibbonDensity,
    compact_controls: bool,
) {
    let auto_collapsed = density == RibbonDensity::Collapsed && app.session.ui.ribbon_expanded;
    let full_collapse_label = if auto_collapsed {
        format!("{} Ribbon auto-collapsed", icon::CARET_DOWN)
    } else if app.session.ui.ribbon_expanded {
        format!("{} Collapse ribbon", icon::CARET_UP)
    } else {
        format!("{} Expand ribbon", icon::CARET_DOWN)
    };
    let collapse_label = if compact_controls {
        if app.session.ui.ribbon_expanded {
            icon::CARET_UP.to_owned()
        } else {
            icon::CARET_DOWN.to_owned()
        }
    } else {
        full_collapse_label
    };
    // The strip next to the task tabs stays quiet: chrome buttons show
    // their frame only on hover so they read no heavier than the tabs.
    let collapse = ui.add_enabled(
        !auto_collapsed,
        Button::new(collapse_label).frame_when_inactive(false),
    );
    let collapse = if auto_collapsed {
        collapse.on_disabled_hover_text(
            "The ribbon collapses automatically at this width; use menus or Search commands",
        )
    } else {
        collapse.on_hover_text("Collapse or expand the ribbon command area")
    };
    if collapse.clicked() {
        app.session.ui.ribbon_expanded = !app.session.ui.ribbon_expanded;
    }
    update_button(app, ui, compact_controls);
    let palette = commands::describe(app, CommandId::CommandPalette);
    let search_label = if compact_controls {
        icon::MAGNIFYING_GLASS.to_owned()
    } else {
        format!("{} Search commands", icon::MAGNIFYING_GLASS)
    };
    if ui
        .add(Button::new(search_label).frame_when_inactive(false))
        .on_hover_text(format!(
            "Search every command ({})",
            palette.shortcut.as_deref().unwrap_or("Ctrl+K")
        ))
        .clicked()
    {
        commands::execute(CommandId::CommandPalette, app, clipboard, ui.ctx());
    }
    ui.separator();
    // Right-to-left layout: added secondary-first so the pair reads
    // [left sidebar][right sidebar], mirroring the window.
    super::ribbon_chrome::sidebar_toggle_button(
        app,
        clipboard,
        ui,
        CommandId::ToggleSecondarySidebar,
    );
    super::ribbon_chrome::sidebar_toggle_button(
        app,
        clipboard,
        ui,
        CommandId::TogglePrimarySidebar,
    );
}

fn select_workflow_tab(app: &mut PlotxApp, tab: WorkflowTab) {
    app.session.ui.ribbon_tab = tab;
    match tab {
        WorkflowTab::Data => {}
        WorkflowTab::Process => super::tools::expand_processing_surface(app),
        WorkflowTab::Analyze => {
            if let Some(dataset) = app.active_dataset().and_then(|di| app.doc.datasets.get(di)) {
                app.session.ui.requested_tool_group = dataset
                    .tool_groups()
                    .iter()
                    .copied()
                    .find(|group| *group != ToolGroup::Processing);
            }
        }
        WorkflowTab::View | WorkflowTab::Figure | WorkflowTab::Arrange => {}
    }
}

fn command_row(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    density: RibbonDensity,
) {
    let tab = app.session.ui.ribbon_tab;
    let catalog = commands::catalog(app);
    let groups = layout::groups_for_tab(&catalog, tab);
    if density == RibbonDensity::Collapsed {
        return;
    }
    let measure = layout::text_measure(ui.ctx().clone());
    let widths: Vec<f32> = groups
        .iter()
        .map(|(title, _, entries)| layout::group_width(title, entries, density, &measure) + 8.0)
        .collect();
    let priorities: Vec<u8> = groups.iter().map(|(_, priority, _)| *priority).collect();
    let required: f32 = widths.iter().sum();
    let available = ui.available_width();
    let budget = if required <= available {
        available
    } else {
        (available - more_button_width(ui, &measure)).max(0.0)
    };
    let shown = layout::shown_groups(&priorities, &widths, budget);
    let (visible, hidden): (Vec<_>, Vec<_>) = groups
        .into_iter()
        .enumerate()
        .partition(|(index, _)| shown[*index]);

    ui.horizontal(|ui| {
        ui.set_min_height(if density == RibbonDensity::Full {
            TILE_HEIGHT + 18.0
        } else {
            ROW_HEIGHT + 18.0
        });
        ui.spacing_mut().item_spacing.x = if density == RibbonDensity::Full {
            7.0
        } else {
            3.0
        };
        for (_, (group, _, commands)) in visible {
            ribbon_group(app, clipboard, ui, group, commands, density, &measure);
            ui.separator();
        }
        if !hidden.is_empty() {
            ui.menu_button(more_label(), |ui| {
                for (_, (group, _, entries)) in hidden {
                    ui.label(crate::typography::headline(group));
                    for command in entries {
                        overflow_item(app, clipboard, ui, command.id);
                    }
                    ui.separator();
                }
            })
            .response
            .on_hover_text("Commands moved here to keep targets readable at this width");
        }
    });
}

fn more_label() -> String {
    format!("{} More", icon::DOTS_THREE)
}

/// Measured reservation for the More overflow button, so the budget tracks the
/// live fonts instead of a fixed guess.
fn more_button_width(ui: &Ui, measure: layout::Measure) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    measure(&more_label(), font) + ui.spacing().button_padding.x * 2.0 + ui.spacing().item_spacing.x
}

fn ribbon_group(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    title: &str,
    entries: Vec<&CommandDescriptor>,
    density: RibbonDensity,
    measure: layout::Measure,
) {
    let width = layout::group_width(title, &entries, density, measure);
    let tile = layout::tile_width(&entries, measure);
    ui.allocate_ui_with_layout(
        Vec2::new(
            width,
            if density == RibbonDensity::Full {
                TILE_HEIGHT + 16.0
            } else {
                ROW_HEIGHT + 16.0
            },
        ),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = if density == RibbonDensity::Full {
                    4.0
                } else {
                    2.0
                };
                for command in entries {
                    ribbon_button(app, clipboard, ui, command, density, tile, measure);
                }
            });
            ui.add_space(1.0);
            ui.add(
                Label::new(crate::typography::caption(title).color(ui.visuals().weak_text_color()))
                    .wrap_mode(TextWrapMode::Extend),
            );
        },
    );
}

fn update_button(app: &mut PlotxApp, ui: &mut Ui, compact: bool) {
    use plotx_core::update::UpdateStatus;
    match app.session.updates.status().clone() {
        UpdateStatus::Downloading { percent, .. } => {
            let text = if compact {
                percent.map_or_else(|| icon::ARROW_CLOCKWISE.to_owned(), |p| format!("{p}%"))
            } else {
                percent.map_or_else(|| "Updating…".to_owned(), |p| format!("Updating… {p}%"))
            };
            ui.label(
                RichText::new(text)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
        UpdateStatus::Installed { version, .. }
            if ui
                .button(if compact {
                    icon::ARROW_CLOCKWISE.to_owned()
                } else {
                    format!("{} Restart to update", icon::ARROW_CLOCKWISE)
                })
                .on_hover_text(format!(
                    "PlotX {version} is installed and ready after restart"
                ))
                .clicked() =>
        {
            crate::request_relaunch();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        _ => {}
    }
}
