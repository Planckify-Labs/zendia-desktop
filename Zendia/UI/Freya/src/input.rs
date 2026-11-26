/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Freya's input events, as the input events of a page.

use freya::prelude::{Code, Key, Modifiers, MouseButton, NamedKey, WheelSource};
use hub_protocol::{
    Web_DevicePixelPoint, Web_KeyEvent, Web_KeyEvent_Type, Web_MouseEvent, Web_MouseEvent_Type, Web_PointerInputData,
    Web_ScrollGesturePhase, Web_UIEvents_KeyCode as KeyCode, Web_UIEvents_KeyModifier as KeyModifier,
    Web_UIEvents_MouseButton as WebMouseButton, Web_WheelDeltaPrecision,
};
use std::time::{Duration, Instant};
use zendia_keymap::{KeyCode as MapKeyCode, KeyModifiers as MapModifiers, KeyStroke};

/// Translates a Freya keyboard event into a normalized [`KeyStroke`] for [`zendia_keymap`].
///
/// Pure modifier presses (Shift, Control, Alt, Meta, lock keys) return `None`.
pub fn key_stroke_from_event(key: &Key, code: Code, modifiers: Modifiers) -> Option<KeyStroke> {
    let mut map_modifiers = MapModifiers::NONE;
    // NB: AltGr reports Ctrl+Alt on some platforms; the character it produced is what the user typed.
    let alt_graph = modifiers.contains(Modifiers::ALT_GRAPH);
    if modifiers.contains(Modifiers::CONTROL) && !alt_graph {
        map_modifiers |= MapModifiers::CTRL;
    }
    if modifiers.contains(Modifiers::SHIFT) {
        map_modifiers |= MapModifiers::SHIFT;
    }
    if modifiers.contains(Modifiers::ALT) && !alt_graph {
        map_modifiers |= MapModifiers::ALT;
    }
    if modifiers.contains(Modifiers::META) {
        map_modifiers |= MapModifiers::SUPER;
    }

    // NB: Alt and Ctrl change the character on some layouts (Option+j is "∆" on macOS), so a chorded character key is
    //     the physical key: <A-j> means the j key.
    if map_modifiers.has_non_shift()
        && let Key::Character(text) = key
        && text.chars().count() == 1
        && let Some(ch) = physical_character(code)
    {
        return Some(KeyStroke::new(map_modifiers, MapKeyCode::Char(ch)));
    }

    // On X11/XKB, Ctrl+M/J/I/H/[ produce ASCII control characters that winit maps to NamedKey::Enter/Tab/Backspace/
    // Escape. Recover the underlying character key from the physical `Code` while preserving real Ctrl+Enter/Tab/etc.
    if map_modifiers.contains(MapModifiers::CTRL) {
        let recovered = match (key, code) {
            (Key::Named(NamedKey::Enter), Code::KeyM) => Some('m'),
            (Key::Named(NamedKey::Enter), Code::KeyJ) => Some('j'),
            (Key::Named(NamedKey::Tab), Code::KeyI) => Some('i'),
            (Key::Named(NamedKey::Backspace), Code::KeyH) => Some('h'),
            (Key::Named(NamedKey::Escape), Code::BracketLeft) => Some('['),
            _ => None,
        };
        if let Some(ch) = recovered {
            return Some(KeyStroke::new(map_modifiers, MapKeyCode::Char(ch)));
        }
    }

    let map_key = match key {
        Key::Character(text) => {
            let mut chars = text.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            match ch {
                // Some X11 layouts emit ASCII control characters (0x01..=0x1a) when Control is held (for example
                // Ctrl+M -> '\r' = 0x0d, Ctrl+I -> '\t' = 0x09). Check this before matching control whitespace so
                // letter chords with Control are never mistaken for Enter/Tab/Backspace.
                c if map_modifiers.contains(MapModifiers::CTRL) && ('\x01'..='\x1a').contains(&c) => {
                    MapKeyCode::Char((b'a' + (c as u8 - 1)) as char)
                }
                ' ' => MapKeyCode::Space,
                '\t' => MapKeyCode::Tab,
                '\r' | '\n' => MapKeyCode::Enter,
                '\x1b' => MapKeyCode::Escape,
                '\x08' => MapKeyCode::Backspace,
                '\x7f' => MapKeyCode::Delete,
                c => MapKeyCode::Char(c),
            }
        }
        Key::Named(named) => match named {
            NamedKey::Shift
            | NamedKey::Control
            | NamedKey::Alt
            | NamedKey::AltGraph
            | NamedKey::Meta
            | NamedKey::CapsLock
            | NamedKey::NumLock
            | NamedKey::ScrollLock => return None,
            NamedKey::Enter => MapKeyCode::Enter,
            NamedKey::Tab => MapKeyCode::Tab,
            NamedKey::Backspace => MapKeyCode::Backspace,
            NamedKey::Delete => MapKeyCode::Delete,
            NamedKey::Escape => MapKeyCode::Escape,
            NamedKey::ArrowUp => MapKeyCode::Up,
            NamedKey::ArrowDown => MapKeyCode::Down,
            NamedKey::ArrowLeft => MapKeyCode::Left,
            NamedKey::ArrowRight => MapKeyCode::Right,
            NamedKey::Home => MapKeyCode::Home,
            NamedKey::End => MapKeyCode::End,
            NamedKey::PageUp => MapKeyCode::PageUp,
            NamedKey::PageDown => MapKeyCode::PageDown,
            NamedKey::Insert => MapKeyCode::Insert,
            NamedKey::F1 => MapKeyCode::F(1),
            NamedKey::F2 => MapKeyCode::F(2),
            NamedKey::F3 => MapKeyCode::F(3),
            NamedKey::F4 => MapKeyCode::F(4),
            NamedKey::F5 => MapKeyCode::F(5),
            NamedKey::F6 => MapKeyCode::F(6),
            NamedKey::F7 => MapKeyCode::F(7),
            NamedKey::F8 => MapKeyCode::F(8),
            NamedKey::F9 => MapKeyCode::F(9),
            NamedKey::F10 => MapKeyCode::F(10),
            NamedKey::F11 => MapKeyCode::F(11),
            NamedKey::F12 => MapKeyCode::F(12),
            NamedKey::F13 => MapKeyCode::F(13),
            NamedKey::F14 => MapKeyCode::F(14),
            NamedKey::F15 => MapKeyCode::F(15),
            NamedKey::F16 => MapKeyCode::F(16),
            NamedKey::F17 => MapKeyCode::F(17),
            NamedKey::F18 => MapKeyCode::F(18),
            NamedKey::F19 => MapKeyCode::F(19),
            NamedKey::F20 => MapKeyCode::F(20),
            NamedKey::F21 => MapKeyCode::F(21),
            NamedKey::F22 => MapKeyCode::F(22),
            NamedKey::F23 => MapKeyCode::F(23),
            NamedKey::F24 => MapKeyCode::F(24),
            _ => return None,
        },
    };

    Some(KeyStroke::new(map_modifiers, map_key))
}

