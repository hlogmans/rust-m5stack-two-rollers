//! Input routing: map touch events to logical buttons per screen
//!
//! This module decouples raw touch input from business actions by:
//! - Receiving `TouchPoint` events
//! - Using a registered `ButtonLayout` for the active screen
//! - Emitting `ButtonEvent`s when touches hit a button
//!
//! Screens register and unregister their buttons via `set_buttons()`.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;

use crate::hardware::{TouchEvent, TouchPoint};
use crate::ui::buttons::{ButtonId, ButtonSpec};

/// Event types for button interactions
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ButtonEventKind {
    Press,
    Release,
}

/// A routed button event
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ButtonEvent {
    pub id: ButtonId,
    pub kind: ButtonEventKind,
}

/// A fixed-size layout of buttons for the active screen
#[derive(Copy, Clone, Debug)]
pub struct ButtonLayout {
    buttons: [Option<ButtonSpec>; 4],
}

impl ButtonLayout {
    pub const fn empty() -> Self {
        Self { buttons: [None, None, None, None] }
    }

    pub fn from_slice(specs: &[ButtonSpec]) -> Self {
        let mut buttons = [None, None, None, None];
        let mut i = 0;
        while i < specs.len() && i < 4 {
            buttons[i] = Some(specs[i]);
            i += 1;
        }
        Self { buttons }
    }

    fn hit(&self, x: u16, y: u16) -> Option<ButtonId> {
        for slot in &self.buttons {
            if let Some(spec) = slot {
                if spec.contains(x, y) {
                    return Some(spec.id);
                }
            }
        }
        None
    }
}

/// Channel with the latest button layout (set by the active screen)
pub static BUTTON_LAYOUT: Watch<CriticalSectionRawMutex, ButtonLayout, 2> = Watch::new();

/// Stream of button events generated from touches
pub static BUTTON_EVENTS: Channel<CriticalSectionRawMutex, ButtonEvent, 8> = Channel::new();

/// Update the active button layout (called by the UI when a screen is shown)
pub fn set_buttons(specs: &[ButtonSpec]) {
    let layout = ButtonLayout::from_slice(specs);
    BUTTON_LAYOUT.sender().send(layout);
}

/// Clear buttons when a screen is hidden
pub fn clear_buttons() {
    BUTTON_LAYOUT.sender().send(ButtonLayout::empty());
}

/// Task: Route touches to button events. Provide the touch `Watch`.
#[embassy_executor::task]
pub async fn run_button_router(
    touch_watch: &'static Watch<CriticalSectionRawMutex, TouchPoint, 4>,
) {
    let mut layout_rx = BUTTON_LAYOUT
        .receiver()
        .expect("Failed to create BUTTON_LAYOUT receiver");
    let mut touch_rx = touch_watch
        .receiver()
        .expect("Failed to create touch receiver");

    // Start with no buttons
    let mut layout = ButtonLayout::empty();
    let mut active_pressed: Option<ButtonId> = None;

    loop {
        // Wait for either a layout change or a touch event
        let evt = embassy_futures::select::select(layout_rx.changed(), touch_rx.changed()).await;
        match evt {
            embassy_futures::select::Either::First(new_layout) => {
                layout = new_layout;
                // On layout change, consider any pressed state released
                active_pressed = None;
            }
            embassy_futures::select::Either::Second(touch) => {
                match touch.event {
                    TouchEvent::Press | TouchEvent::Contact => {
                        if let Some(id) = layout.hit(touch.x, touch.y) {
                            // Debounce: only emit press when id changes from none/other
                            if active_pressed != Some(id) {
                                active_pressed = Some(id);
                                let _ = BUTTON_EVENTS.try_send(ButtonEvent { id, kind: ButtonEventKind::Press });
                            }
                        } else {
                            // Touch not on any button, reset pressed state
                            active_pressed = None;
                        }
                    }
                    TouchEvent::Release => {
                        if let Some(id) = active_pressed.take() {
                            let _ = BUTTON_EVENTS.try_send(ButtonEvent { id, kind: ButtonEventKind::Release });
                        }
                    }
                }
            }
        }
    }
}

/// Task: Handle button events and trigger business actions passed in
#[embassy_executor::task]
pub async fn run_button_actions(
    reset_a: &'static Channel<CriticalSectionRawMutex, (), 1>,
    reset_b: &'static Channel<CriticalSectionRawMutex, (), 1>,
) {
    let rx = BUTTON_EVENTS.receiver();
    loop {
        let evt = rx.receive().await;
        match (evt.id, evt.kind) {
            (ButtonId::ZeroA, ButtonEventKind::Press) => {
                let _ = reset_a.try_send(());
            }
            (ButtonId::ZeroB, ButtonEventKind::Press) => {
                let _ = reset_b.try_send(());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::buttons::{ButtonSpec, ButtonId};

    #[test]
    fn layout_hit_detection() {
        let specs = [
            ButtonSpec::rect(ButtonId::ZeroA, 10, 20, 100, 30),
            ButtonSpec::rect(ButtonId::ZeroB, 150, 200, 50, 40),
        ];
        let layout = ButtonLayout::from_slice(&specs);
        assert_eq!(layout.hit(20, 30), Some(ButtonId::ZeroA));
        assert_eq!(layout.hit(180, 220), Some(ButtonId::ZeroB));
        assert_eq!(layout.hit(0, 0), None);
    }
}
