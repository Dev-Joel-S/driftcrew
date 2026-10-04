//! Gamepads und Joy-Cons: eigene Anbindung an gilrs (ersetzt bevys `GilrsPlugin`).
//!
//! Ein einzelner Joy-Con wird zu einem kleinen Standard-Gamepad umgerechnet, damit Menüs, Slots
//! und Zielen überall gleich funktionieren. Gehalten wird er **hochkant** (wie eine Hälfte eines
//! Controllers): die vier Tasten nach ihrer Lage (unten = South, rechts = East …), L/ZL bzw.
//! R/ZR oben, SL/SR an der Schiene, −/+ als Start, Aufnahme/Home als Select, der Stick als
//! linker Stick – ungedreht, oben ist oben.
//!
//! Unter Linux lesen wir Joy-Cons über die evdev-Codes des Kerneltreibers (hid-nintendo): Die
//! SDL-Zuordnungstabelle in gilrs passt nicht zu den Tasten, die der Treiber meldet (dort wurde
//! z. B. ← zu Start). Auf anderen Systemen gilt die SDL-Zuordnung – die ist für quer gehaltene
//! Joy-Cons gedacht und wird hier auf hochkant zurückgedreht. Alle anderen Gamepads laufen
//! unverändert durch.

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

/// Welcher Joy-Con.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
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

/// Joy-Con-Taste → Taste des virtuellen Standard-Gamepads (hochkant gehalten).
pub fn virt(side: Side, p: Phys) -> Option<GamepadButton> {
    use GamepadButton as G;
    let b = match (side, p) {
        // Lage wie gedruckt, L/ZL bzw. R/ZR oben, SL/SR an der Schiene.
        (Side::Left, Phys::Up) => G::North,
        (Side::Left, Phys::Right) => G::East,
        (Side::Left, Phys::Down) => G::South,
        (Side::Left, Phys::Left) => G::West,
        (Side::Left, Phys::Shoulder) => G::LeftTrigger,
        (Side::Left, Phys::Trigger) => G::LeftTrigger2,
        (Side::Left, Phys::Sl) => G::RightTrigger,
        (Side::Left, Phys::Sr) => G::RightTrigger2,
        (Side::Right, Phys::X) => G::North,
        (Side::Right, Phys::A) => G::East,
        (Side::Right, Phys::B) => G::South,
        (Side::Right, Phys::Y) => G::West,
        (Side::Right, Phys::Shoulder) => G::RightTrigger,
        (Side::Right, Phys::Trigger) => G::RightTrigger2,
        (Side::Right, Phys::Sl) => G::LeftTrigger,
        (Side::Right, Phys::Sr) => G::LeftTrigger2,
        (_, Phys::Menu) => G::Start,
        (_, Phys::Extra) => G::Select,
        (_, Phys::Stick) => G::LeftThumb,
        _ => return None,
    };
    Some(b)
}

/// Umkehrung von [`virt`]: welche Joy-Con-Taste steckt hinter der virtuellen Taste?
pub fn phys_of(side: Side, b: GamepadButton) -> Option<Phys> {
    ALL_PHYS.into_iter().find(|p| virt(side, *p) == Some(b))
}

/// SDL-Zuordnung (Windows/macOS) liefert den Stick quer gedreht – zurück auf hochkant
/// (x zur Schiene bzw. von ihr weg, y zum oberen Ende). Links und rechts drehen entgegengesetzt.
pub fn upright_from_sideways(side: Side, v: Vec2) -> Vec2 {
    match side {
        // Linker Joy-Con quer: das obere Ende zeigt nach links.
        Side::Left => Vec2::new(v.y, -v.x),
        // Rechter Joy-Con quer: das obere Ende zeigt nach rechts.
        Side::Right => Vec2::new(-v.y, v.x),
    }
}