/// The letter or digit printed on a physical key, if it is one.
fn physical_character(code: Code) -> Option<char> {
    let ch = match code {
        Code::KeyA => 'a',
        Code::KeyB => 'b',
        Code::KeyC => 'c',
        Code::KeyD => 'd',
        Code::KeyE => 'e',
        Code::KeyF => 'f',
        Code::KeyG => 'g',
        Code::KeyH => 'h',
        Code::KeyI => 'i',
        Code::KeyJ => 'j',
        Code::KeyK => 'k',
        Code::KeyL => 'l',
        Code::KeyM => 'm',
        Code::KeyN => 'n',
        Code::KeyO => 'o',
        Code::KeyP => 'p',
        Code::KeyQ => 'q',
        Code::KeyR => 'r',
        Code::KeyS => 's',
        Code::KeyT => 't',
        Code::KeyU => 'u',
        Code::KeyV => 'v',
        Code::KeyW => 'w',
        Code::KeyX => 'x',
        Code::KeyY => 'y',
        Code::KeyZ => 'z',
        Code::Digit0 => '0',
        Code::Digit1 => '1',
        Code::Digit2 => '2',
        Code::Digit3 => '3',
        Code::Digit4 => '4',
        Code::Digit5 => '5',
        Code::Digit6 => '6',
        Code::Digit7 => '7',
        Code::Digit8 => '8',
        Code::Digit9 => '9',
        _ => return None,
    };
    Some(ch)
}

