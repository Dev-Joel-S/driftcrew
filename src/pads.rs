//! Gamepads und Joy-Cons: eigene Anbindung an gilrs (ersetzt bevys `GilrsPlugin`).
//!
//! Ein einzelner Joy-Con wird zu einem kleinen Standard-Gamepad umgerechnet, damit Menüs, Slots
//! und Zielen überall gleich funktionieren – quer oder hochkant gehalten:
//! die vier Gesichtstasten nach ihrer Lage (unten = South, rechts = East …), SL/SR als
//! Schultertasten, −/+ als Start, Aufnahme/Home als Select, der Stick als linker Stick.
//! Der Griff lässt sich pro Joy-Con in der Lobby umschalten (Stick drücken).
//!
//! Unter Linux lesen wir Joy-Cons über die evdev-Codes des Kerneltreibers (hid-nintendo): Die
//! SDL-Zuordnungstabelle in gilrs passt nicht zu den Tasten, die der Treiber meldet (dort wurde
//! z. B. ← zu Start). Auf anderen Systemen gilt die SDL-Zuordnung (für quer gehaltene Joy-Cons)
//! und wird von dort aus umgerechnet. Alle anderen Gamepads laufen unverändert durch.

use std::sync::RwLock;

use bevy::ecs::system::SystemParam;
use bevy::input::InputSystems;
use bevy::input::gamepad::{
    GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
    RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
};
use bevy::platform::cell::SyncCell;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use gilrs::ev::filter::axis_dpad_to_button;
use gilrs::{EventType, Filter, GilrsBuilder, MappingSource};
use serde::{Deserialize, Serialize};

/// Welcher Joy-Con.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Wie ein einzelner Joy-Con gehalten wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Grip {
    /// Waagerecht, Schiene mit SL/SR oben, Stick links.
    #[default]
    Sideways,
    /// Senkrecht wie eine Hälfte eines normalen Controllers.
    Upright,
}

impl Grip {
    pub fn label(self) -> &'static str {
        match self {
            Grip::Sideways => "quer",
            Grip::Upright => "hochkant",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Grip::Sideways => Grip::Upright,
            Grip::Upright => Grip::Sideways,
        }
    }
}

/// Tasten eines einzelnen Joy-Cons, benannt nach dem Aufdruck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phys {
    /// Pfeiltasten (linker Joy-Con).
    Up,
    Down,
    Left,
    Right,
    /// Buchstabentasten (rechter Joy-Con).
    A,
    B,
    X,
    Y,
    Sl,
    Sr,
    /// L bzw. R oben am Joy-Con.
    Shoulder,
    /// ZL bzw. ZR.
    Trigger,
    /// − bzw. +.
    Menu,
    /// Aufnahme bzw. Home.
    Extra,
    /// Stick drücken.
    Stick,
}

pub const ALL_PHYS: [Phys; 15] = [
    Phys::Up,
    Phys::Down,
    Phys::Left,
    Phys::Right,
    Phys::A,
    Phys::B,
    Phys::X,
    Phys::Y,
    Phys::Sl,
    Phys::Sr,
    Phys::Shoulder,
    Phys::Trigger,
    Phys::Menu,
    Phys::Extra,
    Phys::Stick,
];

/// Beschriftung wie auf dem Joy-Con.
pub fn phys_label(side: Side, p: Phys) -> &'static str {
    match (side, p) {
        (_, Phys::Up) => "↑",
        (_, Phys::Down) => "↓",
        (_, Phys::Left) => "←",
        (_, Phys::Right) => "→",
        (_, Phys::A) => "A",
        (_, Phys::B) => "B",
        (_, Phys::X) => "X",
        (_, Phys::Y) => "Y",
        (_, Phys::Sl) => "SL",
        (_, Phys::Sr) => "SR",
        (Side::Left, Phys::Shoulder) => "L",
        (Side::Right, Phys::Shoulder) => "R",
        (Side::Left, Phys::Trigger) => "ZL",
        (Side::Right, Phys::Trigger) => "ZR",
        (Side::Left, Phys::Menu) => "−",
        (Side::Right, Phys::Menu) => "+",
        (Side::Left, Phys::Extra) => "Foto",
        (Side::Right, Phys::Extra) => "Home",
        (_, Phys::Stick) => "Stick",
    }
}

