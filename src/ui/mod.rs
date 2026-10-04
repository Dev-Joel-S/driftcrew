//! Benutzeroberfläche: gemeinsame Bausteine, Menülisten, Meldungen.

pub mod callout;
pub mod cargo;
pub mod course;
pub mod hud;
pub mod lobby;
pub mod logbook;
pub mod map;
pub mod minigame;
pub mod pause;
pub mod radio;
pub mod replay;
pub mod report;
pub mod station;
pub mod text_entry;
pub mod title;

use bevy::prelude::*;

use crate::game::SimMsg;
use crate::input::MenuInput;
use crate::sim::{SimEvent, ToastKind};

pub const BG: Color = Color::srgba(0.025, 0.04, 0.085, 0.94);
pub const BG_ITEM: Color = Color::srgba(1.0, 1.0, 1.0, 0.035);
pub const BG_FOCUS: Color = Color::srgba(0.12, 0.82, 0.76, 0.22);
pub const BORDER: Color = Color::srgba(0.12, 0.82, 0.76, 0.55);
pub const ACCENT: Color = Color::srgb(1.0, 0.7, 0.12);
pub const TEXT: Color = Color::srgb(0.93, 0.95, 1.0);
pub const MUTED: Color = Color::srgb(0.55, 0.6, 0.72);
pub const GOOD: Color = Color::srgb(0.35, 1.0, 0.6);
pub const WARN: Color = Color::srgb(1.0, 0.75, 0.2);
pub const BAD: Color = Color::srgb(1.0, 0.3, 0.3);
pub const TEAL: Color = Color::srgb(0.12, 0.85, 0.78);

pub const MENU_TITLE: usize = 0;
pub const MENU_PAUSE: usize = 1;
pub const MENU_STATION: usize = 2;
pub const MENU_MAP: usize = 3;

pub struct UiPlugin;

const FONT_REGULAR: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans-Bold.ttf");

static BOLD: std::sync::OnceLock<Handle<Font>> = std::sync::OnceLock::new();

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        // DejaVu Sans als Standardschrift: deckt Umlaute und Symbole ab.
        {
            let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
            let _ = fonts.insert(AssetId::default(), Font::from_bytes(FONT_REGULAR.to_vec()));
            let bold = fonts.add(Font::from_bytes(FONT_BOLD.to_vec()));
            let _ = BOLD.set(bold);
        }
        app.init_resource::<MenuFocus>()
            .init_resource::<MenuScroll>()
            .init_resource::<Toasts>()
            .add_systems(
                Update,
                (
                    collect_toasts,
                    draw_toasts,
                    highlight_items,
                    scroll_focus_into_view,
                ),
            )
            .add_plugins((
                title::TitlePlugin,
                lobby::LobbyPlugin,
                hud::HudPlugin,
                station::StationPlugin,
                map::MapPlugin,
                pause::PausePlugin,
                report::ReportPlugin,
                radio::RadioPlugin,
                course::CoursePlugin,
                minigame::MinigamePlugin,
                logbook::LogbookPlugin,
                cargo::CargoPlanPlugin,
                replay::ReplayUiPlugin,
                text_entry::TextEntryPlugin,
                callout::CalloutPlugin,
            ));
    }
}

pub fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    let mut font = TextFont::from_font_size(size);
    if size >= 19.0
        && let Some(h) = BOLD.get()
    {
        font.font = bevy::text::FontSource::Handle(h.clone());
    }
    (Text::new(s), font, TextColor(color))
}

pub fn panel_node(width: Val) -> Node {
    Node {
        width,
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(Val::Px(16.0)),
        row_gap: Val::Px(6.0),
        border: UiRect::all(Val::Px(1.5)),
        border_radius: BorderRadius::all(Val::Px(12.0)),
        ..default()
    }
}

pub fn panel(width: Val) -> impl Bundle {
    (
        panel_node(width),
        BackgroundColor(BG),
        BorderColor::all(BORDER),
    )
}

pub fn chip(color: Color, size: f32) -> impl Bundle {
    (
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            border_radius: BorderRadius::all(Val::Px(size * 0.3)),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(color),
    )
}