/// The modifiers held once a key event is done. A modifier key's own release still reports that modifier as held, which
/// would leave it stuck for the mouse events after it.
pub fn modifiers_after_key(is_down: bool, key: &Key, mut modifiers: Modifiers) -> Modifiers {
    if is_down {
        return modifiers;
    }
    match key {
        Key::Named(NamedKey::Shift) => modifiers.remove(Modifiers::SHIFT),
        Key::Named(NamedKey::Control) => modifiers.remove(Modifiers::CONTROL),
        Key::Named(NamedKey::Alt) => modifiers.remove(Modifiers::ALT),
        Key::Named(NamedKey::Meta) => modifiers.remove(Modifiers::META),
        _ => {}
    }
    modifiers
}

pub fn key_modifiers(modifiers: Modifiers) -> KeyModifier {
    let mut result = KeyModifier::Mod_None;
    if modifiers.contains(Modifiers::ALT) {
        result = result | KeyModifier::Mod_Alt;
    }
    if modifiers.contains(Modifiers::CONTROL) {
        result = result | KeyModifier::Mod_Ctrl;
    }
    if modifiers.contains(Modifiers::SHIFT) {
        result = result | KeyModifier::Mod_Shift;
    }
    if modifiers.contains(Modifiers::META) {
        result = result | KeyModifier::Mod_Super;
    }
    result
}

fn key_code_for_character(character: char) -> KeyCode {
    match character.to_ascii_uppercase() {
        'A' => KeyCode::Key_A,
        'B' => KeyCode::Key_B,
        'C' => KeyCode::Key_C,
        'D' => KeyCode::Key_D,
        'E' => KeyCode::Key_E,
        'F' => KeyCode::Key_F,
        'G' => KeyCode::Key_G,
        'H' => KeyCode::Key_H,
        'I' => KeyCode::Key_I,
        'J' => KeyCode::Key_J,
        'K' => KeyCode::Key_K,
        'L' => KeyCode::Key_L,
        'M' => KeyCode::Key_M,
        'N' => KeyCode::Key_N,
        'O' => KeyCode::Key_O,
        'P' => KeyCode::Key_P,
        'Q' => KeyCode::Key_Q,
        'R' => KeyCode::Key_R,
        'S' => KeyCode::Key_S,
        'T' => KeyCode::Key_T,
        'U' => KeyCode::Key_U,
        'V' => KeyCode::Key_V,
        'W' => KeyCode::Key_W,
        'X' => KeyCode::Key_X,
        'Y' => KeyCode::Key_Y,
        'Z' => KeyCode::Key_Z,
        '0' => KeyCode::Key_0,
        '1' => KeyCode::Key_1,
        '2' => KeyCode::Key_2,
        '3' => KeyCode::Key_3,
        '4' => KeyCode::Key_4,
        '5' => KeyCode::Key_5,
        '6' => KeyCode::Key_6,
        '7' => KeyCode::Key_7,
        '8' => KeyCode::Key_8,
        '9' => KeyCode::Key_9,
        ' ' => KeyCode::Key_Space,
        ')' => KeyCode::Key_RightParen,
        '!' => KeyCode::Key_ExclamationPoint,
        '@' => KeyCode::Key_AtSign,
        '#' => KeyCode::Key_Hashtag,
        '$' => KeyCode::Key_Dollar,
        '%' => KeyCode::Key_Percent,
        '^' => KeyCode::Key_Circumflex,
        '&' => KeyCode::Key_Ampersand,
        '*' => KeyCode::Key_Asterisk,
        '(' => KeyCode::Key_LeftParen,
        '+' => KeyCode::Key_Plus,
        '-' => KeyCode::Key_Minus,
        '/' => KeyCode::Key_Slash,
        ',' => KeyCode::Key_Comma,
        '.' => KeyCode::Key_Period,
        ':' => KeyCode::Key_Colon,
        '"' => KeyCode::Key_DoubleQuote,
        '\'' => KeyCode::Key_Apostrophe,
        ';' => KeyCode::Key_Semicolon,
        '<' => KeyCode::Key_LessThan,
        '=' => KeyCode::Key_Equal,
        '>' => KeyCode::Key_GreaterThan,
        '?' => KeyCode::Key_QuestionMark,
        '[' => KeyCode::Key_LeftBracket,
        ']' => KeyCode::Key_RightBracket,
        '\\' => KeyCode::Key_Backslash,
        '_' => KeyCode::Key_Underscore,
        '{' => KeyCode::Key_LeftBrace,
        '}' => KeyCode::Key_RightBrace,
        '|' => KeyCode::Key_Pipe,
        '~' => KeyCode::Key_Tilde,
        '`' => KeyCode::Key_Backtick,
        _ => KeyCode::Key_Invalid,
    }
}

