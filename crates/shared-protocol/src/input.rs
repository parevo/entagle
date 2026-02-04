//! Input event definitions for keyboard and mouse

use serde::{Deserialize, Serialize};

/// Mouse button type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// Keyboard modifier keys
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool, // Command on macOS, Windows key on Windows
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl KeyModifiers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn with_ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn with_alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn with_meta(mut self) -> Self {
        self.meta = true;
        self
    }
}

/// Key press state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyState {
    Pressed,
    Released,
}

/// Virtual key code (cross-platform)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum VirtualKeyCode {
    // Alphanumeric
    A = 0x0004,
    B = 0x0005,
    C = 0x0006,
    D = 0x0007,
    E = 0x0008,
    F = 0x0009,
    G = 0x000A,
    H = 0x000B,
    I = 0x000C,
    J = 0x000D,
    K = 0x000E,
    L = 0x000F,
    M = 0x0010,
    N = 0x0011,
    O = 0x0012,
    P = 0x0013,
    Q = 0x0014,
    R = 0x0015,
    S = 0x0016,
    T = 0x0017,
    U = 0x0018,
    V = 0x0019,
    W = 0x001A,
    X = 0x001B,
    Y = 0x001C,
    Z = 0x001D,

    // Numbers
    Num1 = 0x001E,
    Num2 = 0x001F,
    Num3 = 0x0020,
    Num4 = 0x0021,
    Num5 = 0x0022,
    Num6 = 0x0023,
    Num7 = 0x0024,
    Num8 = 0x0025,
    Num9 = 0x0026,
    Num0 = 0x0027,

    // Function keys
    F1 = 0x003A,
    F2 = 0x003B,
    F3 = 0x003C,
    F4 = 0x003D,
    F5 = 0x003E,
    F6 = 0x003F,
    F7 = 0x0040,
    F8 = 0x0041,
    F9 = 0x0042,
    F10 = 0x0043,
    F11 = 0x0044,
    F12 = 0x0045,

    // Control keys
    Escape = 0x0029,
    Tab = 0x002B,
    CapsLock = 0x0039,
    Shift = 0x00E1,
    Control = 0x00E0,
    Alt = 0x00E2,
    Meta = 0x00E3,
    Space = 0x002C,
    Enter = 0x0028,
    Backspace = 0x002A,
    Delete = 0x004C,
    Insert = 0x0049,
    Home = 0x004A,
    End = 0x004D,
    PageUp = 0x004B,
    PageDown = 0x004E,

    // Arrow keys
    Left = 0x0050,
    Right = 0x004F,
    Up = 0x0052,
    Down = 0x0051,

    // Punctuation
    Minus = 0x002D,
    Equal = 0x002E,
    LeftBracket = 0x002F,
    RightBracket = 0x0030,
    Backslash = 0x0031,
    Semicolon = 0x0033,
    Quote = 0x0034,
    Grave = 0x0035,
    Comma = 0x0036,
    Period = 0x0037,
    Slash = 0x0038,

    // Numpad
    NumpadDivide = 0x0054,
    NumpadMultiply = 0x0055,
    NumpadSubtract = 0x0056,
    NumpadAdd = 0x0057,
    NumpadEnter = 0x0058,
    Numpad1 = 0x0059,
    Numpad2 = 0x005A,
    Numpad3 = 0x005B,
    Numpad4 = 0x005C,
    Numpad5 = 0x005D,
    Numpad6 = 0x005E,
    Numpad7 = 0x005F,
    Numpad8 = 0x0060,
    Numpad9 = 0x0061,
    Numpad0 = 0x0062,
    NumpadDecimal = 0x0063,

    // Media keys
    PrintScreen = 0x0046,
    ScrollLock = 0x0047,
    Pause = 0x0048,
    NumLock = 0x0053,

    // Unknown/unmapped
    Unknown = 0xFFFF,
}

/// Input event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    /// Mouse movement (absolute coordinates)
    MouseMove {
        x: f64,
        y: f64,
        /// Coordinates are relative to screen dimensions (0.0-1.0)
        normalized: bool,
    },
    /// Mouse button press/release
    MouseButton {
        button: MouseButton,
        state: KeyState,
        x: f64,
        y: f64,
    },
    /// Mouse scroll
    MouseScroll {
        delta_x: f64,
        delta_y: f64,
        /// true if using pixel-precise scrolling (e.g., trackpad)
        precise: bool,
    },
    /// Keyboard key press/release
    Key {
        key_code: VirtualKeyCode,
        state: KeyState,
        modifiers: KeyModifiers,
    },
    /// Text input (for IME and Unicode)
    TextInput { text: String },
}

/// Input event packet with timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputPacket {
    /// Sequence number for ordering
    pub sequence: u64,
    /// Timestamp in microseconds
    pub timestamp_us: u64,
    /// The input event
    pub event: InputEvent,
}

impl InputPacket {
    pub fn new(sequence: u64, event: InputEvent) -> Self {
        Self {
            sequence,
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_micros() as u64)
                .unwrap_or(0),
            event,
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}
