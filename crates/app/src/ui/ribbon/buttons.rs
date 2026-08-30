//! Individual Ribbon command widgets: the density-dependent buttons, their
//! short labels, and the overflow-menu rows.

use egui::text::LayoutJob;
use egui::{Align2, Button, Color32, FontId, RichText, TextFormat, Ui, Vec2};
use plotx_core::actions::ZOrder;
use plotx_core::export::ExportFormat;
use plotx_core::state::{PlotxApp, Tool};

use super::super::clipboard_table::ClipboardTablePaste;
use super::super::commands::{self, CommandDescriptor, CommandId};
use super::RibbonDensity;
use super::layout::{Measure, ROW_HEIGHT, TILE_HEIGHT, button_width};

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

pub(super) fn ribbon_button(
    app: &mut PlotxApp,
    clipboard: &mut ClipboardTablePaste,
    ui: &mut Ui,
    command: &CommandDescriptor,
    density: RibbonDensity,
    tile: f32,
    measure: Measure,
) {
    let label = short_label(command);
    // Icons carry the accent colour; label text keeps the theme colour via the
    // placeholder, which also inherits the correct disabled/selected colours.
    let icon_color = if command.enabled && command.checked != Some(true) {
        ui.visuals().hyperlink_color
    } else {
        Color32::PLACEHOLDER
    };
    let mut job = LayoutJob::default();
    let response = if density == RibbonDensity::Full {
        let icon_font = FontId::proportional(16.0);
        let label_font = crate::typography::subheadline_font();
        let selected = command.checked == Some(true);
        // Keep the command name in the button for accessibility, but paint the
        // two visible rows ourselves so both share the tile's exact centre.
        // LayoutJob's per-row offsets otherwise make differently sized glyphs
        // appear alternately left- and right-aligned.
        let button = Button::selectable(
            selected,
            RichText::new(&label).size(1.0).color(Color32::TRANSPARENT),
        )
        .min_size(Vec2::new(tile, TILE_HEIGHT));
        let response = ui.add_enabled(command.enabled, button);
        let text_color = ui
            .style()
            .button_style(response.widget_state(), selected)
            .text_style
            .color;
        let center = response.rect.center();
        if let Some(icon) = command.icon {
            ui.painter().text(
                center - Vec2::new(0.0, 7.5),
                Align2::CENTER_CENTER,
                icon,
                icon_font,
                if command.enabled && !selected {
                    icon_color
                } else {
                    text_color
                },
            );
            ui.painter().text(
                center + Vec2::new(0.0, 9.0),
                Align2::CENTER_CENTER,
                &label,
                label_font,
                text_color,
            );
        } else {
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                &label,
                label_font,
                text_color,
            );
        }
        response
    } else {
        if let Some(icon) = command.icon {
            job.append(
                icon,
                0.0,
                TextFormat {
                    font_id: FontId::proportional(14.0),
                    color: icon_color,
                    ..Default::default()
                },
            );
        } else {
            job.append(
                &label,
                0.0,
                TextFormat {
                    font_id: crate::typography::callout_font(),
                    color: Color32::PLACEHOLDER,
                    ..Default::default()
                },
            );
        }
        let button = Button::selectable(command.checked == Some(true), job)
            .min_size(Vec2::new(button_width(command, measure), ROW_HEIGHT));
        ui.add_enabled(command.enabled, button)
    };
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