fn key_code_for_named_key(key: NamedKey, code: Code) -> KeyCode {
    match key {
        NamedKey::Backspace => KeyCode::Key_Backspace,
        NamedKey::Tab => KeyCode::Key_Tab,
        NamedKey::Enter => KeyCode::Key_Return,
        NamedKey::Shift if code == Code::ShiftRight => KeyCode::Key_RightShift,
        NamedKey::Shift => KeyCode::Key_LeftShift,
        NamedKey::Control if code == Code::ControlRight => KeyCode::Key_RightControl,
        NamedKey::Control => KeyCode::Key_LeftControl,
        NamedKey::Alt if code == Code::AltRight => KeyCode::Key_RightAlt,
        NamedKey::Alt => KeyCode::Key_LeftAlt,
        NamedKey::AltGraph => KeyCode::Key_AltGr,
        NamedKey::Meta if code == Code::MetaRight => KeyCode::Key_RightSuper,
        NamedKey::Meta => KeyCode::Key_LeftSuper,
        NamedKey::Pause => KeyCode::Key_PauseBreak,
        NamedKey::CapsLock => KeyCode::Key_CapsLock,
        NamedKey::Escape => KeyCode::Key_Escape,
        NamedKey::PageUp => KeyCode::Key_PageUp,
        NamedKey::PageDown => KeyCode::Key_PageDown,
        NamedKey::End => KeyCode::Key_End,
        NamedKey::Home => KeyCode::Key_Home,
        NamedKey::ArrowLeft => KeyCode::Key_Left,
        NamedKey::ArrowUp => KeyCode::Key_Up,
        NamedKey::ArrowRight => KeyCode::Key_Right,
        NamedKey::ArrowDown => KeyCode::Key_Down,
        NamedKey::PrintScreen => KeyCode::Key_PrintScreen,
        NamedKey::Delete => KeyCode::Key_Delete,
        NamedKey::Insert => KeyCode::Key_Insert,
        NamedKey::NumLock => KeyCode::Key_NumLock,
        NamedKey::ScrollLock => KeyCode::Key_ScrollLock,
        NamedKey::ContextMenu => KeyCode::Key_Menu,
        NamedKey::F1 => KeyCode::Key_F1,
        NamedKey::F2 => KeyCode::Key_F2,
        NamedKey::F3 => KeyCode::Key_F3,
        NamedKey::F4 => KeyCode::Key_F4,
        NamedKey::F5 => KeyCode::Key_F5,
        NamedKey::F6 => KeyCode::Key_F6,
        NamedKey::F7 => KeyCode::Key_F7,
        NamedKey::F8 => KeyCode::Key_F8,
        NamedKey::F9 => KeyCode::Key_F9,
        NamedKey::F10 => KeyCode::Key_F10,
        NamedKey::F11 => KeyCode::Key_F11,
        NamedKey::F12 => KeyCode::Key_F12,
        NamedKey::BrowserBack => KeyCode::Key_BrowserBack,
        NamedKey::BrowserForward => KeyCode::Key_BrowserForward,
        NamedKey::BrowserRefresh => KeyCode::Key_BrowserRefresh,
        NamedKey::BrowserStop => KeyCode::Key_BrowserStop,
        NamedKey::BrowserSearch => KeyCode::Key_BrowserSearch,
        NamedKey::BrowserFavorites => KeyCode::Key_BrowserFavorites,
        NamedKey::BrowserHome => KeyCode::Key_BrowserHome,
        NamedKey::AudioVolumeDown => KeyCode::Key_VolumeDown,
        NamedKey::AudioVolumeUp => KeyCode::Key_VolumeUp,
        NamedKey::AudioVolumeMute => KeyCode::Key_Mute,
        NamedKey::MediaPlayPause => KeyCode::Key_PlayPause,
        NamedKey::MediaTrackNext => KeyCode::Key_NextTrack,
        NamedKey::MediaTrackPrevious => KeyCode::Key_PreviousTrack,
        _ => KeyCode::Key_Invalid,
    }
}

