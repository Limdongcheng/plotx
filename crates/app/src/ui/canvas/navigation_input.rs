use egui::Vec2;
use plotx_core::state::ObjectId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum NavigationInput {
    None,
    TrackpadPan(Vec2),
    WheelZoom(f32),
    Pinch(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MacosTrackpadGesture {
    pub(super) active: bool,
    pub(super) suppressed: bool,
    pub(super) board_target: bool,
    pub(super) started: bool,
    pub(super) finished: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) enum TrackpadNavigationTarget {
    #[default]
    Board,
    Plot {
        canvas: usize,
        object: ObjectId,
    },
}

pub(super) fn classify_navigation_input(
    zoom_delta: f32,
    scroll_delta: Vec2,
    precise_scroll: bool,
) -> NavigationInput {
    if (zoom_delta - 1.0).abs() > 0.001 {
        NavigationInput::Pinch(zoom_delta)
    } else if precise_scroll && scroll_delta != Vec2::ZERO {
        NavigationInput::TrackpadPan(scroll_delta)
    } else if scroll_delta.y != 0.0 {
        NavigationInput::WheelZoom(scroll_delta.y)
    } else {
        NavigationInput::None
    }
}

pub(super) fn macos_trackpad_gesture(
    ctx: &egui::Context,
    events: &[egui::Event],
    pointer_owned: bool,
) -> MacosTrackpadGesture {
    #[cfg(target_os = "macos")]
    {
        #[derive(Clone, Copy, Default)]
        struct StoredGesture {
            active: bool,
            suppressed: bool,
            board_target: bool,
        }

        let state_id = egui::Id::new("plotx.macos_trackpad_gesture");
        ctx.data_mut(|data| {
            let mut stored = data.get_temp::<StoredGesture>(state_id).unwrap_or_default();
            let mut frame_target = stored.active.then_some(stored.board_target);
            let mut frame_suppressed = stored.suppressed;
            let mut started = false;
            let mut finished = false;
            for event in events {
                let egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase,
                    modifiers,
                    ..
                } = event
                else {
                    continue;
                };
                match phase {
                    egui::TouchPhase::Start => {
                        started = pointer_owned;
                        stored.active = pointer_owned;
                        stored.suppressed = !pointer_owned;
                        stored.board_target = modifiers.command || modifiers.ctrl;
                        frame_target = stored.active.then_some(stored.board_target);
                        frame_suppressed = stored.suppressed;
                    }
                    egui::TouchPhase::Move => {
                        if stored.active {
                            frame_target = Some(stored.board_target);
                        }
                        frame_suppressed = stored.suppressed;
                    }
                    egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                        finished = true;
                        if stored.active {
                            frame_target = Some(stored.board_target);
                        }
                        frame_suppressed = stored.suppressed;
                        stored = StoredGesture::default();
                    }
                }
            }
            if stored.active || stored.suppressed {
                data.insert_temp(state_id, stored);
            } else {
                data.remove_temp::<StoredGesture>(state_id);
            }
            MacosTrackpadGesture {
                active: frame_target.is_some(),
                suppressed: frame_suppressed,
                board_target: frame_target.unwrap_or(false),
                started,
                finished,
            }
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ctx, events, pointer_owned);
        MacosTrackpadGesture::default()
    }
}

pub(crate) fn sync_macos_trackpad_gesture(
    ctx: &egui::Context,
    events: &[egui::Event],
    pointer_owned: bool,
) {
    if pointer_owned {
        return;
    }
    #[cfg(target_os = "macos")]
    if events.iter().any(|event| {
        matches!(
            event,
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                ..
            }
        )
    }) {
        let gesture = macos_trackpad_gesture(ctx, events, false);
        if gesture.finished {
            ctx.data_mut(|data| {
                data.remove_temp::<TrackpadNavigationTarget>(egui::Id::new(
                    "plotx.trackpad_navigation_target",
                ));
            });
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (ctx, events);
}

pub(super) fn macos_precise_scroll_delta(events: &[egui::Event]) -> Vec2 {
    #[cfg(target_os = "macos")]
    {
        events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta,
                    ..
                } => Some(*delta),
                _ => None,
            })
            .fold(Vec2::ZERO, |sum, delta| sum + delta)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = events;
        Vec2::ZERO
    }
}
