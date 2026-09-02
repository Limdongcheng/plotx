//! Individual Ribbon command widgets: the scale-dependent buttons, their
//! short labels, the Collapsed group tile, and the overflow-menu rows.

use egui::text::LayoutJob;
use egui::{Align2, Button, Color32, FontId, Response, RichText, Stroke, TextFormat, Ui, Vec2};
use plotx_core::actions::ZOrder;
use plotx_core::export::ExportFormat;
use plotx_core::layout::SpacingMode;
use plotx_core::state::{PlotxApp, Tool};

use super::super::clipboard_table::ClipboardTablePaste;
use super::super::commands::{self, CommandDescriptor, CommandId};
use super::layout::{GroupScale, Measure, STACK_ROW_HEIGHT, TILE_HEIGHT, segment_width};

/// Ribbon buttons carry short verb labels; the full command name and shortcut
/// stay in the tooltip, menus and the command palette.
pub(super) fn short_label(command: &CommandDescriptor) -> String {
    match command.id {
        CommandId::NewCanvas(index) => match index {
            0 => "Slides",
            1 => "1 Column",
            2 => "2 Columns",
            3 => "Poster",
            _ => "Canvas",
        }
        .to_owned(),
        CommandId::ChartType => "Chart".to_owned(),
        CommandId::ApplyTheme(id) => match id {
            "publication" => "Paper",
            "presentation_dark" => "Dark",
            "vibrant" => "Vibrant",
            _ => "Theme",
        }
        .to_owned(),
        CommandId::CopyFigure => "Copy".to_owned(),
        CommandId::Export(format) => match format {
            ExportFormat::Png => "PNG",
            ExportFormat::Svg => "SVG",
            _ => format.label(),
        }
        .to_owned(),
        CommandId::ImportTable => "Import Table".to_owned(),
        CommandId::ImportImage => "Add Images".to_owned(),
        CommandId::ImportImageFirstFrame => "First Frame".to_owned(),
        CommandId::PasteTable => "Paste Table".to_owned(),
        CommandId::NewTable => "New Table".to_owned(),
        CommandId::StackData => "Stack Data".to_owned(),
        CommandId::SaveProcessingTemplate => "Save Template".to_owned(),
        CommandId::ApplyProcessingTemplate => "Apply Template".to_owned(),
        CommandId::SpectrumArithmetic => "Arithmetic".to_owned(),
        CommandId::AlignSpectra => "Align Spectra".to_owned(),
        CommandId::TidyBoard => "Tidy Frames".to_owned(),
        CommandId::ToggleSnap => "Snapping".to_owned(),
        CommandId::TogglePrimarySidebar => "Left Bar".to_owned(),
        CommandId::ToggleSecondarySidebar => "Right Bar".to_owned(),
        CommandId::ArrangeGrid(rows, cols) => format!("Plots {rows} × {cols}"),
        // Segment captions: the family's shared "Spacing:" / "Minimum
        // spacing:" prefix lives in the tooltip, not on every segment.
        CommandId::SetSpacingMode(mode) => match mode {
            SpacingMode::Frame => "Frame",
            SpacingMode::Visual => "Visual",
        }
        .to_owned(),
        CommandId::SetGutterPreset(preset) => preset.label().to_owned(),
        CommandId::ZOrder(mode) => match mode {
            ZOrder::Front => "To Front",
            ZOrder::Forward => "Forward",
            ZOrder::Backward => "Backward",
            ZOrder::Back => "To Back",
        }
        .to_owned(),
        CommandId::Align(_) => command.label.trim_start_matches("Align ").to_owned(),
        CommandId::Distribute(_) => command.label.trim_start_matches("Distribute ").to_owned(),
        // A Ribbon tile shows the group's own short name; the full "… settings"
        // wording stays in the tooltip, the menus and the palette.
        CommandId::PropertyGroup(section) => super::super::properties::discovery::group(section)
            .map(|group| group.label.get().to_owned())
            .unwrap_or_else(|| "Settings".to_owned()),
        CommandId::Tool(Tool::BrowseZoom) => "Zoom".to_owned(),
        CommandId::Tool(_) => command.label.trim_start_matches("Tool: ").to_owned(),
        _ => command.label.clone(),
    }
}