// evdev-Codes (linux/input-event-codes.h), wie sie hid-nintendo für Joy-Cons meldet.
const BTN_SOUTH: u16 = 0x130;
const BTN_EAST: u16 = 0x131;
const BTN_NORTH: u16 = 0x133;
const BTN_WEST: u16 = 0x134;
const BTN_Z: u16 = 0x135;
const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
const BTN_TL2: u16 = 0x138;
const BTN_TR2: u16 = 0x139;
const BTN_SELECT: u16 = 0x13a;
const BTN_START: u16 = 0x13b;
const BTN_MODE: u16 = 0x13c;
const BTN_THUMBL: u16 = 0x13d;
const BTN_THUMBR: u16 = 0x13e;
const BTN_DPAD_UP: u16 = 0x220;
const BTN_DPAD_DOWN: u16 = 0x221;
const BTN_DPAD_LEFT: u16 = 0x222;
const BTN_DPAD_RIGHT: u16 = 0x223;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;

/// evdev-Tastencode → Joy-Con-Taste (Linux, Treiber hid-nintendo).
pub fn phys_from_evdev(side: Side, code: u16) -> Option<Phys> {
    Some(match (side, code) {
        (Side::Left, BTN_DPAD_UP) => Phys::Up,
        (Side::Left, BTN_DPAD_DOWN) => Phys::Down,
        (Side::Left, BTN_DPAD_LEFT) => Phys::Left,
        (Side::Left, BTN_DPAD_RIGHT) => Phys::Right,
        (Side::Left, BTN_TL) => Phys::Shoulder,
        (Side::Left, BTN_TL2) => Phys::Trigger,
        (Side::Left, BTN_TR) => Phys::Sl,
        (Side::Left, BTN_TR2) => Phys::Sr,
        (Side::Left, BTN_SELECT) => Phys::Menu,
        (Side::Left, BTN_Z) => Phys::Extra,
        (Side::Left, BTN_THUMBL) => Phys::Stick,
        (Side::Right, BTN_EAST) => Phys::A,
        (Side::Right, BTN_SOUTH) => Phys::B,
        (Side::Right, BTN_NORTH) => Phys::X,
        (Side::Right, BTN_WEST) => Phys::Y,
        (Side::Right, BTN_TR) => Phys::Shoulder,
        (Side::Right, BTN_TR2) => Phys::Trigger,
        (Side::Right, BTN_TL) => Phys::Sl,
        (Side::Right, BTN_TL2) => Phys::Sr,
        (Side::Right, BTN_START) => Phys::Menu,
        (Side::Right, BTN_MODE) => Phys::Extra,
        (Side::Right, BTN_THUMBR) => Phys::Stick,
        _ => return None,
    })
}

/// SDL-Zuordnung (quer gehalten; Windows/macOS) → Joy-Con-Taste.
pub fn phys_from_sdl(side: Side, b: GamepadButton) -> Option<Phys> {
    use GamepadButton as G;
    Some(match (side, b) {
        (Side::Left, G::South) => Phys::Left,
        (Side::Left, G::East) => Phys::Down,
        (Side::Left, G::West) => Phys::Up,
        (Side::Left, G::North) => Phys::Right,
        (Side::Right, G::South) => Phys::A,
        (Side::Right, G::East) => Phys::X,
        (Side::Right, G::West) => Phys::B,
        (Side::Right, G::North) => Phys::Y,
        (_, G::LeftTrigger) => Phys::Sl,
        (_, G::RightTrigger) => Phys::Sr,
        (_, G::Start) => Phys::Menu,
        (_, G::Select) => Phys::Extra,
        (_, G::LeftThumb) => Phys::Stick,
        _ => return None,
    })
}

