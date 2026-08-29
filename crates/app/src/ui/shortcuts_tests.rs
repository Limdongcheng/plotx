use super::*;

/// Two dispatchable bindings must never share an effective chord. The
/// matcher ignores Shift for plain keys, so those normalize shift away.
#[test]
fn dispatchable_chords_are_unambiguous() {
    let mut seen = std::collections::HashSet::new();
    for binding in BINDINGS.iter().filter(|binding| binding.dispatch) {
        for chord in std::iter::once(binding.primary).chain(binding.aliases.iter().copied()) {
            assert!(
                seen.insert((chord.command, chord.command && chord.shift, chord.key)),
                "chord {chord:?} bound twice"
            );
        }
    }
}

#[test]
fn labels_derive_from_the_binding_table() {
    let label = shortcut_label(commands::CommandId::SaveProject).unwrap();
    assert!(label.ends_with("+S"));
    assert!(
        shortcut_label(commands::CommandId::PasteImage).is_some_and(|label| label.ends_with("+V"))
    );
    assert_eq!(
        shortcut_label(commands::CommandId::Tool(Tool::Select)).as_deref(),
        Some("V")
    );
    assert_eq!(
        shortcut_label(commands::CommandId::CycleCursor).as_deref(),
        Some("C")
    );
    assert!(shortcut_label(commands::CommandId::Tool(Tool::Symmetry)).is_none());
    assert!(shortcut_label(commands::CommandId::About).is_none());
}

fn paste_key_event() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::V,
        physical_key: Some(egui::Key::V),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    }
}

#[test]
fn restored_ctrl_v_and_platform_paste_events_route_to_paste_image() {
    for event in [
        paste_key_event(),
        egui::Event::Paste("clipboard".to_owned()),
    ] {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![event],
            modifiers: egui::Modifiers::CTRL,
            ..Default::default()
        };
        let mut command = None;
        let _ = ctx.run_ui(input, |ui| command = shortcut_command(ui.ctx()));
        assert_eq!(command, Some(commands::CommandId::PasteImage));
    }
}

#[test]
fn focused_text_edit_keeps_ctrl_v_for_text_paste() {
    let ctx = egui::Context::default();
    let mut text = String::new();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut text))
            .request_focus();
    });
    let input = egui::RawInput {
        events: vec![egui::Event::Paste("text".to_owned())],
        modifiers: egui::Modifiers::CTRL,
        ..Default::default()
    };
    let mut command = None;
    let _ = ctx.run_ui(input, |ui| {
        command = shortcut_command(ui.ctx());
        ui.add(egui::TextEdit::singleline(&mut text));
    });
    assert_eq!(command, None);
    assert_eq!(text, "text");
}

fn f_key_event() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::F,
        physical_key: Some(egui::Key::F),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

/// Plain `F` is context-split: over a plot's data area it fits that plot's
/// data viewport, elsewhere it keeps the board Zoom-to-Selection meaning.
#[test]
fn plain_f_fits_the_plot_under_the_pointer_and_the_board_otherwise() {
    let (mut app, ids) = crate::ui::properties::fixture::contour_page(1);
    app.session.board = plotx_core::state::BoardViewport {
        zoom: 1.0,
        world_center: [500.0, 400.0],
    };
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0));
    let plot = canvas::plot_inner_rect(&app, 0, ids[0], screen).expect("plot on the board");
    let inside = egui::Pos2::new(
        (plot.left + plot.right()) * 0.5,
        (plot.top + plot.bottom()) * 0.5,
    );

    // Zoom the data viewport away from the full range first.
    let plot_object = app.doc.canvases[0]
        .object_mut(ids[0])
        .and_then(|object| object.plot_mut())
        .expect("fixture plot");
    let full_x = plot_object.viewport.full_x;
    let full_y = plot_object.viewport.full_y;
    plot_object.viewport.view_x = plotx_core::state::AxisRange::new(
        full_x.min + full_x.span() * 0.25,
        full_x.max - full_x.span() * 0.25,
    );
    plot_object.apply_viewport();

    let ctx = egui::Context::default();
    let mut clipboard = clipboard_table::ClipboardTablePaste::default();
    let mut frame = |app: &mut PlotxApp, pointer: egui::Pos2| {
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(pointer), f_key_event()],
                ..Default::default()
            },
            |ui| {
                canvas::store_navigation_rect(ui.ctx(), screen);
                handle_plot_fit_shortcut(app, &mut clipboard, ui.ctx());
            },
        );
    };

    frame(&mut app, inside);
    let viewport = app.doc.canvases[0]
        .object(ids[0])
        .and_then(|object| object.plot())
        .expect("fixture plot")
        .viewport
        .clone();
    assert_eq!(viewport.view_x, full_x);
    assert_eq!(viewport.view_y, full_y);
    assert_eq!(app.session.status, "Fit plot to the full data range.");

    // Outside any plot the chord still fits the board to the selection.
    frame(&mut app, egui::Pos2::new(5.0, 5.0));
    assert!(matches!(
        app.session.viewport_mode,
        plotx_core::state::ViewportMode::Fit(_)
    ));
}

#[test]
fn escape_exits_an_active_tool_after_other_fallbacks() {
    let mut app = PlotxApp::new_with_settings(plotx_core::settings::Settings::default());
    app.set_tool(Tool::Integrate);

    handle_escape(&mut app, 0.0);

    assert_eq!(app.session.tool, Tool::BrowseZoom);
    assert_eq!(app.session.status, "Exited tool mode.");
}

#[test]
fn escape_finishes_a_pending_wheel_property_gesture() {
    let mut app = PlotxApp::new_with_settings(plotx_core::settings::Settings::default());
    app.session.ui.wheel_property = Some(plotx_core::actions::PendingWheelPropertyEdit {
        canvas: 0,
        object: plotx_core::state::ObjectId::new(1),
        property: plotx_core::properties::contour::BASE_MAGNITUDE,
        targets: Vec::new(),
        accumulator: 0.0,
        last_input_time: 0.0,
        gesture_started: false,
    });

    handle_escape(&mut app, 1.0);

    assert!(app.session.ui.wheel_property.is_none());
    assert_eq!(app.session.status, "Cancelled interaction.");
}
