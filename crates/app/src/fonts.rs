use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

const SYSTEM_UI_KEY: &str = "plotx-system-ui";
const SYSTEM_CJK_KEY: &str = "plotx-system-cjk";

struct LoadedFont {
    bytes: Vec<u8>,
    index: u32,
}

#[derive(Default)]
struct PlatformFonts {
    system_ui: Option<LoadedFont>,
    system_cjk: Option<LoadedFont>,
}

pub(crate) fn definitions() -> FontDefinitions {
    #[cfg(target_os = "macos")]
    let platform = load_macos_fonts();
    #[cfg(not(target_os = "macos"))]
    let platform = PlatformFonts::default();

    build_definitions(platform)
}

fn build_definitions(platform: PlatformFonts) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let has_platform_font = platform.system_ui.is_some() || platform.system_cjk.is_some();

    if let Some(font) = platform.system_cjk {
        insert_proportional(&mut fonts, SYSTEM_CJK_KEY, font);
    }
    if let Some(font) = platform.system_ui {
        insert_proportional(&mut fonts, SYSTEM_UI_KEY, font);
    }

    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    if has_platform_font {
        prioritize_platform_fonts(&mut fonts);
    }
    fonts
}

fn insert_proportional(fonts: &mut FontDefinitions, key: &str, font: LoadedFont) {
    let mut data = FontData::from_owned(font.bytes);
    data.index = font.index;
    fonts.font_data.insert(key.to_owned(), Arc::new(data));
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("egui default fonts include a proportional family")
        .insert(0, key.to_owned());
}

fn prioritize_platform_fonts(fonts: &mut FontDefinitions) {
    let proportional = fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("egui default fonts include a proportional family");
    for key in [SYSTEM_CJK_KEY, "phosphor", SYSTEM_UI_KEY] {
        if let Some(index) = proportional.iter().position(|name| name == key) {
            let name = proportional.remove(index);
            proportional.insert(0, name);
        }
    }
}

#[cfg(target_os = "macos")]
fn load_macos_fonts() -> PlatformFonts {
    use fontdb::{Database, Family, Query, Weight};

    let mut db = Database::new();
    db.load_system_fonts();

    let load = |label: &str, families: &[Family<'_>]| {
        let result = db
            .query(&Query {
                families,
                weight: Weight::NORMAL,
                ..Query::default()
            })
            .ok_or_else(|| format!("could not find {label}"))
            .and_then(|id| {
                db.with_face_data(id, |bytes, index| LoadedFont {
                    bytes: bytes.to_vec(),
                    index,
                })
                .ok_or_else(|| format!("could not read {label}"))
            });

        match result {
            Ok(font) => Some(font),
            Err(error) => {
                eprintln!("PlotX font fallback: {error}; using the remaining font stack");
                None
            }
        }
    };

    PlatformFonts {
        system_ui: load(
            "the macOS system UI font",
            &[Family::Name("System Font"), Family::Name(".SF NS")],
        ),
        system_cjk: load("Hiragino Sans GB", &[Family::Name("Hiragino Sans GB")]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{FontDefinitions, FontFamily};

    fn fake_font() -> LoadedFont {
        LoadedFont {
            bytes: vec![0],
            index: 3,
        }
    }

    #[test]
    fn system_fonts_have_the_intended_proportional_priority() {
        let fonts = build_definitions(PlatformFonts {
            system_ui: Some(fake_font()),
            system_cjk: Some(fake_font()),
        });

        assert_eq!(
            &fonts.families[&FontFamily::Proportional][..3],
            [SYSTEM_UI_KEY, "phosphor", SYSTEM_CJK_KEY]
        );
        assert_eq!(fonts.font_data[SYSTEM_UI_KEY].index, 3);
        assert_eq!(fonts.font_data[SYSTEM_CJK_KEY].index, 3);
    }

    #[test]
    fn platform_fonts_do_not_change_monospace() {
        let defaults = FontDefinitions::default();
        let fonts = build_definitions(PlatformFonts {
            system_ui: Some(fake_font()),
            system_cjk: Some(fake_font()),
        });

        assert_eq!(
            fonts.families[&FontFamily::Monospace],
            defaults.families[&FontFamily::Monospace]
        );
    }

    #[test]
    fn missing_platform_fonts_preserve_portable_fallbacks() {
        let fonts = build_definitions(PlatformFonts::default());
        let proportional = &fonts.families[&FontFamily::Proportional];

        assert_eq!(proportional[0], "Ubuntu-Light");
        assert_eq!(proportional[1], "phosphor");
    }

    #[test]
    fn cjk_fallback_never_preempts_phosphor_when_system_ui_is_missing() {
        let fonts = build_definitions(PlatformFonts {
            system_ui: None,
            system_cjk: Some(fake_font()),
        });
        let proportional = &fonts.families[&FontFamily::Proportional];
        let phosphor = proportional
            .iter()
            .position(|name| name == "phosphor")
            .unwrap();
        let system_cjk = proportional
            .iter()
            .position(|name| name == SYSTEM_CJK_KEY)
            .unwrap();

        assert!(phosphor < system_cjk);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_system_faces_load_with_expected_glyph_coverage() {
        use ab_glyph::{Font, FontRef};

        let platform = load_macos_fonts();
        let system_ui = platform.system_ui.expect("macOS system UI font");
        let system_cjk = platform.system_cjk.expect("macOS Simplified Chinese font");
        let system_ui = FontRef::try_from_slice_and_index(&system_ui.bytes, system_ui.index)
            .expect("valid macOS system UI face");
        let system_cjk = FontRef::try_from_slice_and_index(&system_cjk.bytes, system_cjk.index)
            .expect("valid macOS Simplified Chinese face");

        assert_ne!(system_ui.glyph_id('A').0, 0);
        for ch in ['中', '文', '图'] {
            assert_ne!(system_cjk.glyph_id(ch).0, 0, "missing glyph {ch}");
        }
    }
}