/// Joy-Con-Taste → Taste des virtuellen Standard-Gamepads, je nach Griff.
pub fn virt(side: Side, grip: Grip, p: Phys) -> Option<GamepadButton> {
    use GamepadButton as G;
    let b = match (grip, side, p) {
        // Quer: Lage der vier Tasten, SL/SR als Schultertasten.
        (Grip::Sideways, Side::Left, Phys::Left) => G::South,
        (Grip::Sideways, Side::Left, Phys::Down) => G::East,
        (Grip::Sideways, Side::Left, Phys::Up) => G::West,
        (Grip::Sideways, Side::Left, Phys::Right) => G::North,
        (Grip::Sideways, Side::Right, Phys::A) => G::South,
        (Grip::Sideways, Side::Right, Phys::X) => G::East,
        (Grip::Sideways, Side::Right, Phys::B) => G::West,
        (Grip::Sideways, Side::Right, Phys::Y) => G::North,
        (Grip::Sideways, _, Phys::Sl) => G::LeftTrigger,
        (Grip::Sideways, _, Phys::Sr) => G::RightTrigger,
        (Grip::Sideways, _, Phys::Shoulder) => G::LeftTrigger2,
        (Grip::Sideways, _, Phys::Trigger) => G::RightTrigger2,
        // Hochkant: Lage wie gedruckt, L/ZL bzw. R/ZR oben, SL/SR an der Schiene.
        (Grip::Upright, Side::Left, Phys::Up) => G::North,
        (Grip::Upright, Side::Left, Phys::Right) => G::East,
        (Grip::Upright, Side::Left, Phys::Down) => G::South,
        (Grip::Upright, Side::Left, Phys::Left) => G::West,
        (Grip::Upright, Side::Left, Phys::Shoulder) => G::LeftTrigger,
        (Grip::Upright, Side::Left, Phys::Trigger) => G::LeftTrigger2,
        (Grip::Upright, Side::Left, Phys::Sl) => G::RightTrigger,
        (Grip::Upright, Side::Left, Phys::Sr) => G::RightTrigger2,
        (Grip::Upright, Side::Right, Phys::X) => G::North,
        (Grip::Upright, Side::Right, Phys::A) => G::East,
        (Grip::Upright, Side::Right, Phys::B) => G::South,
        (Grip::Upright, Side::Right, Phys::Y) => G::West,
        (Grip::Upright, Side::Right, Phys::Shoulder) => G::RightTrigger,
        (Grip::Upright, Side::Right, Phys::Trigger) => G::RightTrigger2,
        (Grip::Upright, Side::Right, Phys::Sl) => G::LeftTrigger,
        (Grip::Upright, Side::Right, Phys::Sr) => G::LeftTrigger2,
        (_, _, Phys::Menu) => G::Start,
        (_, _, Phys::Extra) => G::Select,
        (_, _, Phys::Stick) => G::LeftThumb,
        _ => return None,
    };
    Some(b)
}

/// Umkehrung von [`virt`]: welche Joy-Con-Taste steckt hinter der virtuellen Taste?
pub fn phys_of(side: Side, grip: Grip, b: GamepadButton) -> Option<Phys> {
    ALL_PHYS
        .into_iter()
        .find(|p| virt(side, grip, *p) == Some(b))
}

/// Stick im Hochkant-Bezug (x zur Schiene bzw. von ihr weg, y zum oberen Ende) → virtueller
/// linker Stick, so wie man den Joy-Con gerade hält.
pub fn virt_stick(side: Side, grip: Grip, s: Vec2) -> Vec2 {
    match (grip, side) {
        (Grip::Upright, _) => s,
        // Linker Joy-Con quer: das obere Ende zeigt nach links.
        (Grip::Sideways, Side::Left) => Vec2::new(-s.y, s.x),
        // Rechter Joy-Con quer: das obere Ende zeigt nach rechts.
        (Grip::Sideways, Side::Right) => Vec2::new(s.y, -s.x),
    }
}