fn is_keypad(code: Code) -> bool {
    matches!(
        code,
        Code::Numpad0
            | Code::Numpad1
            | Code::Numpad2
            | Code::Numpad3
            | Code::Numpad4
            | Code::Numpad5
            | Code::Numpad6
            | Code::Numpad7
            | Code::Numpad8
            | Code::Numpad9
            | Code::NumpadAdd
            | Code::NumpadSubtract
            | Code::NumpadMultiply
            | Code::NumpadDivide
            | Code::NumpadDecimal
            | Code::NumpadEnter
            | Code::NumpadEqual
    )
}

pub fn key_event(is_down: bool, key: &Key, code: Code, modifiers: Modifiers) -> Web_KeyEvent {
    let modifiers = modifiers_after_key(is_down, key, modifiers);
    let mut web_modifiers = key_modifiers(modifiers);
    if is_keypad(code) {
        web_modifiers = web_modifiers | KeyModifier::Mod_Keypad;
    }

    let (key_code, code_point) = match key {
        Key::Character(text) => {
            let character = text.chars().next().unwrap_or('\0');
            (key_code_for_character(character), character as u32)
        }
        // NB: Enter goes as '\n' everywhere, as it does from the Qt UI.
        Key::Named(NamedKey::Enter) => (KeyCode::Key_Return, '\n' as u32),
        Key::Named(NamedKey::Tab) => (KeyCode::Key_Tab, '\t' as u32),
        Key::Named(NamedKey::Backspace) => (KeyCode::Key_Backspace, 0x08),
        Key::Named(NamedKey::Escape) => (KeyCode::Key_Escape, 0x1B),
        Key::Named(NamedKey::Delete) => (KeyCode::Key_Delete, 0x7F),
        Key::Named(named) => (key_code_for_named_key(*named, code), 0),
    };

    // NB: A key that writes a character inserts it, unless a shortcut modifier makes it a command.
    let is_command = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
    let should_insert_text = is_down && matches!(key, Key::Character(_)) && !is_command;

    Web_KeyEvent {
        r#type: if is_down {
            Web_KeyEvent_Type::KeyDown
        } else {
            Web_KeyEvent_Type::KeyUp
        },
        key: key_code,
        modifiers: web_modifiers,
        code_point,
        repeat: false,
        should_insert_text,
        async_scroll_performed_default_action: false,
        user_input_scroll_count_with_nothing_to_scroll: None,
        id: 0,
        editing_commands: Vec::new(),
        native_editing_binding_resolved: false,
    }
}

pub fn mouse_button(button: MouseButton) -> WebMouseButton {
    match button {
        MouseButton::Left => WebMouseButton::Primary,
        MouseButton::Right => WebMouseButton::Secondary,
        MouseButton::Middle => WebMouseButton::Middle,
        MouseButton::Back => WebMouseButton::Backward,
        MouseButton::Forward => WebMouseButton::Forward,
        MouseButton::Other(_) => WebMouseButton::None,
    }
}

/// How long and how far apart two presses may be to make one double or triple click.
const MULTI_CLICK_INTERVAL: Duration = Duration::from_millis(500);
const MULTI_CLICK_DISTANCE: i32 = 4;

/// What the page needs to know of the mouse beyond each event: the buttons held down and the clicks in a row.
#[derive(Default)]
pub struct MouseState {
    pub buttons: WebMouseButton,
    pub modifiers: KeyModifier,
    last_press: Option<(Instant, Web_DevicePixelPoint, WebMouseButton)>,
    click_count: i32,
}

impl MouseState {
    fn event(
        &self,
        r#type: Web_MouseEvent_Type,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
        button: WebMouseButton,
    ) -> Web_MouseEvent {
        Web_MouseEvent {
            r#type,
            position,
            screen_position,
            button,
            buttons: self.buttons,
            modifiers: self.modifiers,
            wheel_delta_x: 0.0,
            wheel_delta_y: 0.0,
            wheel_tick_120ths_x: 0.0,
            wheel_tick_120ths_y: 0.0,
            wheel_delta_precision: Web_WheelDeltaPrecision::Discrete,
            scroll_gesture_phase: Web_ScrollGesturePhase::None,
            click_count: self.click_count,
            async_scroll_performed_default_action: false,
            id: 0,
            scrollbar_dragged_by_compositor: None,
            pointer_data: Web_PointerInputData { properties: None },
            wheel_gesture_mark: None,
        }
    }

