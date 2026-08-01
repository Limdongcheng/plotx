use super::*;
use std::cell::Cell;

#[test]
fn highlight_falls_off_symmetrically_to_zero() {
    assert_eq!(sidebar_highlight_alpha(0.0), 1.0);
    assert_eq!(sidebar_highlight_alpha(SIDEBAR_HIGHLIGHT_RADIUS), 0.0);
    assert_eq!(
        sidebar_highlight_alpha(SIDEBAR_HIGHLIGHT_RADIUS + 20.0),
        0.0
    );

    let samples = [10.0, 30.0, 60.0, 80.0];
    for distance in samples {
        assert_eq!(
            sidebar_highlight_alpha(distance),
            sidebar_highlight_alpha(-distance)
        );
    }
    assert!(
        samples
            .windows(2)
            .all(|pair| { sidebar_highlight_alpha(pair[0]) > sidebar_highlight_alpha(pair[1]) })
    );
}

#[test]
fn central_margin_preserves_sidebar_gaps_only_when_present() {
    let both = central_workspace_margin(true, true);
    assert_eq!((both.left, both.right), (8, 8));

    let primary_only = central_workspace_margin(true, false);
    assert_eq!((primary_only.left, primary_only.right), (8, 4));

    let neither = central_workspace_margin(false, false);
    assert_eq!((neither.left, neither.right), (4, 4));
}

#[test]
fn resize_cursor_uses_two_point_radius_on_both_sidebar_edges() {
    for edge in [SidebarEdge::Left, SidebarEdge::Right] {
        assert_eq!(
            cursor_at_edge_offset(edge, 1.5),
            egui::CursorIcon::ResizeHorizontal
        );
        assert_eq!(cursor_at_edge_offset(edge, 2.5), egui::CursorIcon::Default);
    }
}

fn cursor_at_edge_offset(edge: SidebarEdge, offset: f32) -> egui::CursorIcon {
    let ctx = egui::Context::default();
    let screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0)));
    let boundary = Cell::new(0.0);
    let render = |events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect,
                events,
                ..Default::default()
            },
            |ui| {
                let panel = match edge {
                    SidebarEdge::Left => egui::Panel::right("test_sidebar"),
                    SidebarEdge::Right => egui::Panel::left("test_sidebar"),
                }
                .frame(egui::Frame::NONE)
                .default_size(100.0)
                .size_range(50.0..=200.0);
                let response =
                    show_resizable_sidebar(panel, ui, Id::new("test_sidebar"), edge, |ui| {
                        ui.set_min_size(ui.available_size());
                    });
                boundary.set(match edge {
                    SidebarEdge::Left => response.response.rect.left(),
                    SidebarEdge::Right => response.response.rect.right(),
                });
                paint_sidebar_resize_edge(
                    ui,
                    Id::new("test_sidebar"),
                    response.response.rect,
                    edge,
                    true,
                );
                egui::CentralPanel::default().show_inside(ui, |_| {});
            },
        )
    };

    let _ = render(Vec::new());
    let pointer_x = match edge {
        SidebarEdge::Left => boundary.get() - offset,
        SidebarEdge::Right => boundary.get() + offset,
    };
    render(vec![egui::Event::PointerMoved(Pos2::new(pointer_x, 150.0))])
        .platform_output
        .cursor_icon
}