/// Umkehrung von [`virt_stick`] für quer gehaltene Joy-Cons (SDL-Zuordnung).
pub fn upright_from_sideways(side: Side, v: Vec2) -> Vec2 {
    match side {
        Side::Left => Vec2::new(v.y, -v.x),
        Side::Right => Vec2::new(-v.y, v.x),
    }
}

/// Einzelner Joy-Con? (Nintendo-Kennung 057e:2006/2007, sonst über den Namen.)
pub fn joycon_side(vendor: Option<u16>, product: Option<u16>, name: &str) -> Option<Side> {
    if vendor == Some(0x057e) {
        return match product {
            Some(0x2006) => Some(Side::Left),
            Some(0x2007) => Some(Side::Right),
            _ => None,
        };
    }
    let n = name.to_lowercase();
    if !(n.contains("joy-con") || n.contains("joycon"))
        || n.contains("combined")
        || n.contains("(l/r)")
    {
        return None;
    }
    if n.contains("(l)") || n.contains("left") {
        Some(Side::Left)
    } else if n.contains("(r)") || n.contains("right") {
        Some(Side::Right)
    } else {
        None
    }
}

/// Verbundene Joy-Cons mit Seite und Griff. Den Griff hier ändern schaltet um (Lobby).
#[derive(Resource, Default, Clone, Debug)]
pub struct JoyCons(pub Vec<JoyConInfo>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JoyConInfo {
    pub pad: Entity,
    pub side: Side,
    pub grip: Grip,
}

impl JoyCons {
    pub fn get(&self, pad: Entity) -> Option<JoyConInfo> {
        self.0.iter().find(|i| i.pad == pad).copied()
    }

    pub fn set_grip(&mut self, pad: Entity, grip: Grip) {
        if let Some(i) = self.0.iter_mut().find(|i| i.pad == pad) {
            i.grip = grip;
        }
    }
}

/// Spiegel von [`JoyCons`] für Tastenbeschriftungen (die kennen nur Gerät und Taste).
static LABELS: RwLock<Vec<JoyConInfo>> = RwLock::new(Vec::new());

/// Beschriftung einer virtuellen Taste, wie sie auf dem Joy-Con steht (None = kein Joy-Con).
pub fn button_label(pad: Entity, b: GamepadButton) -> Option<String> {
    let info = LABELS.read().ok()?.iter().find(|i| i.pad == pad).copied()?;
    let p = phys_of(info.side, info.grip, b)?;
    Some(phys_label(info.side, p).to_string())
}

#[derive(Resource)]
struct GilrsCell(SyncCell<gilrs::Gilrs>);

struct Pad {
    entity: Entity,
    joycon: Option<JoyCon>,
}

struct JoyCon {
    side: Side,
    /// Linux: Tasten über evdev-Codes lesen; sonst über die SDL-Zuordnung (quer).
    evdev: bool,
    grip: Grip,
    /// Stick im Hochkant-Bezug.
    stick: Vec2,
    held: Vec<Phys>,
}

#[derive(Resource, Default)]
struct PadMap(HashMap<gilrs::GamepadId, Pad>);

pub struct PadsPlugin;

impl Plugin for PadsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JoyCons>();
        match GilrsBuilder::new()
            .with_default_filters(false)
            .set_update_state(false)
            .build()
        {
            Ok(g) => {
                app.insert_resource(GilrsCell(SyncCell::new(g)))
                    .init_resource::<PadMap>()
                    .add_systems(PreStartup, connect_present)
                    .add_systems(PreUpdate, read_pads.before(InputSystems));
            }
            Err(e) => error!("Gamepads nicht verfügbar: {e}"),
        }
    }
}

/// Schreibt Gamepad-Ereignisse so, wie bevy sie von seinem gilrs-Plugin erwartet.
#[derive(SystemParam)]
struct Out<'w> {
    raw: MessageWriter<'w, RawGamepadEvent>,
    conn: MessageWriter<'w, GamepadConnectionEvent>,
    buttons: MessageWriter<'w, RawGamepadButtonChangedEvent>,
    axes: MessageWriter<'w, RawGamepadAxisChangedEvent>,
}

