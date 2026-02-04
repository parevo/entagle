//! Input injector trait abstraction

use shared_protocol::{InputEvent, InputPacket};

use crate::InjectorResult;

/// Input injector trait
pub trait InputInjector: Send + Sync {
    /// Check if we have permission to inject input
    fn has_permission(&self) -> bool;

    /// Request permission (may show system dialog)
    fn request_permission(&self) -> InjectorResult<bool>;

    /// Inject a single input event
    fn inject(&self, event: &InputEvent) -> InjectorResult<()>;

    /// Inject a batch of input events
    fn inject_batch(&self, events: &[InputEvent]) -> InjectorResult<()>;

    /// Move mouse to absolute position
    fn move_mouse(&self, x: f64, y: f64) -> InjectorResult<()>;

    /// Move mouse relative to current position
    fn move_mouse_relative(&self, dx: f64, dy: f64) -> InjectorResult<()>;

    /// Click mouse button
    fn click(&self, button: shared_protocol::MouseButton) -> InjectorResult<()>;

    /// Press and hold mouse button
    fn mouse_down(&self, button: shared_protocol::MouseButton) -> InjectorResult<()>;

    /// Release mouse button
    fn mouse_up(&self, button: shared_protocol::MouseButton) -> InjectorResult<()>;

    /// Scroll mouse wheel
    fn scroll(&self, delta_x: f64, delta_y: f64) -> InjectorResult<()>;

    /// Press and release a key
    fn tap_key(&self, key: shared_protocol::VirtualKeyCode) -> InjectorResult<()>;

    /// Press and hold a key
    fn key_down(&self, key: shared_protocol::VirtualKeyCode) -> InjectorResult<()>;

    /// Release a key
    fn key_up(&self, key: shared_protocol::VirtualKeyCode) -> InjectorResult<()>;

    /// Type a string (simulates key presses)
    fn type_text(&self, text: &str) -> InjectorResult<()>;

    /// Get current mouse position
    fn mouse_position(&self) -> InjectorResult<(f64, f64)>;

    /// Get screen dimensions
    fn screen_size(&self) -> InjectorResult<(u32, u32)>;
}

/// Input event processor that handles packets from the network
pub struct InputProcessor {
    injector: Box<dyn InputInjector>,
    last_sequence: u64,
    screen_width: u32,
    screen_height: u32,
}

impl InputProcessor {
    pub fn new(injector: Box<dyn InputInjector>) -> InjectorResult<Self> {
        let (screen_width, screen_height) = injector.screen_size()?;

        Ok(Self {
            injector,
            last_sequence: 0,
            screen_width,
            screen_height,
        })
    }

    /// Process an input packet from the network
    pub fn process_packet(&mut self, packet: &InputPacket) -> InjectorResult<()> {
        // Check for out-of-order packets
        if packet.sequence <= self.last_sequence && self.last_sequence > 0 {
            tracing::warn!(
                "Out-of-order input packet: {} <= {}",
                packet.sequence,
                self.last_sequence
            );
            // Still process it - input ordering is less critical than video
        }
        self.last_sequence = packet.sequence;

        self.process_event(&packet.event)
    }

    /// Process a single input event
    pub fn process_event(&self, event: &InputEvent) -> InjectorResult<()> {
        match event {
            InputEvent::MouseMove { x, y, normalized } => {
                let (abs_x, abs_y) = if *normalized {
                    (x * self.screen_width as f64, y * self.screen_height as f64)
                } else {
                    (*x, *y)
                };
                self.injector.move_mouse(abs_x, abs_y)
            }
            InputEvent::MouseButton {
                button,
                state,
                x,
                y,
            } => {
                // Move to position first
                self.injector.move_mouse(*x, *y)?;

                match state {
                    shared_protocol::KeyState::Pressed => self.injector.mouse_down(*button),
                    shared_protocol::KeyState::Released => self.injector.mouse_up(*button),
                }
            }
            InputEvent::MouseScroll {
                delta_x, delta_y, ..
            } => self.injector.scroll(*delta_x, *delta_y),
            InputEvent::Key {
                key_code, state, ..
            } => match state {
                shared_protocol::KeyState::Pressed => self.injector.key_down(*key_code),
                shared_protocol::KeyState::Released => self.injector.key_up(*key_code),
            },
            InputEvent::TextInput { text } => self.injector.type_text(text),
        }
    }
}
