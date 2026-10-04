//! Eingabegeräte → Slots. Die Simulation kennt nur Slots, hier wird entschieden,
//! welche Taste welchen Slot drückt und wer zielt.
//!
//! * Tastatur + Maus zählt als ein Gerät, jedes Gamepad (auch jeder einzelne Joy-Con)
//!   als eigenes Gerät.
//! * Reservierte Tasten (Menü/Navigation) können nicht als Slot belegt werden.

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::game::{AppState, GameCamera, Sim};
use crate::sim::data::ShipDef;
use crate::sim::ship::Loadout;
use crate::sim::{MAX_SLOTS, TickInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Device {
    Keyboard,
    Pad(Entity),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Btn {
    Key(KeyCode),
    Mouse(MouseButton),
    Pad(Entity, GamepadButton),
}

impl Btn {
    pub fn device(&self) -> Device {
        match self {
            Btn::Key(_) | Btn::Mouse(_) => Device::Keyboard,
            Btn::Pad(e, _) => Device::Pad(*e),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Btn::Key(k) => key_label(*k),
            Btn::Mouse(MouseButton::Right) => "Maus R".into(),
            Btn::Mouse(MouseButton::Middle) => "Maus M".into(),
            Btn::Mouse(MouseButton::Back) => "Maus 4".into(),
            Btn::Mouse(MouseButton::Forward) => "Maus 5".into(),
            Btn::Mouse(b) => format!("{b:?}"),
            Btn::Pad(_, b) => pad_label(*b),
        }
    }
}

pub fn key_label(k: KeyCode) -> String {
    let s = format!("{k:?}");
    if let Some(rest) = s.strip_prefix("Key") {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix("Digit") {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix("Numpad") {
        return format!("Num{rest}");
    }
    match k {
        KeyCode::Space => "Leertaste".into(),
        KeyCode::ShiftLeft => "Shift L".into(),
        KeyCode::ShiftRight => "Shift R".into(),
        KeyCode::ControlLeft => "Strg L".into(),
        KeyCode::ControlRight => "Strg R".into(),
        KeyCode::AltLeft => "Alt".into(),
        KeyCode::AltRight => "AltGr".into(),
        _ => s,
    }
}

fn pad_label(b: GamepadButton) -> String {
    match b {
        GamepadButton::South => "A".into(),
        GamepadButton::East => "B".into(),
        GamepadButton::North => "Y".into(),
        GamepadButton::West => "X".into(),
        GamepadButton::LeftTrigger => "L".into(),
        GamepadButton::RightTrigger => "R".into(),
        GamepadButton::LeftTrigger2 => "ZL".into(),
        GamepadButton::RightTrigger2 => "ZR".into(),
        GamepadButton::LeftThumb => "L3".into(),
        GamepadButton::RightThumb => "R3".into(),
        other => format!("{other:?}"),
    }
}

/// Tasten, die nie als Slot belegt werden können (Navigation, Menüs).
pub fn reserved_key(k: KeyCode) -> bool {
    matches!(
        k,
        KeyCode::Escape
            | KeyCode::Enter
            | KeyCode::NumpadEnter
            | KeyCode::Tab
            | KeyCode::Backspace
            | KeyCode::ArrowUp
            | KeyCode::ArrowDown
            | KeyCode::ArrowLeft
            | KeyCode::ArrowRight
            | KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
            | KeyCode::CapsLock
            | PING_KEY
    )
}

/// Ping-Taste der Tastatur (auf deutschen Tastaturen „^“ links neben der 1).
pub const PING_KEY: KeyCode = KeyCode::Backquote;
/// Ping am Gamepad: Stick drücken (bei einzelnen Joy-Cons gibt es nur einen Stick).
pub const PING_PAD: [GamepadButton; 2] = [GamepadButton::LeftThumb, GamepadButton::RightThumb];

pub fn reserved_pad(b: GamepadButton) -> bool {
    matches!(
        b,
        GamepadButton::Start
            | GamepadButton::Select
            | GamepadButton::Mode
            | GamepadButton::DPadUp
            | GamepadButton::DPadDown
            | GamepadButton::DPadLeft
            | GamepadButton::DPadRight
            | GamepadButton::LeftThumb
            | GamepadButton::RightThumb
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimTarget {
    /// Index in der Belegungsreihenfolge der Triebwerke.
    Thruster(usize),
    /// Index in der Werkzeugliste des Schiffs.
    Tool(usize),
}

#[derive(Clone, Debug)]
pub struct Player {
    pub device: Device,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub btn: Btn,
    pub player: usize,
    pub target: ClaimTarget,
}

/// Die Crew vor dem Bildschirm: Spieler (Geräte) und ihre Slot-Tasten.
#[derive(Resource, Default, Clone, Debug)]
pub struct Crew {
    pub players: Vec<Player>,
    pub bindings: Vec<Binding>,
}

/// Fertig aufgelöste Zuordnung Taste → Simulations-Slot.
#[derive(Clone, Debug)]
pub struct ActiveBinding {
    pub btn: Btn,
    pub slot: u8,
    pub player: usize,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct ActiveBindings(pub Vec<ActiveBinding>);

impl Crew {
    pub fn player_for(
        &mut self,
        device: Device,
        gamepads: &Query<(Entity, &Gamepad, Option<&Name>)>,
    ) -> usize {
        if let Some(i) = self.players.iter().position(|p| p.device == device) {
            return i;
        }
        let label = match device {
            Device::Keyboard => "Tastatur".to_string(),
            Device::Pad(e) => gamepads
                .get(e)
                .ok()
                .and_then(|(_, _, n)| n.map(|n| n.as_str().to_string()))
                .unwrap_or_else(|| "Gamepad".into()),
        };
        self.players.push(Player { device, label });
        self.players.len() - 1
    }

    pub fn thruster_claims(&self) -> Vec<&Binding> {
        self.bindings
            .iter()
            .filter(|b| matches!(b.target, ClaimTarget::Thruster(_)))
            .collect()
    }

    pub fn loadout(&self) -> Loadout {
        let mut tools: Vec<usize> = self
            .bindings
            .iter()
            .filter_map(|b| match b.target {
                ClaimTarget::Tool(i) => Some(i),
                _ => None,
            })
            .collect();
        tools.sort_unstable();
        tools.dedup();
        Loadout {
            thrusters: self.thruster_claims().len() as u8,
            tools,
        }
    }

    /// Slot in der Simulation für einen belegten Claim.
    pub fn resolve(&self, def: &ShipDef) -> Vec<ActiveBinding> {
        let thruster_x: Vec<f32> = def.thruster_parts().map(|(_, p, _)| p.pos.0).collect();
        let mut claimed_thrusters: Vec<usize> = self
            .bindings
            .iter()
            .filter_map(|b| match b.target {
                ClaimTarget::Thruster(i) => Some(i),
                _ => None,
            })
            .collect();
        claimed_thrusters.sort_by(|a, b| thruster_x[*a].total_cmp(&thruster_x[*b]).then(a.cmp(b)));
        claimed_thrusters.dedup();
        let n = claimed_thrusters.len();
        let tools = self.loadout().tools;
        self.bindings
            .iter()
            .filter_map(|b| {
                let slot = match b.target {
                    ClaimTarget::Thruster(i) => claimed_thrusters.iter().position(|x| *x == i)?,
                    ClaimTarget::Tool(i) => n + tools.iter().position(|x| *x == i)?,
                };
                Some(ActiveBinding {
                    btn: b.btn,
                    slot: slot as u8,
                    player: b.player,
                })
            })
            .collect()
    }

    /// Spieler ohne Slots entfernen und Indizes neu vergeben.
    pub fn prune_players(&mut self) {
        let mut keep: Vec<usize> = Vec::new();
        for (i, _) in self.players.iter().enumerate() {
            if self.bindings.iter().any(|b| b.player == i) {
                keep.push(i);
            }
        }
        let players: Vec<Player> = keep.iter().map(|i| self.players[*i].clone()).collect();
        for b in &mut self.bindings {
            b.player = keep.iter().position(|k| *k == b.player).unwrap_or(0);
        }
        self.players = players;
    }
}

/// Zwischen zwei Simulationsschritten gedrückte Tasten, damit kurze Tipper nicht verloren gehen.
#[derive(Resource, Default)]
pub struct InputLatch(pub u32);

/// Zielwinkel pro Slot (für Werkzeuge), wird jedes Frame aktualisiert.
#[derive(Resource)]
pub struct Aims(pub Vec<f32>);

impl Default for Aims {
    fn default() -> Self {
        Aims(vec![std::f32::consts::FRAC_PI_2; MAX_SLOTS])
    }
}

/// Mausposition in der Spielebene.
#[derive(Resource, Default)]
pub struct CursorWorld(pub Option<Vec2>);

/// Menü-Eingabe dieses Frames (Tastatur und alle Gamepads zusammen).
#[derive(Resource, Default, Debug, Clone)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    /// Nur Enter auf der Tastatur.
    pub enter: bool,
    pub back: bool,
    /// Gerät, das zuletzt bestätigt hat (für die Abstimmung: wer hat den Kauf angestoßen).
    pub device: Option<Device>,
    pub escape: bool,
    pub tab: bool,
    pub start: bool,
    pub select: bool,
    pub backspace: bool,
}

pub fn btn_pressed(
    btn: &Btn,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    pads: &Query<(Entity, &Gamepad, Option<&Name>)>,
) -> bool {
    match btn {
        Btn::Key(k) => keys.pressed(*k),
        Btn::Mouse(m) => mouse.pressed(*m),
        Btn::Pad(e, b) => pads.get(*e).map(|(_, g, _)| g.pressed(*b)).unwrap_or(false),
    }
}

pub fn btn_just_pressed(
    btn: &Btn,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    pads: &Query<(Entity, &Gamepad, Option<&Name>)>,
) -> bool {
    match btn {
        Btn::Key(k) => keys.just_pressed(*k),
        Btn::Mouse(m) => mouse.just_pressed(*m),
        Btn::Pad(e, b) => pads
            .get(*e)
            .map(|(_, g, _)| g.just_pressed(*b))
            .unwrap_or(false),
    }
}

/// Alle frisch gedrückten, belegbaren Tasten dieses Frames.
pub fn fresh_claimable(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    pads: &Query<(Entity, &Gamepad, Option<&Name>)>,
) -> Vec<Btn> {
    let mut out: Vec<Btn> = keys
        .get_just_pressed()
        .filter(|k| !reserved_key(**k))
        .map(|k| Btn::Key(*k))
        .collect();
    for m in mouse.get_just_pressed() {
        if *m != MouseButton::Left {
            out.push(Btn::Mouse(*m));
        }
    }
    let mut pad_list: Vec<_> = pads.iter().collect();
    pad_list.sort_by_key(|(e, _, _)| *e);
    for (e, g, _) in pad_list {
        for b in g.get_just_pressed() {
            if !reserved_pad(*b) {
                out.push(Btn::Pad(e, *b));
            }
        }
    }
    out
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Crew>()
            .init_resource::<ActiveBindings>()
            .init_resource::<InputLatch>()
            .init_resource::<Aims>()
            .init_resource::<CursorWorld>()
            .init_resource::<MenuInput>()
            .add_systems(
                PreUpdate,
                (read_menu_input, latch_slots).after(bevy::input::InputSystems),
            )
            .add_systems(
                Update,
                (update_aims, send_pings)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// Ping: Tastatur markiert die Mausposition, Gamepads die Richtung des Sticks (oder voraus).
/// Nur Crewmitglieder pingen; der Ping läuft als Befehl durch die Simulation.
#[allow(clippy::too_many_arguments)]
fn send_pings(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    crew: Res<Crew>,
    sim: Res<Sim>,
    cursor: Res<CursorWorld>,
    paused: Res<crate::game::Paused>,
    map: Res<crate::game::MapOpen>,
    mut pending: ResMut<crate::game::PendingCommands>,
) {
    if paused.0 || map.0 {
        return;
    }
    let ship = &sim.0.ship;
    let ahead = ship.pos + crate::sim::geom::rot(Vec2::Y, ship.angle) * 30.0;
    let player_of = |d: Device| crew.players.iter().position(|p| p.device == d);
    if keys.just_pressed(PING_KEY)
        && let Some(p) = player_of(Device::Keyboard)
    {
        pending.0.push(crate::sim::Command::Ping {
            player: p as u8,
            pos: cursor.0.unwrap_or(ahead),
        });
    }
    for (e, g, _) in &pads {
        if !PING_PAD.iter().any(|b| g.just_pressed(*b)) {
            continue;
        }
        let Some(p) = player_of(Device::Pad(e)) else {
            continue;
        };
        let (l, r) = (g.left_stick(), g.right_stick());
        let stick = if r.length() > l.length() { r } else { l };
        let pos = if stick.length() > 0.35 {
            ship.pos + stick.normalize() * 45.0
        } else {
            ahead
        };
        pending.0.push(crate::sim::Command::Ping {
            player: p as u8,
            pos,
        });
    }
}

pub fn read_menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    bindings: Res<ActiveBindings>,
    mut menu: ResMut<MenuInput>,
) {
    let bound = |e: Entity, b: GamepadButton| bindings.0.iter().any(|x| x.btn == Btn::Pad(e, b));
    let mut m = MenuInput {
        up: keys.just_pressed(KeyCode::ArrowUp),
        down: keys.just_pressed(KeyCode::ArrowDown),
        left: keys.just_pressed(KeyCode::ArrowLeft),
        right: keys.just_pressed(KeyCode::ArrowRight),
        confirm: keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter),
        enter: keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter),
        back: false,
        device: None,
        escape: keys.just_pressed(KeyCode::Escape),
        tab: keys.just_pressed(KeyCode::Tab),
        start: false,
        select: false,
        backspace: keys.just_pressed(KeyCode::Backspace),
    };
    if m.confirm {
        m.device = Some(Device::Keyboard);
    }
    for (e, g, _) in &pads {
        if g.just_pressed(GamepadButton::Start)
            || (g.just_pressed(GamepadButton::South) && !bound(e, GamepadButton::South))
        {
            m.device = Some(Device::Pad(e));
        }
        m.up |= g.just_pressed(GamepadButton::DPadUp);
        m.down |= g.just_pressed(GamepadButton::DPadDown);
        m.left |= g.just_pressed(GamepadButton::DPadLeft);
        m.right |= g.just_pressed(GamepadButton::DPadRight);
        m.start |= g.just_pressed(GamepadButton::Start);
        m.select |= g.just_pressed(GamepadButton::Select);
        // Gesichtstasten nur, wenn sie nicht als Slot belegt sind.
        if g.just_pressed(GamepadButton::South) && !bound(e, GamepadButton::South) {
            m.confirm = true;
        }
        if g.just_pressed(GamepadButton::East) && !bound(e, GamepadButton::East) {
            m.back = true;
        }
    }
    *menu = m;
}

fn latch_slots(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    bindings: Res<ActiveBindings>,
    mut latch: ResMut<InputLatch>,
) {
    for b in &bindings.0 {
        if btn_just_pressed(&b.btn, &keys, &mouse, &pads) {
            latch.0 |= 1 << b.slot;
        }
    }
}

/// Eingabe für einen Simulationsschritt zusammenbauen.
pub fn build_tick_input(
    bindings: &ActiveBindings,
    latch: &mut InputLatch,
    aims: &Aims,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    pads: &Query<(Entity, &Gamepad, Option<&Name>)>,
) -> TickInput {
    let mut slots = latch.0;
    latch.0 = 0;
    for b in &bindings.0 {
        if btn_pressed(&b.btn, keys, mouse, pads) {
            slots |= 1 << b.slot;
        }
    }
    // Bremsassistent: ↓ auf der Tastatur oder Steuerkreuz ↓ an irgendeinem Gamepad.
    let brake = keys.pressed(KeyCode::ArrowDown)
        || pads
            .iter()
            .any(|(_, g, _)| g.pressed(GamepadButton::DPadDown));
    TickInput {
        slots,
        aims: aims.0.clone(),
        commands: Vec::new(),
        brake,
    }
}

fn update_aims(
    sim: Res<Sim>,
    bindings: Res<ActiveBindings>,
    crew: Res<Crew>,
    pads: Query<(Entity, &Gamepad, Option<&Name>)>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    mut cursor_world: ResMut<CursorWorld>,
    mut aims: ResMut<Aims>,
) {
    cursor_world.0 = None;
    if let (Ok(win), Ok((cam, cam_t))) = (window.single(), camera.single())
        && let Some(cursor) = win.cursor_position()
        && let Ok(ray) = cam.viewport_to_world(cam_t, cursor)
        && let Some(t) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Z))
    {
        cursor_world.0 = Some(ray.get_point(t).truncate());
    }
    let ship = &sim.0.ship;
    let _ = &crew;
    for tool_i in 0..ship.tools.len() {
        let slot = ship.tools[tool_i].slot as usize;
        // Zielen darf nur das Gerät, dem der Werkzeug-Slot gehört.
        let Some(device) = aim_device(&bindings.0, slot as u8) else {
            continue;
        };
        let mount = ship.tool_world_pos(tool_i);
        let aim = match device {
            Device::Keyboard => cursor_world.0.map(|c| (c - mount).to_angle()),
            Device::Pad(e) => pads.get(e).ok().and_then(|(_, g, _)| {
                let r = g.right_stick();
                let l = g.left_stick();
                let s = if r.length() > 0.35 { r } else { l };
                (s.length() > 0.35).then(|| s.to_angle())
            }),
        };
        if let Some(a) = aim
            && slot < aims.0.len()
        {
            aims.0[slot] = a;
        }
    }
}

/// Welches Gerät zielt für diesen Slot? Nur das Gerät, dessen Taste den Slot belegt.
/// Ist ein Slot mehrfach belegt (sollte nicht vorkommen), gewinnt die erste Belegung.
pub fn aim_device(bindings: &[ActiveBinding], slot: u8) -> Option<Device> {
    bindings
        .iter()
        .find(|b| b.slot == slot)
        .map(|b| b.btn.device())
}

/// Erkennungsfarben der Spieler (unabhängig von den Slotfarben).
pub const PLAYER_COLORS: [[f32; 3]; 8] = [
    [0.92, 0.94, 0.98],
    [1.0, 0.72, 0.16],
    [0.3, 0.75, 1.0],
    [1.0, 0.42, 0.68],
    [0.62, 0.92, 0.42],
    [0.72, 0.58, 1.0],
    [1.0, 0.5, 0.3],
    [0.35, 0.95, 0.85],
];

pub fn player_color(i: usize) -> Color {
    let c = PLAYER_COLORS[i % PLAYER_COLORS.len()];
    Color::srgb(c[0], c[1], c[2])
}

/// Kurzes Geräte-Kürzel für Abzeichen: ⌨ Tastatur, JC-L/JC-R für Joy-Cons, ◉ sonst.
pub fn device_badge(device: Device, label: &str) -> String {
    match device {
        Device::Keyboard => "⌨".into(),
        Device::Pad(_) => {
            let l = label.to_lowercase();
            if l.contains("joy-con") || l.contains("joycon") {
                if l.contains("(l)") || l.ends_with(" l") {
                    "JC-L".into()
                } else if l.contains("(r)") || l.ends_with(" r") {
                    "JC-R".into()
                } else {
                    "JC".into()
                }
            } else {
                "◉".into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(n: u32) -> Entity {
        Entity::from_raw_u32(n).unwrap()
    }

    #[test]
    fn only_the_owner_aims_a_tool_slot() {
        let p1 = pad(7);
        let p2 = pad(9);
        let bindings = vec![
            ActiveBinding {
                btn: Btn::Key(KeyCode::KeyA),
                slot: 0,
                player: 0,
            },
            ActiveBinding {
                btn: Btn::Pad(p1, GamepadButton::South),
                slot: 1,
                player: 1,
            },
            ActiveBinding {
                btn: Btn::Pad(p2, GamepadButton::East),
                slot: 2,
                player: 2,
            },
            ActiveBinding {
                btn: Btn::Key(KeyCode::Space),
                slot: 3,
                player: 0,
            },
        ];
        assert_eq!(aim_device(&bindings, 1), Some(Device::Pad(p1)));
        assert_eq!(aim_device(&bindings, 2), Some(Device::Pad(p2)));
        assert_eq!(aim_device(&bindings, 3), Some(Device::Keyboard));
        // Unbelegte Slots zielt niemand.
        assert_eq!(aim_device(&bindings, 4), None);
    }

    #[test]
    fn badges() {
        assert_eq!(device_badge(Device::Keyboard, "Tastatur"), "⌨");
        assert_eq!(device_badge(Device::Pad(pad(3)), "Joy-Con (L)"), "JC-L");
        assert_eq!(device_badge(Device::Pad(pad(3)), "Joy-Con (R)"), "JC-R");
        assert_eq!(device_badge(Device::Pad(pad(3)), "Xbox Controller"), "◉");
    }
}

#[cfg(test)]
mod ping_tests {
    use super::*;

    #[test]
    fn ping_buttons_are_never_slots() {
        assert!(reserved_key(PING_KEY));
        for b in PING_PAD {
            assert!(reserved_pad(b));
        }
    }
}
