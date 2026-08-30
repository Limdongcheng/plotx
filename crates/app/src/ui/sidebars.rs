use super::*;

const MIN_WORKSPACE_WIDTH: f32 = 320.0;
/// Releasing a resize drag with the pointer this far past the sidebar's
/// minimum width hides the sidebar. Wide enough that overshooting a fast
/// resize does not hide it by accident.
const HIDE_DRAG_SLACK: f32 = 60.0;

pub(super) fn render(app: &mut PlotxApp, ui: &mut Ui, dark: bool, workspace_width: f32) {
    let mut primary_rect = None;
    let mut secondary_rect = None;
    let compact = workspace_width < 1200.0;
    let inspector_visible = app.session.secondary_sidebar_visible;
    if !inspector_visible {
        app.finish_axis_overrides_edit();
    }
    object_inspector::finish_series_edit_if_inactive(app, inspector_visible);
    if app.session.primary_sidebar_visible {
        let min_width = if compact { 150.0 } else { 190.0 };
        let other_width = if app.session.secondary_sidebar_visible {
            app.session.secondary_sidebar_width
        } else {
            0.0
        };
        let max_width =
            (workspace_width - other_width - MIN_WORKSPACE_WIDTH).clamp(min_width, 420.0);
        let panel = egui::Panel::left("primary_sidebar")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 8,
                right: 0,
                top: 4,
                bottom: 8,
            }))
            .show_separator_line(false)
            .resizable(true)
            .default_size(
                app.session
                    .primary_sidebar_width
                    .clamp(min_width, max_width),
            )
            .size_range(min_width..=max_width);
        let response = show_sidebar(panel, app, ui, dark, true);
        paint_sidebar_resize_edge(
            ui,
            Id::new("primary_sidebar"),
            response.inner,
            SidebarEdge::Right,
            dark,
        );
        app.session.primary_sidebar_width = response.response.rect.width();
        primary_rect = Some(response.inner);
        if resize_dragged_past_min(
            ui.ctx(),
            Id::new("primary_sidebar"),
            response.inner,
            SidebarEdge::Right,
        ) {
            app.session.primary_sidebar_visible = false;
            sidebar_hidden_status(app, commands::CommandId::TogglePrimarySidebar, "Left");
        }
    } else {
        // A shown panel consumes one auto-id slot from this ui. Burn the same
        // slot while hidden so the later siblings' container ids (secondary
        // panel, central panel) do not shift when a sidebar toggles.
        ui.skip_ahead_auto_ids(1);
    }

    if app.session.secondary_sidebar_visible {
        let min_width = if compact { 180.0 } else { 230.0 };
        let other_width = if app.session.primary_sidebar_visible {
            app.session.primary_sidebar_width
        } else {
            0.0
        };
        let max_width =
            (workspace_width - other_width - MIN_WORKSPACE_WIDTH).clamp(min_width, 460.0);
        let panel = egui::Panel::right("secondary_sidebar")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 0,
                right: 8,
                top: 4,
                bottom: 8,
            }))
            .show_separator_line(false)
            .resizable(true)
            .default_size(
                app.session
                    .secondary_sidebar_width
                    .clamp(min_width, max_width),
            )
            .size_range(min_width..=max_width);
        let response = show_sidebar(panel, app, ui, dark, false);
        paint_sidebar_resize_edge(
            ui,
            Id::new("secondary_sidebar"),
            response.inner,
            SidebarEdge::Left,
            dark,
        );
        app.session.secondary_sidebar_width = response.response.rect.width();
        secondary_rect = Some(response.inner);
        if resize_dragged_past_min(
            ui.ctx(),
            Id::new("secondary_sidebar"),
            response.inner,
            SidebarEdge::Left,
        ) {
            app.session.secondary_sidebar_visible = false;
            sidebar_hidden_status(app, commands::CommandId::ToggleSecondarySidebar, "Right");
        }
    } else {
        // See the primary branch: keep the sibling auto-id sequence stable.
        ui.skip_ahead_auto_ids(1);
    }
    super::workspace_geometry::set_sidebar_rects(ui.ctx(), primary_rect, secondary_rect);
}

/// True when a resize drag on `panel_id` just ended with the pointer well past
/// the sidebar's minimum width — the user pulled the edge "through" the
/// sidebar. egui clamps the panel at its minimum during the drag, so the
/// overshoot is the pointer-to-edge distance on release.
fn resize_dragged_past_min(
    ctx: &egui::Context,
    panel_id: Id,
    card_rect: Rect,
    edge: SidebarEdge,
) -> bool {
    let Some(resize) = ctx.read_response(panel_id.with("__resize")) else {
        return false;
    };
    if !resize.drag_stopped() {
        return false;
    }
    let Some(pointer) = resize
        .interact_pointer_pos()
        .or_else(|| ctx.pointer_interact_pos())
    else {
        return false;
    };
    match edge {
        SidebarEdge::Right => pointer.x < card_rect.right() - HIDE_DRAG_SLACK,
        SidebarEdge::Left => pointer.x > card_rect.left() + HIDE_DRAG_SLACK,
    }
}

fn sidebar_hidden_status(app: &mut PlotxApp, id: commands::CommandId, side: &str) {
    let recovery = match shortcuts::shortcut_label(id) {
        Some(chord) => format!("the layout buttons in the title row or {chord}"),
        None => "the layout buttons in the title row".to_owned(),
    };
    app.session.status = format!("{side} sidebar hidden. Show it again with {recovery}.");
}

fn show_sidebar(
    panel: egui::Panel,
    app: &mut PlotxApp,
    ui: &mut Ui,
    dark: bool,
    primary: bool,
) -> InnerResponse<Rect> {
    let (id, edge) = if primary {
        (Id::new("primary_sidebar"), SidebarEdge::Right)
    } else {
        (Id::new("secondary_sidebar"), SidebarEdge::Left)
    };
    show_resizable_sidebar(panel, ui, id, edge, |ui| {
        // Anchor the content ids globally: a Ui's per-pass unique id folds in
        // the parent's auto-id counter, so without this every widget in this
        // sidebar changes id whenever an earlier sibling panel toggles. That
        // dropped focus mid-edit and tripped egui's rect-changed-id debug
        // overlay (one-frame red boxes) on the unmoved sidebar.
        ui.scope_builder(
            UiBuilder::new()
                .id_salt(id.with("stable_scope"))
                .global_scope(true),
            |ui| {
                let size = ui.available_size();
                let frame = card_frame(dark, egui::Margin::ZERO);
                let inset = frame.total_margin().sum();
                frame
                    .show(ui, |ui| {
                        ui.set_min_size((size - inset).max(Vec2::ZERO));
                        if primary {
                            primary_sidebar::render(app, ui);
                        } else {
                            secondary_sidebar::render(app, ui);
                        }
                    })
                    .response
                    .rect
            },
        )
        .inner
    })
}
