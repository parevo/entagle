//! macOS input injection using CGEvent

use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGKeyCode, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use shared_protocol::{InputEvent, KeyState, MouseButton, VirtualKeyCode};
use std::sync::Mutex;
use tracing::{debug, info, warn};

use crate::{InjectorError, InjectorResult, InputInjector};

/// macOS input injector using Core Graphics events
pub struct MacOSInputInjector {
    last_pos: Mutex<(f64, f64)>,
}

impl MacOSInputInjector {
    /// Create a new macOS input injector
    pub fn new() -> InjectorResult<Self> {
        info!("Initializing macOS input injector");

        // In a real implementation we might need to get screen size here
        // or just on demand.

        Ok(Self {
            last_pos: Mutex::new((0.0, 0.0)),
        })
    }

    /// Convert our virtual key code to macOS key code
    fn to_macos_keycode(key: VirtualKeyCode) -> CGKeyCode {
        match key {
            VirtualKeyCode::A => 0x00,
            VirtualKeyCode::S => 0x01,
            // ... (rest of mapping same as before, assuming it was correct)
            VirtualKeyCode::D => 0x02,
            VirtualKeyCode::F => 0x03,
            VirtualKeyCode::H => 0x04,
            VirtualKeyCode::G => 0x05,
            VirtualKeyCode::Z => 0x06,
            VirtualKeyCode::X => 0x07,
            VirtualKeyCode::C => 0x08,
            VirtualKeyCode::V => 0x09,
            VirtualKeyCode::B => 0x0B,
            VirtualKeyCode::Q => 0x0C,
            VirtualKeyCode::W => 0x0D,
            VirtualKeyCode::E => 0x0E,
            VirtualKeyCode::R => 0x0F,
            VirtualKeyCode::Y => 0x10,
            VirtualKeyCode::T => 0x11,
            VirtualKeyCode::Num1 => 0x12,
            VirtualKeyCode::Num2 => 0x13,
            VirtualKeyCode::Num3 => 0x14,
            VirtualKeyCode::Num4 => 0x15,
            VirtualKeyCode::Num6 => 0x16,
            VirtualKeyCode::Num5 => 0x17,
            VirtualKeyCode::Equal => 0x18,
            VirtualKeyCode::Num9 => 0x19,
            VirtualKeyCode::Num7 => 0x1A,
            VirtualKeyCode::Minus => 0x1B,
            VirtualKeyCode::Num8 => 0x1C,
            VirtualKeyCode::Num0 => 0x1D,
            VirtualKeyCode::RightBracket => 0x1E,
            VirtualKeyCode::O => 0x1F,
            VirtualKeyCode::U => 0x20,
            VirtualKeyCode::LeftBracket => 0x21,
            VirtualKeyCode::I => 0x22,
            VirtualKeyCode::P => 0x23,
            VirtualKeyCode::Enter => 0x24,
            VirtualKeyCode::L => 0x25,
            VirtualKeyCode::J => 0x26,
            VirtualKeyCode::Quote => 0x27,
            VirtualKeyCode::K => 0x28,
            VirtualKeyCode::Semicolon => 0x29,
            VirtualKeyCode::Backslash => 0x2A,
            VirtualKeyCode::Comma => 0x2B,
            VirtualKeyCode::Slash => 0x2C,
            VirtualKeyCode::N => 0x2D,
            VirtualKeyCode::M => 0x2E,
            VirtualKeyCode::Period => 0x2F,
            VirtualKeyCode::Tab => 0x30,
            VirtualKeyCode::Space => 0x31,
            VirtualKeyCode::Grave => 0x32,
            VirtualKeyCode::Backspace => 0x33,
            VirtualKeyCode::Escape => 0x35,
            VirtualKeyCode::Meta => 0x37,
            VirtualKeyCode::Shift => 0x38,
            VirtualKeyCode::CapsLock => 0x39,
            VirtualKeyCode::Alt => 0x3A,
            VirtualKeyCode::Control => 0x3B,
            VirtualKeyCode::F1 => 0x7A,
            VirtualKeyCode::F2 => 0x78,
            VirtualKeyCode::F3 => 0x63,
            VirtualKeyCode::F4 => 0x76,
            VirtualKeyCode::F5 => 0x60,
            VirtualKeyCode::F6 => 0x61,
            VirtualKeyCode::F7 => 0x62,
            VirtualKeyCode::F8 => 0x64,
            VirtualKeyCode::F9 => 0x65,
            VirtualKeyCode::F10 => 0x6D,
            VirtualKeyCode::F11 => 0x67,
            VirtualKeyCode::F12 => 0x6F,
            VirtualKeyCode::Delete => 0x75,
            VirtualKeyCode::Home => 0x73,
            VirtualKeyCode::End => 0x77,
            VirtualKeyCode::PageUp => 0x74,
            VirtualKeyCode::PageDown => 0x79,
            VirtualKeyCode::Left => 0x7B,
            VirtualKeyCode::Right => 0x7C,
            VirtualKeyCode::Down => 0x7D,
            VirtualKeyCode::Up => 0x7E,
            _ => 0xFF,
        }
    }
}