/// Umkehrung von [`upright_from_sideways`].
pub fn sideways_from_upright(side: Side, s: Vec2) -> Vec2 {
    match side {
        Side::Left => Vec2::new(-s.y, s.x),
        Side::Right => Vec2::new(s.y, -s.x),
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

/// Verbundene einzelne Joy-Cons und ihre Seite.
#[derive(Resource, Default, Clone, Debug)]
pub struct JoyCons(pub Vec<JoyConInfo>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JoyConInfo {
    pub pad: Entity,
    pub side: Side,
}

/// Spiegel von [`JoyCons`] für Tastenbeschriftungen (die kennen nur Gerät und Taste).
static LABELS: RwLock<Vec<JoyConInfo>> = RwLock::new(Vec::new());

/// Beschriftung einer virtuellen Taste, wie sie auf dem Joy-Con steht (None = kein Joy-Con).
pub fn button_label(pad: Entity, b: GamepadButton) -> Option<String> {
    let info = LABELS.read().ok()?.iter().find(|i| i.pad == pad).copied()?;
    let p = phys_of(info.side, b)?;
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
    /// Stick, hochkant gehalten: oben = +y.
    stick: Vec2,
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
        self.axis(pad, GamepadAxis::LeftStickX, jc.stick.x);
        self.axis(pad, GamepadAxis::LeftStickY, jc.stick.y);
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
    joycons.0.retain(|i| i.pad != entity);
    if let Some(side) = side {
        joycons.0.push(JoyConInfo { pad: entity, side });
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
                stick: Vec2::ZERO,
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
    mut out: Out,
) {
    let g = gilrs.0.get();
    let ids: Vec<gilrs::GamepadId> = g.gamepads().map(|(id, _)| id).collect();
    for id in ids {
        connect(id, g, &mut commands, &mut map, &mut joycons, &mut out);
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

/// Joy-Con-Taste gedrückt/losgelassen: als virtuelle Taste weitergeben.
fn joycon_button(pad: Entity, jc: &JoyCon, p: Phys, value: f32, out: &mut Out) {
    if let Some(b) = virt(jc.side, p) {
        out.button(pad, b, value);
    }
}

#[allow(clippy::too_many_arguments)]
fn read_pads(
    mut commands: Commands,
    mut gilrs: ResMut<GilrsCell>,
    mut map: ResMut<PadMap>,
    mut joycons: ResMut<JoyCons>,
    mut out: Out,
) {
    let g = gilrs.0.get();
    while let Some(ev) = g.next_event().filter_ev(&axis_dpad_to_button, g) {
        g.update(&ev);
        match ev.event {
            EventType::Connected => {
                connect(ev.id, g, &mut commands, &mut map, &mut joycons, &mut out);
            }
            EventType::Disconnected => {
                let Some(pad) = map.0.get_mut(&ev.id) else {
                    continue;
                };
                if let Some(jc) = &mut pad.joycon {
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
                        let mut v = sideways_from_upright(jc.side, jc.stick);
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
    fn every_joycon_button_has_exactly_one_virtual_button() {
        for side in [Side::Left, Side::Right] {
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
                let b = virt(side, p).expect("jede Taste kommt an");
                assert!(!seen.contains(&b), "{side:?}: {b:?} doppelt");
                seen.push(b);
                assert_eq!(phys_of(side, b), Some(p), "Umkehrung");
            }
            // Vier Gesichtstasten, Start und Select gibt es immer – Menüs gehen.
            for b in [G::South, G::East, G::North, G::West, G::Start, G::Select] {
                assert!(seen.contains(&b), "{side:?} ohne {b:?}");
            }
        }
    }

    #[test]
    fn upright_buttons_follow_the_print() {
        let l = |p| virt(Side::Left, p);
        assert_eq!(l(Phys::Down), Some(G::South));
        assert_eq!(l(Phys::Right), Some(G::East));
        assert_eq!(l(Phys::Up), Some(G::North));
        assert_eq!(l(Phys::Left), Some(G::West));
        assert_eq!(l(Phys::Menu), Some(G::Start));
        let r = |p| virt(Side::Right, p);
        assert_eq!(r(Phys::B), Some(G::South));
        assert_eq!(r(Phys::A), Some(G::East));
    }

    #[test]
    fn evdev_codes_of_hid_nintendo() {
        // Der Fehler aus dem Test: ← am linken Joy-Con darf nie Start sein.
        let left = phys_from_evdev(Side::Left, BTN_DPAD_LEFT).unwrap();
        assert_eq!(left, Phys::Left);
        assert_ne!(virt(Side::Left, left), Some(G::Start));
        assert_eq!(phys_from_evdev(Side::Left, BTN_SELECT), Some(Phys::Menu));
        assert_eq!(phys_from_evdev(Side::Left, BTN_TR), Some(Phys::Sl));
        assert_eq!(phys_from_evdev(Side::Right, BTN_EAST), Some(Phys::A));
        assert_eq!(phys_from_evdev(Side::Right, BTN_TL2), Some(Phys::Sr));
        assert_eq!(phys_from_evdev(Side::Right, BTN_START), Some(Phys::Menu));
        assert_eq!(phys_from_evdev(Side::Right, BTN_DPAD_LEFT), None);
    }

    #[test]
    fn sdl_sticks_are_turned_back_upright_differently_per_side() {
        // Quer liefert SDL beim linken Joy-Con „nach oben (zum Stick-Ende)“ als links,
        // beim rechten als rechts – hochkant ist beides wieder oben.
        assert_eq!(
            upright_from_sideways(Side::Left, Vec2::new(-1.0, 0.0)),
            Vec2::Y
        );
        assert_eq!(
            upright_from_sideways(Side::Right, Vec2::new(1.0, 0.0)),
            Vec2::Y
        );
        for side in [Side::Left, Side::Right] {
            let s = Vec2::new(0.3, -0.7);
            let v = sideways_from_upright(side, s);
            assert!((upright_from_sideways(side, v) - s).length() < 1e-6);
        }
        // SDL (quer) und evdev landen bei derselben Taste.
        for (side, b, p) in [
            (Side::Left, G::South, Phys::Left),
            (Side::Left, G::East, Phys::Down),
            (Side::Right, G::South, Phys::A),
            (Side::Right, G::East, Phys::X),
        ] {
            assert_eq!(phys_from_sdl(side, b), Some(p));
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