/// Ein Eintrag in einer Menüliste.
#[derive(Clone, Debug)]
pub struct Item<A: Clone> {
    pub label: String,
    pub right: String,
    pub detail: String,
    pub enabled: bool,
    pub action: Option<A>,
    /// Farbfeld vor dem Text (z. B. Lackiererei).
    pub swatch: Option<Color>,
}

impl<A: Clone> Item<A> {
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Item {
            label: label.into(),
            right: String::new(),
            detail: String::new(),
            enabled: true,
            action: Some(action),
            swatch: None,
        }
    }
    pub fn swatch(mut self, c: Color) -> Self {
        self.swatch = Some(c);
        self
    }
    pub fn right(mut self, r: impl Into<String>) -> Self {
        self.right = r.into();
        self
    }
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = d.into();
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
}

#[derive(Component)]
pub struct ItemButton {
    pub menu: usize,
    pub index: usize,
}

/// Liste mit Scrollbalken: hält den per Tastatur/Gamepad gewählten Eintrag im Bild.
#[derive(Component)]
pub struct ScrollList(pub usize);

/// Letzte Scrollposition pro Menü – damit ein Neuaufbau der Liste nicht nach oben springt.
#[derive(Resource, Default)]
pub struct MenuScroll(pub [f32; 8]);

fn scroll_focus_into_view(
    focus: Res<MenuFocus>,
    mut saved: ResMut<MenuScroll>,
    mut lists: Query<(
        &ScrollList,
        &Node,
        &ComputedNode,
        &mut ScrollPosition,
        &Children,
    )>,
    items: Query<(&ItemButton, &ComputedNode)>,
) {
    for (list, node, computed, mut scroll, children) in &mut lists {
        let k = computed.inverse_scale_factor();
        let view_h = computed.size().y * k;
        let gap = match node.row_gap {
            Val::Px(g) => g,
            _ => 0.0,
        };
        let want = focus.0.get(list.0).copied().unwrap_or(0);
        // Lage im Inhalt aus den Höhen der Einträge davor (die Positionen auf dem Bildschirm
        // hinken dem Scrollwert einen Frame hinterher).
        let mut top = 0.0;
        for c in children.iter() {
            let Ok((ib, cn)) = items.get(c) else {
                continue;
            };
            if ib.menu != list.0 {
                continue;
            }
            let h = cn.size().y * k;
            if ib.index == want {
                if h <= 0.0 || view_h <= 0.0 {
                    break;
                }
                if top < scroll.y {
                    scroll.y = top;
                } else if top + h > scroll.y + view_h {
                    scroll.y = top + h - view_h;
                }
                if let Some(v) = saved.0.get_mut(list.0) {
                    *v = scroll.y;
                }
                break;
            }
            top += h + gap;
        }
    }
}

#[derive(Resource, Default)]
pub struct MenuFocus(pub [usize; 8]);

pub fn spawn_items<A: Clone>(parent: &mut ChildSpawnerCommands, menu: usize, items: &[Item<A>]) {
    for (i, it) in items.iter().enumerate() {
        let label_color = if it.enabled { TEXT } else { MUTED };
        parent
            .spawn((
                Button,
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(BG_ITEM),
                BorderColor::all(Color::NONE),
                ItemButton { menu, index: i },
            ))
            .with_children(|b| {
                b.spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(16.0),
                    ..default()
                })
                .with_children(|row| {
                    if let Some(c) = it.swatch {
                        row.spawn((
                            Node {
                                width: Val::Px(18.0),
                                height: Val::Px(18.0),
                                flex_shrink: 0.0,
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(c),
                            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)),
                        ));
                    }
                    row.spawn((
                        text(it.label.clone(), 17.0, label_color),
                        Node {
                            flex_grow: 1.0,
                            flex_shrink: 1.0,
                            flex_basis: Val::Px(0.0),
                            ..default()
                        },
                    ));
                    if !it.right.is_empty() {
                        // Preise/Belohnungen nie umbrechen.
                        row.spawn((
                            text(
                                it.right.clone(),
                                17.0,
                                if it.enabled { ACCENT } else { MUTED },
                            ),
                            Node {
                                flex_shrink: 0.0,
                                ..default()
                            },
                            TextLayout::no_wrap(),
                        ));
                    }
                });
                if !it.detail.is_empty() {
                    b.spawn(text(it.detail.clone(), 13.0, MUTED));
                }
            });
    }
}

