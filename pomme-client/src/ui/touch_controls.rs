//! Draws the touch controls (`app::input::touch`) over the HUD: the move
//! stick, resting bottom-left until a press places it, and the buttons.

use glam::Vec2;

use crate::app::input::touch::{
    BUTTON_RADIUS, STICK_RADIUS, TouchButton, TouchControls, TouchLayout,
};
use crate::renderer::pipelines::menu_overlay::MenuElement;
use crate::ui::common::{FONT_SIZE, WHITE};

const BASE: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
const KNOB: [f32; 4] = [1.0, 1.0, 1.0, 0.45];

pub fn build_touch_controls(
    elements: &mut Vec<MenuElement>,
    layout: &TouchLayout,
    touch: &TouchControls,
) {
    let gs = layout.gs;
    let radius = STICK_RADIUS * gs;
    let base = touch.stick_origin().unwrap_or_else(|| layout.stick_home());
    push_circle(elements, base, radius, BASE);
    let deflection = touch.stick().map_or(Vec2::ZERO, |v| Vec2::new(v.x, -v.y));
    push_circle(elements, base + deflection * radius, radius * 0.45, KNOB);

    for button in TouchButton::ALL {
        let centre = layout.button_centre(button);
        push_circle(elements, centre, BUTTON_RADIUS * gs, BASE);
        elements.push(MenuElement::Text {
            x: centre.x,
            y: centre.y - FONT_SIZE * gs / 2.0,
            text: if button == TouchButton::Mouse && touch.mouse_mode() {
                "Mouse ON".to_owned()
            } else if button == TouchButton::Mouse {
                "Mouse".to_owned()
            } else {
                button.label().to_owned()
            },
            scale: FONT_SIZE * gs,
            color: WHITE,
            centered: true,
        });
    }
}

fn push_circle(elements: &mut Vec<MenuElement>, centre: Vec2, radius: f32, color: [f32; 4]) {
    elements.push(MenuElement::Rect {
        x: centre.x - radius,
        y: centre.y - radius,
        w: 2.0 * radius,
        h: 2.0 * radius,
        corner_radius: radius,
        color,
    });
}

/// Draws a visible cursor for the virtual mouse, including over menus and
/// inventory screens where the normal gameplay touch HUD is hidden.
pub fn build_virtual_mouse_cursor(
    elements: &mut Vec<MenuElement>,
    cursor: (f32, f32),
    gui_scale: f32,
) {
    let (x, y) = cursor;
    let arm = 7.0 * gui_scale;
    let thickness = 2.0 * gui_scale;
    let color = [1.0, 1.0, 1.0, 0.95];
    for (rx, ry, w, h) in [
        (x - thickness / 2.0, y - arm, thickness, arm * 2.0),
        (x - arm, y - thickness / 2.0, arm * 2.0, thickness),
    ] {
        elements.push(MenuElement::Rect {
            x: rx,
            y: ry,
            w,
            h,
            corner_radius: thickness,
            color,
        });
    }
}