impl Out<'_> {
    fn button(&mut self, pad: Entity, b: GamepadButton, value: f32) {
        let e = RawGamepadButtonChangedEvent::new(pad, b, value);
        self.raw.write(e.into());
        self.buttons.write(e);
    }

    fn axis(&mut self, pad: Entity, a: GamepadAxis, value: f32) {
        let e = RawGamepadAxisChangedEvent::new(pad, a, value);
        self.raw.write(e.into());
        self.axes.write(e);
    }

    fn stick(&mut self, pad: Entity, jc: &JoyCon) {
        let s = virt_stick(jc.side, jc.grip, jc.stick);
        self.axis(pad, GamepadAxis::LeftStickX, s.x);
        self.axis(pad, GamepadAxis::LeftStickY, s.y);
    }

    fn connection(&mut self, e: GamepadConnectionEvent) {
        self.raw.write(e.clone().into());
        self.conn.write(e);
    }
}

#[allow(clippy::too_many_arguments)]
fn connect(
    id: gilrs::GamepadId,
    g: &gilrs::Gilrs,
    commands: &mut Commands,
    map: &mut PadMap,
    joycons: &mut JoyCons,
    default_grip: Grip,
    out: &mut Out,
) {
    let gp = g.gamepad(id);
    let evdev = cfg!(target_os = "linux");
    let side = joycon_side(gp.vendor_id(), gp.product_id(), gp.os_name())
        .or_else(|| joycon_side(gp.vendor_id(), gp.product_id(), gp.name()))
        // Ohne Linux-Codes geht es nur mit der bekannten SDL-Zuordnung.
        .filter(|_| evdev || gp.mapping_source() == MappingSource::SdlMappings);
    let entity = map
        .0
        .get(&id)
        .map(|p| p.entity)
        .unwrap_or_else(|| commands.spawn_empty().id());
    let grip = joycons.get(entity).map_or(default_grip, |i| i.grip);
    joycons.0.retain(|i| i.pad != entity);
    if let Some(side) = side {
        joycons.0.push(JoyConInfo {
            pad: entity,
            side,
            grip,
        });
    }
    let name = match side {
        Some(Side::Left) => "Joy-Con (L)".to_string(),
        Some(Side::Right) => "Joy-Con (R)".to_string(),
        None => gp.name().to_string(),
    };
    map.0.insert(
        id,
        Pad {
            entity,
            joycon: side.map(|side| JoyCon {
                side,
                evdev,
                grip,
                stick: Vec2::ZERO,
                held: Vec::new(),
            }),
        },
    );
    out.connection(GamepadConnectionEvent::new(
        entity,
        GamepadConnection::Connected {
            name,
            vendor_id: gp.vendor_id(),
            product_id: gp.product_id(),
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn connect_present(
    mut commands: Commands,
    mut gilrs: ResMut<GilrsCell>,
    mut map: ResMut<PadMap>,
    mut joycons: ResMut<JoyCons>,
    settings: Option<Res<crate::settings::Settings>>,
    mut out: Out,
) {
    let grip = settings.map(|s| s.joycon_grip).unwrap_or_default();
    let g = gilrs.0.get();
    let ids: Vec<gilrs::GamepadId> = g.gamepads().map(|(id, _)| id).collect();
    for id in ids {
        connect(id, g, &mut commands, &mut map, &mut joycons, grip, &mut out);
    }
    mirror_labels(&joycons);
}

/// Linux: Code eines evdev-Ereignisses (Typ 1 = Taste, 3 = Achse).
fn evdev_code(code: gilrs::ev::Code, kind: u32) -> Option<u16> {
    let u = code.into_u32();
    (u >> 16 == kind).then_some((u & 0xffff) as u16)
}

const EV_KEY: u32 = 1;
const EV_ABS: u32 = 3;

/// Joy-Con-Taste gedrückt/losgelassen: merken und als virtuelle Taste weitergeben.
fn joycon_button(pad: Entity, jc: &mut JoyCon, p: Phys, value: f32, out: &mut Out) {
    jc.held.retain(|h| *h != p);
    if value > 0.5 {
        jc.held.push(p);
    }
    if let Some(b) = virt(jc.side, jc.grip, p) {
        out.button(pad, b, value);
    }
}

#[allow(clippy::too_many_arguments)]
fn read_pads(
    mut commands: Commands,
    mut gilrs: ResMut<GilrsCell>,
    mut map: ResMut<PadMap>,
    mut joycons: ResMut<JoyCons>,
    settings: Option<Res<crate::settings::Settings>>,
    mut out: Out,
) {
    let default_grip = settings.map(|s| s.joycon_grip).unwrap_or_default();
    // Griff gewechselt (Lobby): gehaltene Tasten loslassen, Stick neu drehen.
    for pad in map.0.values_mut() {
        let Some(jc) = &mut pad.joycon else { continue };
        let Some(info) = joycons.get(pad.entity) else {
            continue;
        };
        if info.grip == jc.grip {
            continue;
        }
        for p in std::mem::take(&mut jc.held) {
            if let Some(b) = virt(jc.side, jc.grip, p) {
                out.button(pad.entity, b, 0.0);
            }
        }
        jc.grip = info.grip;
        out.stick(pad.entity, jc);
    }

    let g = gilrs.0.get();
    while let Some(ev) = g.next_event().filter_ev(&axis_dpad_to_button, g) {
        g.update(&ev);
        match ev.event {
            EventType::Connected => {
                connect(
                    ev.id,
                    g,
                    &mut commands,
                    &mut map,
                    &mut joycons,
                    default_grip,
                    &mut out,
                );
            }
            EventType::Disconnected => {
                let Some(pad) = map.0.get_mut(&ev.id) else {
                    continue;
                };
                if let Some(jc) = &mut pad.joycon {
                    jc.held.clear();
                    jc.stick = Vec2::ZERO;
                }
                out.connection(GamepadConnectionEvent::new(
                    pad.entity,
                    GamepadConnection::Disconnected,
                ));
            }
            EventType::ButtonChanged(b, value, code) => {
                let Some(pad) = map.0.get_mut(&ev.id) else {
                    continue;
                };
                let e = pad.entity;
                match &mut pad.joycon {
                    Some(jc) => {
                        let p = if jc.evdev {
                            evdev_code(code, EV_KEY).and_then(|c| phys_from_evdev(jc.side, c))
                        } else {
                            bevy_button(b).and_then(|vb| phys_from_sdl(jc.side, vb))
                        };
                        if let Some(p) = p {
                            joycon_button(e, jc, p, value, &mut out);
                        }
                    }
                    None => {
                        if let Some(vb) = bevy_button(b) {
                            out.button(e, vb, value);
                        }
                    }
                }
            }
            EventType::AxisChanged(a, value, code) => {
                let Some(pad) = map.0.get_mut(&ev.id) else {
                    continue;
                };
                let e = pad.entity;
                match &mut pad.joycon {
                    Some(jc) if jc.evdev => {
                        // Eine Taste, die eine Zuordnung auf eine Achse gelegt hat.
                        if let Some(c) = evdev_code(code, EV_KEY) {
                            if let Some(p) = phys_from_evdev(jc.side, c) {
                                joycon_button(e, jc, p, value.abs(), &mut out);
                            }
                            continue;
                        }
                        let Some(c) = evdev_code(code, EV_ABS) else {
                            continue;
                        };
                        // gilrs dreht Y-Achsen unter Linux um – hier zählt der Rohwert.
                        let native = if matches!(
                            a,
                            gilrs::Axis::LeftStickY | gilrs::Axis::RightStickY | gilrs::Axis::DPadY
                        ) {
                            -value
                        } else {
                            value
                        };
                        match c {
                            ABS_X | ABS_RX => jc.stick.x = native,
                            // evdev: unten positiv.
                            ABS_Y | ABS_RY => jc.stick.y = -native,
                            _ => continue,
                        }
                        out.stick(e, jc);
                    }
                    Some(jc) => {
                        let mut v = virt_stick(jc.side, Grip::Sideways, jc.stick);
                        match a {
                            gilrs::Axis::LeftStickX => v.x = value,
                            gilrs::Axis::LeftStickY => v.y = value,
                            _ => continue,
                        }
                        jc.stick = upright_from_sideways(jc.side, v);
                        out.stick(e, jc);
                    }
                    None => {
                        if let Some(ax) = bevy_axis(a) {
                            out.axis(e, ax, value);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    g.inc();
    mirror_labels(&joycons);
}

fn mirror_labels(joycons: &JoyCons) {
    if let Ok(mut l) = LABELS.write()
        && *l != joycons.0
    {
        *l = joycons.0.clone();
    }
}

/// Wie bevys gilrs-Plugin: gilrs-Taste → bevy-Taste.
fn bevy_button(b: gilrs::Button) -> Option<GamepadButton> {
    use GamepadButton as G;
    use gilrs::Button as B;
    Some(match b {
        B::South => G::South,
        B::East => G::East,
        B::North => G::North,
        B::West => G::West,
        B::C => G::C,
        B::Z => G::Z,
        B::LeftTrigger => G::LeftTrigger,
        B::LeftTrigger2 => G::LeftTrigger2,
        B::RightTrigger => G::RightTrigger,
        B::RightTrigger2 => G::RightTrigger2,
        B::Select => G::Select,
        B::Start => G::Start,
        B::Mode => G::Mode,
        B::LeftThumb => G::LeftThumb,
        B::RightThumb => G::RightThumb,
        B::DPadUp => G::DPadUp,
        B::DPadDown => G::DPadDown,
        B::DPadLeft => G::DPadLeft,
        B::DPadRight => G::DPadRight,
        B::Unknown => return None,
    })
}

/// Wie bevys gilrs-Plugin: gilrs-Achse → bevy-Achse (Steuerkreuz-Achsen werden zu Tasten).
fn bevy_axis(a: gilrs::Axis) -> Option<GamepadAxis> {
    use GamepadAxis as A;
    use gilrs::Axis as X;
    Some(match a {
        X::LeftStickX => A::LeftStickX,
        X::LeftStickY => A::LeftStickY,
        X::LeftZ => A::LeftZ,
        X::RightStickX => A::RightStickX,
        X::RightStickY => A::RightStickY,
        X::RightZ => A::RightZ,
        X::Unknown | X::DPadX | X::DPadY => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use GamepadButton as G;

    #[test]
    fn every_joycon_button_has_exactly_one_virtual_button_per_grip() {
        for side in [Side::Left, Side::Right] {
            for grip in [Grip::Sideways, Grip::Upright] {
                let own: Vec<Phys> = ALL_PHYS
                    .into_iter()
                    .filter(|p| match p {
                        Phys::Up | Phys::Down | Phys::Left | Phys::Right => side == Side::Left,
                        Phys::A | Phys::B | Phys::X | Phys::Y => side == Side::Right,
                        _ => true,
                    })
                    .collect();
                let mut seen = Vec::new();
                for p in own {
                    let b = virt(side, grip, p).expect("jede Taste kommt an");
                    assert!(!seen.contains(&b), "{side:?} {grip:?}: {b:?} doppelt");
                    seen.push(b);
                    assert_eq!(phys_of(side, grip, b), Some(p), "Umkehrung");
                }
                // Vier Gesichtstasten, Start und Select gibt es immer – Menüs gehen.
                for b in [G::South, G::East, G::North, G::West, G::Start, G::Select] {
                    assert!(seen.contains(&b), "{side:?} {grip:?} ohne {b:?}");
                }
            }
        }
    }

    #[test]
    fn sideways_left_joycon_uses_the_position_of_the_arrow_buttons() {
        // Quer gehalten liegt ← unten, ↓ rechts, ↑ links, → oben.
        let s = |p| virt(Side::Left, Grip::Sideways, p);
        assert_eq!(s(Phys::Left), Some(G::South));
        assert_eq!(s(Phys::Down), Some(G::East));
        assert_eq!(s(Phys::Up), Some(G::West));
        assert_eq!(s(Phys::Right), Some(G::North));
        assert_eq!(s(Phys::Menu), Some(G::Start));
        // Hochkant zählt der Aufdruck.
        let u = |p| virt(Side::Left, Grip::Upright, p);
        assert_eq!(u(Phys::Down), Some(G::South));
        assert_eq!(u(Phys::Left), Some(G::West));
    }

    #[test]
    fn evdev_codes_of_hid_nintendo() {
        // Der Fehler aus dem Test: ← am linken Joy-Con darf nie Start sein.
        let left = phys_from_evdev(Side::Left, BTN_DPAD_LEFT).unwrap();
        assert_eq!(left, Phys::Left);
        assert_ne!(virt(Side::Left, Grip::Sideways, left), Some(G::Start));
        assert_eq!(phys_from_evdev(Side::Left, BTN_SELECT), Some(Phys::Menu));
        assert_eq!(phys_from_evdev(Side::Left, BTN_TR), Some(Phys::Sl));
        assert_eq!(phys_from_evdev(Side::Right, BTN_EAST), Some(Phys::A));
        assert_eq!(phys_from_evdev(Side::Right, BTN_TL2), Some(Phys::Sr));
        assert_eq!(phys_from_evdev(Side::Right, BTN_START), Some(Phys::Menu));
        assert_eq!(phys_from_evdev(Side::Right, BTN_DPAD_LEFT), None);
    }

    #[test]
    fn stick_turns_with_the_grip() {
        let up = Vec2::Y; // zum oberen Ende des Joy-Cons
        let rail = Vec2::X; // linker Joy-Con: zur Schiene
        assert_eq!(
            virt_stick(Side::Left, Grip::Sideways, up),
            Vec2::new(-1.0, 0.0)
        );
        assert_eq!(
            virt_stick(Side::Left, Grip::Sideways, rail),
            Vec2::new(0.0, 1.0)
        );
        assert_eq!(
            virt_stick(Side::Right, Grip::Sideways, up),
            Vec2::new(1.0, 0.0)
        );
        assert_eq!(virt_stick(Side::Right, Grip::Upright, up), up);
        for side in [Side::Left, Side::Right] {
            let s = Vec2::new(0.3, -0.7);
            let v = virt_stick(side, Grip::Sideways, s);
            assert!((upright_from_sideways(side, v) - s).length() < 1e-6);
        }
        // SDL (quer) und evdev ergeben dieselbe Taste an derselben Stelle.
        for b in [G::South, G::East, G::North, G::West] {
            for side in [Side::Left, Side::Right] {
                let p = phys_from_sdl(side, b).unwrap();
                assert_eq!(virt(side, Grip::Sideways, p), Some(b));
            }
        }
    }

    #[test]
    fn recognises_single_joycons_only() {
        assert_eq!(
            joycon_side(Some(0x057e), Some(0x2006), ""),
            Some(Side::Left)
        );
        assert_eq!(
            joycon_side(Some(0x057e), Some(0x2007), ""),
            Some(Side::Right)
        );
        assert_eq!(joycon_side(Some(0x057e), Some(0x2008), "x"), None);
        assert_eq!(
            joycon_side(None, None, "Nintendo Switch Left Joy-Con"),
            Some(Side::Left)
        );
        assert_eq!(joycon_side(None, None, "Joy-Con (R)"), Some(Side::Right));
        assert_eq!(
            joycon_side(None, None, "Nintendo Switch Combined Joy-Cons"),
            None
        );
        assert_eq!(joycon_side(None, None, "Xbox Wireless Controller"), None);
    }
}