/// One command at `scale`, painted `width` wide so the cells of a column
/// line up. Large paints an icon-over-label tile; Medium an icon-beside-label
/// row; Small an icon square, falling back to the Medium row for a command
/// with no icon so it never loses its name.
pub(super) fn ribbon_button(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    command: &CommandDescriptor,
    scale: GroupScale,
    width: f32,
) {
    let label = short_label(command);
    let primary = primary_run(command.id) && command.enabled;
    let selected = command.checked == Some(true);
    let paint = Paint::for_command(ui, command, primary);
    let response = match (scale, command.icon) {
        (GroupScale::Large, icon) => tile(
            ui,
            &paint,
            primary,
            selected,
            command.enabled,
            width,
            icon,
            &label,
        ),
        (GroupScale::Small, Some(icon)) => {
            let button = paint.frame(
                Button::selectable(
                    selected,
                    RichText::new(icon).size(13.0).color(paint.icon_color),
                ),
                primary,
            );
            ui.add_enabled(
                command.enabled,
                button.min_size(Vec2::new(width, STACK_ROW_HEIGHT)),
            )
        }
        (GroupScale::Medium | GroupScale::Small | GroupScale::Collapsed, icon) => {
            let mut job = LayoutJob::default();
            if let Some(icon) = icon {
                // The glyph matches the 12 pt label so a row stays within
                // its 22 px stack slot.
                job.append(
                    icon,
                    0.0,
                    TextFormat {
                        font_id: FontId::proportional(12.0),
                        color: paint.icon_color,
                        ..Default::default()
                    },
                );
                job.append(
                    &format!("  {label}"),
                    0.0,
                    TextFormat {
                        font_id: crate::typography::callout_font(),
                        color: paint.label_color,
                        ..Default::default()
                    },
                );
            } else {
                job.append(
                    &label,
                    0.0,
                    TextFormat {
                        font_id: crate::typography::callout_font(),
                        color: paint.label_color,
                        ..Default::default()
                    },
                );
            }
            let button = paint.frame(Button::selectable(selected, job), primary);
            ui.add_enabled(
                command.enabled,
                button.min_size(Vec2::new(width, STACK_ROW_HEIGHT)),
            )
        }
    };
    respond(app, clipboard, ui, command, response);
}

/// The Collapsed group tile: the group's lead icon over its title and a
/// caret. The caller opens the group's Large layout from the response.
pub(super) fn collapsed_tile(
    ui: &mut Ui,
    title: &str,
    icon: Option<&str>,
    width: f32,
    open: bool,
) -> Response {
    let paint = Paint {
        icon_color: ui.visuals().hyperlink_color,
        label_color: Color32::PLACEHOLDER,
        primary_text: ui.visuals().selection.stroke.color,
        primary_fill: ui.visuals().selection.bg_fill,
    };
    let label = super::layout::collapsed_label(title);
    tile(ui, &paint, false, open, true, width, icon, &label)
        .on_hover_text(format!("{title} commands"))
}

/// Colours of one button: the accent glyph, the placeholder label that
/// inherits the widget state, and the fill and text of a primary button.
struct Paint {
    icon_color: Color32,
    label_color: Color32,
    primary_text: Color32,
    primary_fill: Color32,
}

impl Paint {
    fn for_command(ui: &Ui, command: &CommandDescriptor, primary: bool) -> Self {
        let primary_text = ui.visuals().selection.stroke.color;
        // Icons carry the accent colour whenever the command is available — a
        // checked toggle keeps it, so the active tool never reads weaker than
        // an idle one. Labels use the placeholder so they inherit the correct
        // disabled/selected colours from the widget state.
        let icon_color = if primary {
            primary_text
        } else if command.enabled {
            ui.visuals().hyperlink_color
        } else {
            Color32::PLACEHOLDER
        };
        Self {
            icon_color,
            label_color: if primary {
                primary_text
            } else {
                Color32::PLACEHOLDER
            },
            primary_text,
            primary_fill: ui.visuals().selection.bg_fill,
        }
    }

    /// One-shot Run commands render as filled primary buttons — the same
    /// accent treatment as a task card's Run button — so "executes the
    /// computation" and "switches a mode" stop sharing one resting look.
    fn frame<'a>(&self, button: Button<'a>, primary: bool) -> Button<'a> {
        if primary {
            // `selectable(false)` hides the resting frame; a primary button
            // must keep it or the fill never paints.
            button
                .frame_when_inactive(true)
                .fill(self.primary_fill)
                .stroke(Stroke::NONE)
        } else {
            button
        }
    }
}

