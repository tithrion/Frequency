//! Custom cent-deviation meter.

use crate::gui::theme;
use eframe::egui::{pos2, vec2, Align2, Color32, FontId, Rect, Response, Sense, Shape, Stroke, Ui};

const RANGE_CENTS: f32 = 50.0;

/// Draw the -50..+50 cent scale. `cents` is the displayed (already smoothed)
/// deviation; `Some` draws an active marker, `None` shows the idle state.
pub fn cents_meter(ui: &mut Ui, cents: Option<f32>, _target_present: bool) -> Response {
    let width = ui.available_width().max(320.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 156.0), Sense::hover());
    let painter = ui.painter_at(rect);

    let x_left = rect.left() + 34.0;
    let x_right = rect.right() - 34.0;
    let centre_x = (x_left + x_right) * 0.5;
    let baseline_y = rect.center().y + 18.0;
    let half_width = (x_right - x_left) * 0.5;
    let track = Rect::from_min_max(
        pos2(x_left, baseline_y - 6.0),
        pos2(x_right, baseline_y + 6.0),
    );
    painter.rect_filled(track, 6.0, theme::TRACK);

    // Green "in tune" zone (plus/minus three cents).
    let zone_half = half_width * (3.0 / RANGE_CENTS);
    let zone = Rect::from_min_max(
        pos2(centre_x - zone_half, baseline_y - 6.0),
        pos2(centre_x + zone_half, baseline_y + 6.0),
    );
    painter.rect_filled(zone, 2.0, theme::GREEN_DIM);

    // Ticks and labels.
    let mut cent = -50;
    while cent <= 50 {
        let x = centre_x + half_width * (cent as f32 / RANGE_CENTS);
        let major = cent % 25 == 0;
        let medium = cent % 10 == 0;
        let height = if major {
            13.0
        } else if medium {
            8.0
        } else {
            4.0
        };
        let colour = if major { theme::DIM } else { theme::TRACK };
        painter.line_segment(
            [pos2(x, baseline_y - height), pos2(x, baseline_y + height)],
            Stroke::new(if major { 1.5 } else { 1.0 }, colour),
        );
        if major {
            let label = if cent == 0 {
                "0c".to_string()
            } else {
                format!("{cent:+}c")
            };
            painter.text(
                pos2(x, baseline_y + 25.0),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(13.0),
                if cent == 0 { theme::TEXT } else { theme::DIM },
            );
        }
        cent += 5;
    }

    // Centre guide.
    painter.line_segment(
        [
            pos2(centre_x, baseline_y - 42.0),
            pos2(centre_x, baseline_y + 14.0),
        ],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
    );

    if let Some(cents) = cents.filter(|value| value.is_finite()) {
        let clamped = cents.clamp(-RANGE_CENTS, RANGE_CENTS);
        let x = centre_x + half_width * (clamped / RANGE_CENTS);
        let colour = theme::accent_for(cents);

        // Stem and needle tip.
        painter.line_segment(
            [pos2(x, baseline_y - 46.0), pos2(x, baseline_y - 8.0)],
            Stroke::new(3.0, colour),
        );
        painter.circle_filled(pos2(x, baseline_y - 46.0), 7.0, colour);
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x, baseline_y - 9.0),
                pos2(x - 9.0, baseline_y - 24.0),
                pos2(x + 9.0, baseline_y - 24.0),
            ],
            colour,
            Stroke::NONE,
        ));

        painter.text(
            pos2(x, baseline_y - 66.0),
            Align2::CENTER_CENTER,
            format!("{cents:+.1}c"),
            FontId::monospace(15.0),
            colour,
        );
    } else {
        painter.text(
            pos2(centre_x, baseline_y - 66.0),
            Align2::CENTER_CENTER,
            "waiting for signal",
            FontId::proportional(14.0),
            theme::DIM,
        );
    }

    response
}
