use std::fmt::Display;

use egui::{Context, Key};
use gilrs::{Button, GamepadId};
use serde::{Deserialize, Serialize};

pub struct Inputs {
    pub up: InputType,
    pub down: InputType,
    pub left: InputType,
    pub right: InputType,
    pub a: InputType,
    pub b: InputType,
    pub select: InputType,
    pub start: InputType,
    pub gilrs: gilrs::Gilrs,
    egui_ctx: Context,
}

impl Inputs {
    pub fn new(gilrs: gilrs::Gilrs, egui_ctx: Context) -> Self {
        Inputs {
            up: InputType::Keyboard(Key::ArrowUp),
            down: InputType::Keyboard(Key::ArrowDown),
            left: InputType::Keyboard(Key::ArrowLeft),
            right: InputType::Keyboard(Key::ArrowRight),
            a: InputType::Keyboard(Key::Z),
            b: InputType::Keyboard(Key::A),
            select: InputType::Keyboard(Key::Q),
            start: InputType::Keyboard(Key::Enter),
            gilrs,
            egui_ctx,
        }
    }

    pub fn with_state(gilrs: gilrs::Gilrs, egui_ctx: Context, state: InputsState) -> Self {
        let mut inputs = Self::new(gilrs, egui_ctx);
        inputs.load(state);
        inputs
    }

    pub fn update_buttons(&mut self, gb_button: GBButton) -> bool {
        //Check for KB key presses
        let mut input_type = InputType::None;
        self.egui_ctx.input(|i| {
            for key in i.keys_down.iter() {
                input_type = InputType::Keyboard(*key);
            }
        });
        if let InputType::Keyboard(_) = input_type {
            self.set_button(gb_button, input_type);
            return true;
        }
        //Check for gampad key presses
        while let Some(gilrs::Event { id, event, time: _ }) = self.gilrs.next_event() {
            if let gilrs::EventType::ButtonPressed(button, _code) = event {
                let input_type = InputType::Gamepad((id, button));
                self.set_button(gb_button, input_type);
                return true;
            }
        }
        false
    }

    pub fn pressed(&mut self, gb_button: GBButton) -> bool {
        match gb_button {
            GBButton::Up => self.up.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::Down => self.down.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::Left => self.left.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::Right => self.right.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::A => self.a.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::B => self.b.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::Select => self.select.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::Start => self.start.pressed(&self.gilrs, &self.egui_ctx),
            GBButton::None => false,
        }
    }

    pub fn pressed_all(&mut self) -> [bool; 8] {
        [
            self.a.pressed(&self.gilrs, &self.egui_ctx),
            self.b.pressed(&self.gilrs, &self.egui_ctx),
            self.select.pressed(&self.gilrs, &self.egui_ctx),
            self.start.pressed(&self.gilrs, &self.egui_ctx),
            self.right.pressed(&self.gilrs, &self.egui_ctx),
            self.left.pressed(&self.gilrs, &self.egui_ctx),
            self.up.pressed(&self.gilrs, &self.egui_ctx),
            self.down.pressed(&self.gilrs, &self.egui_ctx),
        ]
    }

    pub fn set_button(&mut self, gb_button: GBButton, input: InputType) {
        match gb_button {
            GBButton::Up => self.up.set_button(input),
            GBButton::Down => self.down.set_button(input),
            GBButton::Left => self.left.set_button(input),
            GBButton::Right => self.right.set_button(input),
            GBButton::A => self.a.set_button(input),
            GBButton::B => self.b.set_button(input),
            GBButton::Select => self.select.set_button(input),
            GBButton::Start => self.start.set_button(input),
            GBButton::None => {}
        }
    }

    pub fn save(&self) -> InputsState {
        InputsState {
            up: self.up.clone(),
            down: self.down.clone(),
            left: self.left.clone(),
            right: self.right.clone(),
            a: self.a.clone(),
            b: self.b.clone(),
            select: self.select.clone(),
            start: self.start.clone(),
        }
    }

    pub fn load(&mut self, state: InputsState) {
        self.up = state.up;
        self.down = state.down;
        self.left = state.left;
        self.right = state.right;
        self.a = state.a;
        self.b = state.b;
        self.select = state.select;
        self.start = state.start;
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InputsState {
    up: InputType,
    down: InputType,
    left: InputType,
    right: InputType,
    a: InputType,
    b: InputType,
    select: InputType,
    start: InputType,
}

impl Default for InputsState {
    fn default() -> Self {
        Self {
            up: InputType::Keyboard(Key::ArrowUp),
            down: InputType::Keyboard(Key::ArrowDown),
            left: InputType::Keyboard(Key::ArrowLeft),
            right: InputType::Keyboard(Key::ArrowRight),
            a: InputType::Keyboard(Key::Z),
            b: InputType::Keyboard(Key::A),
            select: InputType::Keyboard(Key::Q),
            start: InputType::Keyboard(Key::Enter),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum InputType {
    Gamepad((GamepadId, Button)),
    Keyboard(Key),
    None,
}

impl InputType {
    fn pressed(&mut self, gilrs: &gilrs::Gilrs, egui_ctx: &Context) -> bool {
        match *self {
            InputType::Gamepad((id, button)) => match &mut gilrs.connected_gamepad(id) {
                Some(gamepad) => gamepad.is_pressed(button),
                None => false,
            },
            InputType::Keyboard(key) => {
                let mut pressed = false;
                egui_ctx.input(|i| pressed = i.key_down(key));
                pressed
            }
            InputType::None => false,
        }
    }

    pub fn set_button(&mut self, button: InputType) {
        match button {
            InputType::None => {}
            _ => *self = button,
        }
    }
}

impl Display for InputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            InputType::Gamepad((id, button)) => write!(f, "Gamepad: {id} - {button:#?}"),
            InputType::Keyboard(key) => write!(f, "Keyboard: {key:#?}"),
            InputType::None => write!(f, ""),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum GBButton {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    Select,
    Start,
    None,
}