    pub fn down(
        &mut self,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
        button: WebMouseButton,
    ) -> Web_MouseEvent {
        let now = Instant::now();
        let continues_clicks = self
            .last_press
            .as_ref()
            .is_some_and(|(at, last_position, last_button)| {
                *last_button == button
                    && now.duration_since(*at) <= MULTI_CLICK_INTERVAL
                    && (last_position.x - position.x).abs() <= MULTI_CLICK_DISTANCE
                    && (last_position.y - position.y).abs() <= MULTI_CLICK_DISTANCE
            });
        self.click_count = if continues_clicks { self.click_count + 1 } else { 1 };
        self.last_press = Some((now, position.clone(), button));
        self.buttons = self.buttons | button;
        self.event(Web_MouseEvent_Type::MouseDown, position, screen_position, button)
    }

    pub fn up(
        &mut self,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
        button: WebMouseButton,
    ) -> Web_MouseEvent {
        self.buttons = self.buttons & !button;
        self.event(Web_MouseEvent_Type::MouseUp, position, screen_position, button)
    }

    pub fn moved(&self, position: Web_DevicePixelPoint, screen_position: Web_DevicePixelPoint) -> Web_MouseEvent {
        self.event(
            Web_MouseEvent_Type::MouseMove,
            position,
            screen_position,
            WebMouseButton::None,
        )
    }

    pub fn left(&mut self) -> Web_MouseEvent {
        self.event(
            Web_MouseEvent_Type::MouseLeave,
            Web_DevicePixelPoint { x: 0, y: 0 },
            Web_DevicePixelPoint { x: 0, y: 0 },
            WebMouseButton::None,
        )
    }

    /// A turn of the wheel or a swipe on a touchpad, as Freya reports it, in logical pixels. Freya counts a line as
    /// 53 pixels and doubles a touchpad's pixels, and its deltas are positive towards the top and left of the page.
    pub fn wheel(
        &self,
        position: Web_DevicePixelPoint,
        screen_position: Web_DevicePixelPoint,
        delta_x: f64,
        delta_y: f64,
        source: WheelSource,
        device_pixel_ratio: f64,
    ) -> Web_MouseEvent {
        const FREYA_PIXELS_PER_LINE: f64 = 53.0;
        const FREYA_TOUCHPAD_SPEED: f64 = 2.0;
        // NB: As the Qt UI does: three lines of 40 pixels to a notch, and a notch is 120 ticks.
        const PIXELS_PER_NOTCH: f64 = 3.0 * 40.0;
        const TICKS_PER_NOTCH: f64 = 120.0;

        let mut event = self.event(
            Web_MouseEvent_Type::MouseWheel,
            position,
            screen_position,
            WebMouseButton::None,
        );
        match source {
            WheelSource::Line => {
                let notches_x = -delta_x / FREYA_PIXELS_PER_LINE;
                let notches_y = -delta_y / FREYA_PIXELS_PER_LINE;
                event.wheel_delta_x = notches_x * PIXELS_PER_NOTCH * device_pixel_ratio;
                event.wheel_delta_y = notches_y * PIXELS_PER_NOTCH * device_pixel_ratio;
                event.wheel_tick_120ths_x = notches_x * TICKS_PER_NOTCH;
                event.wheel_tick_120ths_y = notches_y * TICKS_PER_NOTCH;
                event.wheel_delta_precision = Web_WheelDeltaPrecision::Discrete;
            }
            _ => {
                event.wheel_delta_x = -delta_x / FREYA_TOUCHPAD_SPEED * device_pixel_ratio;
                event.wheel_delta_y = -delta_y / FREYA_TOUCHPAD_SPEED * device_pixel_ratio;
                event.wheel_tick_120ths_x = event.wheel_delta_x / PIXELS_PER_NOTCH * TICKS_PER_NOTCH;
                event.wheel_tick_120ths_y = event.wheel_delta_y / PIXELS_PER_NOTCH * TICKS_PER_NOTCH;
                event.wheel_delta_precision = Web_WheelDeltaPrecision::Precise;
            }
        }
        event
    }
}
