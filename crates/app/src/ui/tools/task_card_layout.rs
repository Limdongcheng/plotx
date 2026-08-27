use egui::{Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HorizontalAnchor {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VerticalAnchor {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CardLayout {
    /// The rectangle actually rendered this frame: `preferred` fitted into the
    /// current workspace bounds.
    pub rect: Rect,
    /// The user-intended rectangle, written only by gestures (drag, resize) and
    /// by boundary-following of a flush edge. It may lie outside the current
    /// bounds; keeping it lets a card that a sidebar pushed aside return to its
    /// place when the sidebar hides again.
    pub preferred: Rect,
    pub bounds: Rect,
    pub horizontal: HorizontalAnchor,
    pub vertical: VerticalAnchor,
    pub chrome_height: f32,
    pub extra_width: f32,
    pub collapsed: bool,
}

/// Distance within which a card edge counts as resting on a workspace boundary.
/// A resting edge keeps following that boundary (the default top-right docking
/// across sidebar toggles); a card parked anywhere else keeps its absolute
/// position and is only clamped back inside the new bounds.
const FLUSH_EPS: f32 = 1.0;

/// Rebuild a card from its preferred rectangle, its preferred size and the
/// edges fixed by the user's last gesture. Viewport fitting may temporarily
/// move or shrink the rendered rectangle, but never mutates the preference:
/// bounds changes must not teleport a parked card (only a flush edge follows
/// its boundary), and a card displaced by a shrinking boundary returns once
/// the boundary recedes.
pub(super) fn fit_layout(
    mut layout: CardLayout,
    bounds: Rect,
    desired_size: Vec2,
    collapsed: bool,
) -> CardLayout {
    let mut preferred = layout.preferred;
    match layout.horizontal {
        HorizontalAnchor::Left => {
            if (preferred.left() - layout.bounds.left()).abs() <= FLUSH_EPS {
                preferred =
                    preferred.translate(Vec2::new(bounds.left() - layout.bounds.left(), 0.0));
            }
        }
        HorizontalAnchor::Right => {
            if (layout.bounds.right() - preferred.right()).abs() <= FLUSH_EPS {
                preferred =
                    preferred.translate(Vec2::new(bounds.right() - layout.bounds.right(), 0.0));
            }
        }
    }
    match layout.vertical {
        VerticalAnchor::Top => {
            if (preferred.top() - layout.bounds.top()).abs() <= FLUSH_EPS {
                preferred = preferred.translate(Vec2::new(0.0, bounds.top() - layout.bounds.top()));
            }
        }
        VerticalAnchor::Bottom => {
            if (layout.bounds.bottom() - preferred.bottom()).abs() <= FLUSH_EPS {
                preferred =
                    preferred.translate(Vec2::new(0.0, bounds.bottom() - layout.bounds.bottom()));
            }
        }
    }
    let x_range = match layout.horizontal {
        HorizontalAnchor::Left => {
            let left = preferred.left().clamp(bounds.left(), bounds.right());
            let width = desired_size.x.min((bounds.right() - left).max(1.0));
            left..=left + width
        }
        HorizontalAnchor::Right => {
            let right = preferred.right().clamp(bounds.left(), bounds.right());
            let width = desired_size.x.min((right - bounds.left()).max(1.0));
            right - width..=right
        }
    };
    let y_range = match layout.vertical {
        VerticalAnchor::Top => {
            let top = preferred.top().clamp(bounds.top(), bounds.bottom());
            let height = desired_size.y.min((bounds.bottom() - top).max(1.0));
            top..=top + height
        }
        VerticalAnchor::Bottom => {
            let bottom = preferred.bottom().clamp(bounds.top(), bounds.bottom());
            let height = desired_size.y.min((bottom - bounds.top()).max(1.0));
            bottom - height..=bottom
        }
    };
    layout.rect = Rect::from_x_y_ranges(x_range, y_range);
    layout.preferred = preferred;
    layout.bounds = bounds;
    layout.collapsed = collapsed;
    layout
}