/// An icon-over-label tile. Keeps the label in the button for accessibility,
/// but paints the two visible rows itself so both share the tile's exact
/// centre: LayoutJob's per-row offsets otherwise make differently sized
/// glyphs appear alternately left- and right-aligned.
#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut Ui,
    paint: &Paint,
    primary: bool,
    selected: bool,
    enabled: bool,
    width: f32,
    icon: Option<&str>,
    label: &str,
) -> Response {
    let button = paint.frame(
        Button::selectable(
            selected,
            RichText::new(label).size(1.0).color(Color32::TRANSPARENT),
        )
        .min_size(Vec2::new(width, TILE_HEIGHT)),
        primary,
    );
    let response = ui.add_enabled(enabled, button);
    let text_color = if primary {
        paint.primary_text
    } else {
        ui.style()
            .button_style(response.widget_state(), selected)
            .text_style
            .color
    };
    let center = response.rect.center();
    let label_font = crate::typography::subheadline_font();
    if let Some(icon) = icon {
        ui.painter().text(
            center - Vec2::new(0.0, 7.5),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.0),
            if enabled {
                paint.icon_color
            } else {
                text_color
            },
        );
        ui.painter().text(
            center + Vec2::new(0.0, 9.0),
            Align2::CENTER_CENTER,
            label,
            label_font,
            text_color,
        );
    } else {
        ui.painter()
            .text(center, Align2::CENTER_CENTER, label, label_font, text_color);
    }
    response
}

fn primary_run(id: CommandId) -> bool {
    matches!(
        id,
        CommandId::RunPeakFit | CommandId::RunCurveFit | CommandId::RunCraft
    )
}

/// A mutually exclusive command family as one segmented control: adjacent
/// framed segments with exactly one selected, replacing a row of look-alike
/// buttons that gave no hint the choices exclude each other.
pub(super) fn segmented_run(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    entries: &[&CommandDescriptor],
    measure: Measure,
    height: f32,
) {
    // Segments sit side by side whatever layout hosts the run: at Large the
    // group row, at Medium and Small a stacked column.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for command in entries {
            let selected = command.checked == Some(true);
            let button =
                Button::selectable(selected, crate::typography::callout(short_label(command)))
                    .frame_when_inactive(true)
                    .min_size(Vec2::new(segment_width(command, measure), height));
            let response = ui.add_enabled(command.enabled, button);
            respond(app, clipboard, ui, command, response);
        }
    });
}

/// Shared tail of every Ribbon command widget: the full-name tooltip (with
/// shortcut and, when disabled, the unblock reason) and catalog execution.
fn respond(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &Ui,
    command: &CommandDescriptor,
    response: Response,
) {
    let tip = match &command.shortcut {
        Some(shortcut) => format!("{} ({shortcut})", command.label),
        None => command.label.clone(),
    };
    let clicked = response.clicked();
    if command.enabled {
        response.on_hover_text(tip);
    } else {
        let reason = command
            .disabled_reason
            .unwrap_or("Unavailable in the current context.");
        response.on_disabled_hover_text(format!("{tip} · {reason}"));
    }
    if clicked {
        commands::execute(command.id, app, clipboard, ui.ctx());
    }
}

pub(super) fn overflow_item(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    id: CommandId,
) {
    let command = commands::describe(app, id);
    let mut button = Button::new(&command.label).selected(command.checked == Some(true));
    if let Some(shortcut) = &command.shortcut {
        button = button.shortcut_text(shortcut);
    }
    let response = ui.add_enabled(command.enabled, button);
    let clicked = response.clicked();
    if !command.enabled
        && let Some(reason) = command.disabled_reason
    {
        response.on_disabled_hover_text(reason);
    }
    if clicked {
        commands::execute(id, app, clipboard, ui.ctx());
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::commands;
    use super::*;
    use plotx_core::state::PlotxApp;

    #[test]
    fn figure_tiles_use_short_labels() {
        let app = PlotxApp::new_with_settings(plotx_core::settings::Settings::default());
        let cases = [
            (CommandId::NewCanvas(0), "Slides"),
            (CommandId::NewCanvas(1), "1 Column"),
            (CommandId::NewCanvas(2), "2 Columns"),
            (CommandId::NewCanvas(3), "Poster"),
            (CommandId::ChartType, "Chart"),
            (CommandId::ApplyTheme("publication"), "Paper"),
            (CommandId::ApplyTheme("presentation_dark"), "Dark"),
            (CommandId::ApplyTheme("vibrant"), "Vibrant"),
            (CommandId::CopyFigure, "Copy"),
            (CommandId::Export(ExportFormat::Png), "PNG"),
            (CommandId::Export(ExportFormat::Svg), "SVG"),
        ];
        for (id, expected) in cases {
            assert_eq!(short_label(&commands::describe(&app, id)), expected);
        }
    }
}
