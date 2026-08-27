//! Shared visual language for click-to-enter surfaces.
//!
//! Ribbon buttons signal clickability by tinting their leading glyph with the
//! theme accent (`Visuals::hyperlink_color`, see `ribbon_button`). Task-card
//! rows that open an editor or reveal content on click reuse the same colour
//! through these helpers, so "this text is clickable" reads identically on
//! every surface instead of each card inventing its own (or, worse, plain
//! text that gives no signal until hovered).

use egui::{Color32, Response, TextFormat, TextStyle, Ui, Visuals, text::LayoutJob};

/// The accent that marks a clickable surface — the exact colour the Ribbon
/// paints its enabled, unchecked button glyphs with.
pub(crate) fn clickable_tint(visuals: &Visuals) -> Color32 {
    visuals.hyperlink_color
}

/// A selectable row that reads as clickable while idle: the leading glyph
/// carries the clickable accent while the label keeps the theme text colour,
/// mirroring Ribbon buttons. A selected row falls back to the selection
/// styling wholesale so the accent never fights the checked state.
pub(crate) fn selectable_row(
    ui: &mut Ui,
    selected: bool,
    glyph: &str,
    label: impl Into<String>,
) -> Response {
    let font_id = TextStyle::Body.resolve(ui.style());
    let glyph_color = if selected {
        Color32::PLACEHOLDER
    } else {
        clickable_tint(ui.visuals())
    };
    let mut job = LayoutJob::default();
    job.append(
        glyph,
        0.0,
        TextFormat {
            font_id: font_id.clone(),
            color: glyph_color,
            ..Default::default()
        },
    );
    job.append(
        &format!("  {}", label.into()),
        0.0,
        TextFormat {
            font_id,
            color: Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    ui.selectable_label(selected, job)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clickable accent must stay the colour Ribbon glyphs use, in both
    /// themes, so every "this is clickable" mark reads as one language.
    #[test]
    fn clickable_tint_matches_the_ribbon_glyph_colour() {
        for visuals in [Visuals::light(), Visuals::dark()] {
            assert_eq!(clickable_tint(&visuals), visuals.hyperlink_color);
        }
    }
}
