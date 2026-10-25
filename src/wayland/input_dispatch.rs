use super::*;

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(
        s: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(c),
        } = event
        {
            if c.contains(wl_seat::Capability::Pointer) && s.pointer.is_none() {
                s.pointer = Some(seat.get_pointer(qh, ()));
            }
            if c.contains(wl_seat::Capability::Keyboard)
                && s.keyboard.is_none()
                && s.config.plugins.iter().any(|p| p.enabled && p.keyboard)
            {
                s.keyboard = Some(seat.get_keyboard(qh, ()));
            }
            if !c.contains(wl_seat::Capability::Keyboard) {
                if let Some(panel) = s.keyboard_panel() {
                    s.input_event(
                        panel,
                        InputEvent::KeyboardFocus {
                            focused: false,
                            keys: Vec::new(),
                        },
                    );
                }
                if let Some(k) = s.keyboard.take()
                    && k.version() >= 3
                {
                    k.release();
                }
                s.keyboard_focus = None;
            }
            if !c.contains(wl_seat::Capability::Pointer) {
                s.pointer_event(PointerKind::Leave, None, None);
                if let Some(p) = s.pointer.take() {
                    p.release();
                }
                s.focus = None;
                s.finish_drag(false);
            }
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(
        s: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                s.focus = surface.data::<SurfaceId>().copied();
                s.pointer_pos = (surface_x, surface_y);
                if s.drag.is_none() {
                    s.pointer_event(PointerKind::Enter, None, None);
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if s.focus == surface.data::<SurfaceId>().copied() {
                    if s.drag.is_none() {
                        s.pointer_event(PointerKind::Leave, None, None);
                    }
                    s.focus = None;
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                s.pointer_pos = (surface_x, surface_y);
                s.motion();
                if s.drag.is_none() {
                    s.pointer_event(PointerKind::Motion, None, None);
                }
            }
            wl_pointer::Event::Axis {
                axis: WEnum::Value(axis),
                value,
                ..
            } if s.drag.is_none() => {
                let axis = match axis {
                    wl_pointer::Axis::VerticalScroll => Axis::Vertical,
                    wl_pointer::Axis::HorizontalScroll => Axis::Horizontal,
                    _ => return,
                };
                s.pointer_event(PointerKind::Scroll, None, Some((axis, value)));
            }
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(state),
                ..
            } => {
                let pressed = state == wl_pointer::ButtonState::Pressed;
                if s.drag.is_some() {
                    if pressed {
                        s.finish_drag(false);
                    } else if button == 0x110 {
                        s.motion();
                        s.finish_drag(true);
                    }
                } else if let Some(panel) = s.interactive_panel() {
                    if pressed && button == 0x110 && s.pointer_pos.1 < s.drag_height(panel) as f64 {
                        s.begin_drag(qh);
                    } else {
                        s.pointer_event(PointerKind::Button, Some((button, pressed)), None);
                    }
                } else if pressed && button == 0x110 {
                    s.begin_drag(qh);
                }
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_keyboard::WlKeyboard, ()> for App {
    fn event(
        s: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap {
                format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
                fd,
                size,
            } => {
                use std::io::Read;
                if size > 131_072 {
                    return;
                }
                let mut bytes = Vec::new();
                if File::from(fd)
                    .take(u64::from(size))
                    .read_to_end(&mut bytes)
                    .is_ok()
                {
                    if bytes.last() == Some(&0) {
                        bytes.pop();
                    }
                    s.keymap = String::from_utf8(bytes).ok();
                    s.keyboard_state();
                }
            }
            wl_keyboard::Event::Enter { surface, keys, .. } => {
                s.keyboard_focus = surface.data::<SurfaceId>().copied();
                s.keyboard_state();
                if let Some(panel) = s.keyboard_panel() {
                    let keys = keys
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .take(256)
                        .map(|k| u32::from_ne_bytes(*k))
                        .collect();
                    s.input_event(
                        panel,
                        InputEvent::KeyboardFocus {
                            focused: true,
                            keys,
                        },
                    );
                }
            }
            wl_keyboard::Event::Leave { surface, .. }
                if s.keyboard_focus == surface.data::<SurfaceId>().copied() =>
            {
                if let Some(panel) = s.keyboard_panel() {
                    s.input_event(
                        panel,
                        InputEvent::KeyboardFocus {
                            focused: false,
                            keys: Vec::new(),
                        },
                    );
                }
                s.keyboard_focus = None;
            }
            wl_keyboard::Event::Key {
                time,
                key,
                state: WEnum::Value(state),
                ..
            } => {
                if let Some(panel) = s.keyboard_panel() {
                    s.input_event(
                        panel,
                        InputEvent::Key {
                            key,
                            pressed: state == wl_keyboard::KeyState::Pressed,
                            time,
                        },
                    );
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                s.modifiers = [mods_depressed, mods_latched, mods_locked, group];
                if let Some(panel) = s.keyboard_panel() {
                    s.input_event(
                        panel,
                        InputEvent::Modifiers {
                            depressed: mods_depressed,
                            latched: mods_latched,
                            locked: mods_locked,
                            group,
                        },
                    );
                }
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                s.repeat = (rate, delay);
                if let Some(panel) = s.keyboard_panel() {
                    s.input_event(panel, InputEvent::RepeatInfo { rate, delay });
                }
            }
            _ => {}
        }
    }
}