/// Pfeiltasten/Steuerkreuz bewegen den Fokus. Liefert true, wenn bestätigt wurde.
pub fn navigate(focus: &mut usize, len: usize, input: &MenuInput) -> bool {
    if len == 0 {
        *focus = 0;
        return false;
    }
    if input.up {
        *focus = (*focus + len - 1) % len;
    }
    if input.down {
        *focus = (*focus + 1) % len;
    }
    if *focus >= len {
        *focus = len - 1;
    }
    input.confirm || input.start
}

/// Maus: Hover setzt den Fokus, Klick löst aus.
pub fn mouse_pick(
    menu: usize,
    focus: &mut usize,
    q: &Query<(&Interaction, &ItemButton), Changed<Interaction>>,
) -> Option<usize> {
    let mut clicked = None;
    for (i, b) in q.iter() {
        if b.menu != menu {
            continue;
        }
        match i {
            Interaction::Hovered => *focus = b.index,
            Interaction::Pressed => {
                *focus = b.index;
                clicked = Some(b.index);
            }
            Interaction::None => {}
        }
    }
    clicked
}

fn highlight_items(
    focus: Res<MenuFocus>,
    mut q: Query<(&ItemButton, &mut BackgroundColor, &mut BorderColor)>,
) {
    for (b, mut bg, mut border) in &mut q {
        let f = focus.0.get(b.menu).copied().unwrap_or(0) == b.index;
        bg.0 = if f { BG_FOCUS } else { BG_ITEM };
        *border = BorderColor::all(if f { BORDER } else { Color::NONE });
    }
}

/// Inhalt nur neu aufbauen, wenn er sich geändert hat.
#[derive(Component, Default)]
pub struct Signature(pub u64);

pub fn sig_of(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

// ---------------------------------------------------------------------------
// Meldungen
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct Toasts {
    pub list: Vec<(String, ToastKind, f32)>,
    pub dirty: bool,
}

impl Toasts {
    pub fn push(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.list.push((text.into(), kind, 0.0));
        if self.list.len() > 5 {
            self.list.remove(0);
        }
        self.dirty = true;
    }
}

#[derive(Component)]
struct ToastRoot;

fn collect_toasts(mut events: MessageReader<SimMsg>, mut toasts: ResMut<Toasts>, time: Res<Time>) {
    for SimMsg(e) in events.read() {
        if let SimEvent::Toast { text, kind } = e {
            toasts.push(text.clone(), kind.clone());
        }
    }
    let dt = time.delta_secs();
    let before = toasts.list.len();
    for t in &mut toasts.list {
        t.2 += dt;
    }
    toasts.list.retain(|t| t.2 < 5.5);
    if toasts.list.len() != before {
        toasts.dirty = true;
    }
}

fn draw_toasts(
    mut commands: Commands,
    mut toasts: ResMut<Toasts>,
    sim: Option<Res<crate::game::Sim>>,
    mut root: Query<(Entity, &mut Node), With<ToastRoot>>,
) {
    // Läuft ein Parcours, steht oben dessen Anzeige: Meldungen rücken darunter.
    let top = if sim.is_some_and(|s| s.0.course.is_some()) {
        Val::Px(132.0)
    } else {
        Val::Px(84.0)
    };
    let root = match root.single_mut() {
        Ok((r, mut n)) => {
            if n.top != top {
                n.top = top;
            }
            r
        }
        Err(_) => commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(84.0),
                    left: Val::Percent(50.0),
                    margin: UiRect::left(Val::Px(-260.0)),
                    width: Val::Px(520.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                GlobalZIndex(50),
                ToastRoot,
                Pickable::IGNORE,
            ))
            .id(),
    };
    if !toasts.dirty {
        return;
    }
    toasts.dirty = false;
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for (msg, kind, _) in &toasts.list {
            let c = match kind {
                ToastKind::Info => TEAL,
                ToastKind::Good => GOOD,
                ToastKind::Warn => WARN,
                ToastKind::Bad => BAD,
            };
            p.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    border: UiRect::left(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(BG),
                BorderColor::all(c),
                Pickable::IGNORE,
            ))
            .with_children(|t| {
                t.spawn((text(msg.clone(), 16.0, TEXT), Pickable::IGNORE));
            });
        }
    });
}

/// Zahlen mit Tausenderpunkt.
pub fn fmt_num(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}