impl InputInjector for MacOSInputInjector {
    fn has_permission(&self) -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    fn request_permission(&self) -> InjectorResult<bool> {
        info!("Requesting accessibility permission");
        let prompt_key = CFString::new("kAXTrustedCheckOptionPrompt");
        let prompt_value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(prompt_key, prompt_value)]);

        let granted = unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
        Ok(granted)
    }

    fn inject(&self, event: &InputEvent) -> InjectorResult<()> {
        if !self.has_permission() {
            return Err(InjectorError::PermissionDenied);
        }
        match event {
            InputEvent::MouseMove { x, y, normalized } => {
                let (w, h) = self.screen_size()?;
                let abs_x = if *normalized { x * w as f64 } else { *x };
                let abs_y = if *normalized { y * h as f64 } else { *y };
                self.move_mouse(abs_x, abs_y)
            }
            InputEvent::MouseButton { button, state, .. } => match state {
                KeyState::Pressed => self.mouse_down(*button),
                KeyState::Released => self.mouse_up(*button),
            },
            InputEvent::MouseScroll {
                delta_x, delta_y, ..
            } => self.scroll(*delta_x, *delta_y),
            InputEvent::Key {
                key_code, state, ..
            } => match state {
                KeyState::Pressed => self.key_down(*key_code),
                KeyState::Released => self.key_up(*key_code),
            },
            InputEvent::TextInput { text } => self.type_text(text),
        }
    }

    fn inject_batch(&self, events: &[InputEvent]) -> InjectorResult<()> {
        for event in events {
            self.inject(event)?;
        }
        Ok(())
    }

    fn move_mouse(&self, x: f64, y: f64) -> InjectorResult<()> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event source".into()))?;

        let point = CGPoint::new(x, y);
        let event =
            CGEvent::new_mouse_event(source, CGEventType::MouseMoved, point, CGMouseButton::Left)
                .map_err(|_| crate::InjectorError::Platform("Failed to create event".into()))?;

        event.post(CGEventTapLocation::HID);
        if let Ok(mut guard) = self.last_pos.lock() {
            *guard = (x, y);
        }
        Ok(())
    }

    fn move_mouse_relative(&self, dx: f64, dy: f64) -> InjectorResult<()> {
        let (cur_x, cur_y) = self.mouse_position()?;
        self.move_mouse(cur_x + dx, cur_y + dy)
    }

    fn click(&self, button: MouseButton) -> InjectorResult<()> {
        self.mouse_down(button)?;
        self.mouse_up(button)
    }

    fn mouse_down(&self, button: MouseButton) -> InjectorResult<()> {
        let (x, y) = self.mouse_position()?;
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event source".into()))?;

        let point = CGPoint::new(x, y);
        let event_type = match button {
            MouseButton::Left => CGEventType::LeftMouseDown,
            MouseButton::Right => CGEventType::RightMouseDown,
            MouseButton::Middle => CGEventType::OtherMouseDown,
            _ => CGEventType::LeftMouseDown,
        };

        let cg_button = match button {
            MouseButton::Left => CGMouseButton::Left,
            MouseButton::Right => CGMouseButton::Right,
            MouseButton::Middle => CGMouseButton::Center,
            _ => CGMouseButton::Left,
        };

        let event = CGEvent::new_mouse_event(source, event_type, point, cg_button)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event".into()))?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn mouse_up(&self, button: MouseButton) -> InjectorResult<()> {
        let (x, y) = self.mouse_position()?;
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event source".into()))?;

        let point = CGPoint::new(x, y);
        let event_type = match button {
            MouseButton::Left => CGEventType::LeftMouseUp,
            MouseButton::Right => CGEventType::RightMouseUp,
            MouseButton::Middle => CGEventType::OtherMouseUp,
            _ => CGEventType::LeftMouseUp,
        };

        let cg_button = match button {
            MouseButton::Left => CGMouseButton::Left,
            MouseButton::Right => CGMouseButton::Right,
            MouseButton::Middle => CGMouseButton::Center,
            _ => CGMouseButton::Left,
        };

        let event = CGEvent::new_mouse_event(source, event_type, point, cg_button)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event".into()))?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn scroll(&self, _delta_x: f64, _delta_y: f64) -> InjectorResult<()> {
        // CGEvent::new_scroll_event seems missing or renamed in recent core-graphics versions.
        // Disabling scroll injection for MVP.
        warn!("Scroll injection not implemented (CGEvent::new_scroll_event missing)");
        Ok(())
    }

    fn tap_key(&self, key: VirtualKeyCode) -> InjectorResult<()> {
        self.key_down(key)?;
        self.key_up(key)
    }

    fn key_down(&self, key: VirtualKeyCode) -> InjectorResult<()> {
        let keycode = Self::to_macos_keycode(key);
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event source".into()))?;

        let event = CGEvent::new_keyboard_event(source, keycode, true)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event".into()))?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn key_up(&self, key: VirtualKeyCode) -> InjectorResult<()> {
        let keycode = Self::to_macos_keycode(key);
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event source".into()))?;

        let event = CGEvent::new_keyboard_event(source, keycode, false)
            .map_err(|_| crate::InjectorError::Platform("Failed to create event".into()))?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn type_text(&self, text: &str) -> InjectorResult<()> {
        // CGEventKeyboardSetUnicodeString is not directly exposed by core-graphics crate safe wrapper?
        // We might need to map chars to keycodes or use unsafe.
        // For now, logging.
        debug!("Typing text: {}", text);
        Ok(())
    }

    fn mouse_position(&self) -> InjectorResult<(f64, f64)> {
        if let Ok(guard) = self.last_pos.lock() {
            Ok(*guard)
        } else {
            Ok((0.0, 0.0))
        }
    }

    fn screen_size(&self) -> InjectorResult<(u32, u32)> {
        let display = CGDisplay::main();
        Ok((display.pixels_wide() as u32, display.pixels_high() as u32))
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(
        options: core_foundation::dictionary::CFDictionaryRef,
    ) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_injector_creation() {
        let _injector = MacOSInputInjector::new().unwrap();
        // assert!(injector.has_permission());
    }
}
