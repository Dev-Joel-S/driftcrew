//! Fixed-Point-Zahlen für die Simulation: **keine Floats** in Zustand und Logik.
//!
//! * [`Fx`] ist eine Ganzzahl mit 32 Nachkommabits (Q32.32 in `i64`, Zwischenwerte in `i128`).
//!   Alle Rechenarten sättigen statt zu überlaufen – dadurch gibt es überall dasselbe Ergebnis,
//!   in Debug wie in Release, auf jeder Plattform.
//! * Wurzel, Sinus, Kosinus und Arkustangens kommen aus reiner Ganzzahlrechnung (Digit-by-digit
//!   bzw. CORDIC), nicht aus der Rechenbibliothek des Systems.
//! * Zahlen im Quelltext schreibt man als `fx!(0.6)`: Der Compiler wertet das zur Übersetzungszeit
//!   aus (`const`-Block), im fertigen Programm steht nur noch die Ganzzahl.
//! * Float-Typen gibt es hier nur an den Rändern: Einlesen von Datendateien und Spielständen,
//!   Eingabegeräte (`from_f32`) und die Anzeige (`to_f32`). Der Test `no_floats_in_sim`
//!   (`sim/float_guard.rs`) prüft den Rest der Simulation.
#![allow(clippy::float_arithmetic, clippy::cast_precision_loss)]

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, Sub, SubAssign};

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const FRAC: u32 = 32;
const ONE_RAW: i64 = 1 << FRAC;

/// Zahl im Quelltext, zur Übersetzungszeit in Fixed-Point umgerechnet.
#[macro_export]
macro_rules! fx {
    ($e:expr) => {
        const { $crate::sim::fx::Fx::lit($e as f64) }
    };
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fx(pub i64);

impl Fx {
    pub const ZERO: Fx = Fx(0);
    pub const ONE: Fx = Fx(ONE_RAW);
    pub const HALF: Fx = Fx(ONE_RAW / 2);
    pub const MAX: Fx = Fx(i64::MAX);
    pub const MIN: Fx = Fx(i64::MIN + 1);
    /// Kleinster Schritt (2⁻³²).
    pub const EPSILON: Fx = Fx(1);
    pub const PI: Fx = Fx(13_493_037_705);
    pub const TAU: Fx = Fx(26_986_075_409);
    pub const HALF_PI: Fx = Fx(6_746_518_852);

    /// Nur für `fx!` (und Konstanten): Dezimalzahl zur Übersetzungszeit umrechnen.
    pub const fn lit(x: f64) -> Fx {
        let v = x * 4_294_967_296.0;
        Fx(if v >= 0.0 {
            (v + 0.5) as i64
        } else {
            (v - 0.5) as i64
        })
    }

    pub const fn int(n: i64) -> Fx {
        Fx(n.saturating_mul(ONE_RAW))
    }

    /// `n / d` zur Übersetzungszeit, nur mit Ganzzahlen (gerundet).
    pub const fn frac(n: i64, d: i64) -> Fx {
        Fx(((n as i128 * ONE_RAW as i128 * 2 + d as i128) / (d as i128 * 2)) as i64)
    }

    pub const fn from_raw(raw: i64) -> Fx {
        Fx(raw)
    }

    pub const fn raw(self) -> i64 {
        self.0
    }

    /// `n / d` als Fixed-Point (d ≠ 0).
    pub fn ratio(n: i64, d: i64) -> Fx {
        Fx::int(n) / Fx::int(d)
    }

    // --- Ränder: Float nur hier -------------------------------------------------------------

    pub fn from_f32(x: f32) -> Fx {
        Fx::from_f64(x as f64)
    }

    pub fn from_f64(x: f64) -> Fx {
        if x.is_nan() {
            return Fx::ZERO;
        }
        let v = (x * 4_294_967_296.0).round();
        if v >= i64::MAX as f64 {
            Fx::MAX
        } else if v <= i64::MIN as f64 {
            Fx::MIN
        } else {
            Fx(v as i64)
        }
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 4_294_967_296.0
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 4_294_967_296.0
    }

    // --- Ganzzahlen -------------------------------------------------------------------------

    /// Ganzzahlanteil, zur Null hin (wie `as i64` bei Floats).
    pub const fn to_int(self) -> i64 {
        self.0 / ONE_RAW
    }

    pub const fn floor_int(self) -> i64 {
        self.0 >> FRAC
    }

    pub fn round_int(self) -> i64 {
        self.round().floor_int()
    }

    pub fn ceil_int(self) -> i64 {
        self.ceil().floor_int()
    }

    pub fn to_u32(self) -> u32 {
        self.to_int().clamp(0, u32::MAX as i64) as u32
    }

    pub fn to_u8(self) -> u8 {
        self.to_int().clamp(0, 255) as u8
    }

    pub fn to_i32(self) -> i32 {
        self.to_int().clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }

    pub fn to_usize(self) -> usize {
        self.to_int().max(0) as usize
    }

    // --- Funktionen -------------------------------------------------------------------------

    pub fn abs(self) -> Fx {
        Fx(self.0.saturating_abs())
    }

    /// Vorzeichen als ±1 (null zählt als +1, wie bei `f32::signum`).
    pub fn signum(self) -> Fx {
        if self.0 < 0 { Fx(-ONE_RAW) } else { Fx::ONE }
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn floor(self) -> Fx {
        Fx(self.0 & !(ONE_RAW - 1))
    }

    pub fn ceil(self) -> Fx {
        Fx(self.0.saturating_add(ONE_RAW - 1) & !(ONE_RAW - 1))
    }

    pub fn round(self) -> Fx {
        // Halbe weg von null, wie f32::round.
        if self.0 >= 0 {
            Fx(self.0.saturating_add(ONE_RAW / 2) & !(ONE_RAW - 1))
        } else {
            Fx(-(self.0.saturating_neg().saturating_add(ONE_RAW / 2) & !(ONE_RAW - 1)))
        }
    }

    pub fn fract(self) -> Fx {
        self - self.floor()
    }

    pub fn rem_euclid(self, m: Fx) -> Fx {
        if m.0 == 0 {
            return Fx::ZERO;
        }
        Fx(self.0.rem_euclid(m.0.abs()))
    }

    pub fn lerp(self, to: Fx, t: Fx) -> Fx {
        self + (to - self) * t
    }

    pub fn powi(self, n: i32) -> Fx {
        let mut r = Fx::ONE;
        for _ in 0..n.unsigned_abs() {
            r *= self;
        }
        if n < 0 { Fx::ONE / r } else { r }
    }

    pub fn sqrt(self) -> Fx {
        if self.0 <= 0 {
            return Fx::ZERO;
        }
        // (raw · 2³²) hat Q64-Skalierung; die Wurzel daraus ist wieder Q32.
        Fx(isqrt((self.0 as u128) << FRAC) as i64)
    }

    pub fn to_radians(self) -> Fx {
        self * fx!(0.017453292519943295)
    }

    pub fn to_degrees(self) -> Fx {
        self * fx!(57.29577951308232)
    }

    /// Sinus und Kosinus (Winkel im Bogenmaß), mit CORDIC aus Ganzzahlen.
    pub fn sin_cos(self) -> (Fx, Fx) {
        // Auf (-π, π] bringen.
        let mut a = self.rem_euclid(Fx::TAU);
        if a > Fx::PI {
            a -= Fx::TAU;
        }
        // Auf [-π/2, π/2] spiegeln; der Kosinus kehrt dabei das Vorzeichen um.
        let mut flip = false;
        if a > Fx::HALF_PI {
            a = Fx::PI - a;
            flip = true;
        } else if a < -Fx::HALF_PI {
            a = -Fx::PI - a;
            flip = true;
        }
        let (mut x, mut y, mut z) = (CORDIC_K, 0i64, a.0 << 28);
        for (i, t) in CORDIC_ATAN.iter().enumerate() {
            let (dx, dy) = (y >> i, x >> i);
            if z >= 0 {
                x -= dx;
                y += dy;
                z -= t;
            } else {
                x += dx;
                y -= dy;
                z += t;
            }
        }
        // Q60 → Q32 mit Rundung.
        let to_fx = |q: i64| Fx((q + (1 << 27)) >> 28);
        let (s, c) = (to_fx(y), to_fx(x));
        (s, if flip { -c } else { c })
    }

    pub fn sin(self) -> Fx {
        self.sin_cos().0
    }

    pub fn cos(self) -> Fx {
        self.sin_cos().1
    }

    /// Winkel von (x, y) zur x-Achse in (-π, π]: `y.atan2(x)` wie bei Floats.
    pub fn atan2(self, x: Fx) -> Fx {
        let y = self;
        if x.0 == 0 && y.0 == 0 {
            return Fx::ZERO;
        }
        let (ax, ay) = (x.0.unsigned_abs(), y.0.unsigned_abs());
        let m = ax.max(ay);
        // So weit aufblasen, dass die Genauigkeit der Ganzzahlen voll genutzt wird.
        let shift = (m.leading_zeros() as i32 - 2).max(0) as u32;
        let (mut cx, mut cy) = ((ax << shift) as i64, (ay << shift) as i64);
        let mut z = 0i64;
        for (i, t) in CORDIC_ATAN.iter().enumerate() {
            let (dx, dy) = (cy >> i, cx >> i);
            if cy > 0 {
                cx += dx;
                cy -= dy;
                z += t;
            } else {
                cx -= dx;
                cy += dy;
                z -= t;
            }
        }
        let mut a = Fx((z + (1 << 27)) >> 28);
        if x.0 < 0 {
            a = Fx::PI - a;
        }
        if y.0 < 0 {
            a = -a;
        }
        a
    }

    /// Kleinster Winkelabstand von `self` zu `to` in (-π, π].
    pub fn angle_to(self, to: Fx) -> Fx {
        let mut d = (to - self).rem_euclid(Fx::TAU);
        if d > Fx::PI {
            d -= Fx::TAU;
        }
        d
    }

    /// Wie `f32::total_cmp`: für `sort_by` und `min_by`.
    pub fn total_cmp(&self, o: &Fx) -> std::cmp::Ordering {
        self.0.cmp(&o.0)
    }

    pub fn to_u64(self) -> u64 {
        self.to_int().max(0) as u64
    }

    pub fn to_i64(self) -> i64 {
        self.to_int()
    }

    pub fn to_u16(self) -> u16 {
        self.to_int().clamp(0, u16::MAX as i64) as u16
    }

    pub fn is_finite(self) -> bool {
        true
    }
}

/// Ganzzahlwurzel (abgerundet) – reine Ganzzahlrechnung der Standardbibliothek.
fn isqrt(n: u128) -> u128 {
    n.isqrt()
}

fn sat(v: i128) -> Fx {
    if v > i64::MAX as i128 {
        Fx::MAX
    } else if v < (i64::MIN + 1) as i128 {
        Fx::MIN
    } else {
        Fx(v as i64)
    }
}

/// 1 / ∏ √(1 + 2⁻²ⁱ) in Q60.
const CORDIC_K: i64 = 700114967507363238;

/// arctan(2⁻ⁱ) in Q60.
const CORDIC_ATAN: [i64; 40] = [
    905502432259640355,
    534549298976576474,
    282441168888798124,
    143371547418228444,
    71963988336308046,
    36017075762092179,
    18012932708689205,
    9007016009513623,
    4503576721087964,
    2251796950380271,
    1125899548928887,
    562949908682076,
    281474971118251,
    140737487656277,
    70368744090283,
    35184372077909,
    17592186043051,
    8796093022037,
    4398046511083,
    2199023255549,
    1099511627776,
    549755813888,
    274877906944,
    137438953472,
    68719476736,
    34359738368,
    17179869184,
    8589934592,
    4294967296,
    2147483648,
    1073741824,
    536870912,
    268435456,
    134217728,
    67108864,
    33554432,
    16777216,
    8388608,
    4194304,
    2097152,
];

impl Add for Fx {
    type Output = Fx;
    fn add(self, o: Fx) -> Fx {
        Fx(self.0.saturating_add(o.0))
    }
}

impl Sub for Fx {
    type Output = Fx;
    fn sub(self, o: Fx) -> Fx {
        Fx(self.0.saturating_sub(o.0))
    }
}

impl Mul for Fx {
    type Output = Fx;
    fn mul(self, o: Fx) -> Fx {
        sat((self.0 as i128 * o.0 as i128) >> FRAC)
    }
}

impl Div for Fx {
    type Output = Fx;
    fn div(self, o: Fx) -> Fx {
        if o.0 == 0 {
            // Wie „unendlich“: gesättigt, aber ohne Absturz und überall gleich.
            return match self.0.cmp(&0) {
                std::cmp::Ordering::Greater => Fx::MAX,
                std::cmp::Ordering::Less => Fx::MIN,
                std::cmp::Ordering::Equal => Fx::ZERO,
            };
        }
        sat(((self.0 as i128) << FRAC) / o.0 as i128)
    }
}

impl Neg for Fx {
    type Output = Fx;
    fn neg(self) -> Fx {
        Fx(self.0.saturating_neg())
    }
}

impl AddAssign for Fx {
    fn add_assign(&mut self, o: Fx) {
        *self = *self + o;
    }
}
impl SubAssign for Fx {
    fn sub_assign(&mut self, o: Fx) {
        *self = *self - o;
    }
}
impl MulAssign for Fx {
    fn mul_assign(&mut self, o: Fx) {
        *self = *self * o;
    }
}
impl DivAssign for Fx {
    fn div_assign(&mut self, o: Fx) {
        *self = *self / o;
    }
}

impl std::iter::Product for Fx {
    fn product<I: Iterator<Item = Fx>>(it: I) -> Fx {
        it.fold(Fx::ONE, |a, b| a * b)
    }
}

/// Rechnen auch mit Referenzen (`&Fx`), wie bei den Float-Typen.
macro_rules! ref_ops {
    ($ty:ty: $($tr:ident $f:ident),*) => {$(
        impl $tr<&$ty> for $ty {
            type Output = <$ty as $tr>::Output;
            fn $f(self, o: &$ty) -> Self::Output {
                $tr::$f(self, *o)
            }
        }
        impl $tr<$ty> for &$ty {
            type Output = <$ty as $tr>::Output;
            fn $f(self, o: $ty) -> Self::Output {
                $tr::$f(*self, o)
            }
        }
        impl $tr<&$ty> for &$ty {
            type Output = <$ty as $tr>::Output;
            fn $f(self, o: &$ty) -> Self::Output {
                $tr::$f(*self, *o)
            }
        }
    )*};
}
ref_ops!(Fx: Add add, Sub sub, Mul mul, Div div);

impl AddAssign<&Fx> for Fx {
    fn add_assign(&mut self, o: &Fx) {
        *self = *self + *o;
    }
}
impl SubAssign<&Fx> for Fx {
    fn sub_assign(&mut self, o: &Fx) {
        *self = *self - *o;
    }
}

impl Sum for Fx {
    fn sum<I: Iterator<Item = Fx>>(it: I) -> Fx {
        it.fold(Fx::ZERO, |a, b| a + b)
    }
}

impl<'a> Sum<&'a Fx> for Fx {
    fn sum<I: Iterator<Item = &'a Fx>>(it: I) -> Fx {
        it.fold(Fx::ZERO, |a, b| a + *b)
    }
}

macro_rules! from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Fx {
            fn from(n: $t) -> Fx {
                Fx::int(n as i64)
            }
        }
        impl Mul<$t> for Fx {
            type Output = Fx;
            fn mul(self, n: $t) -> Fx {
                Fx(self.0.saturating_mul(n as i64))
            }
        }
        impl Div<$t> for Fx {
            type Output = Fx;
            fn div(self, n: $t) -> Fx {
                if n == 0 { self / Fx::ZERO } else { Fx(self.0 / n as i64) }
            }
        }
    )*};
}
from_int!(i32, u32, i64, u64, u8, u16, i16, usize);

/// Ausgabe mit Genauigkeit (`{:.1}`) wie bei Floats, sonst bis zu sechs Stellen ohne Nullen.
impl fmt::Display for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let neg = self.0 < 0;
        let mag = self.0.unsigned_abs() as u128;
        let (digits, trim) = match f.precision() {
            Some(p) => (p.min(9), false),
            None => (6, true),
        };
        let scale = 10u128.pow(digits as u32);
        // Gerundet auf `digits` Stellen.
        let scaled = (mag * scale + (1u128 << (FRAC - 1))) >> FRAC;
        let (int, frac) = (scaled / scale, scaled % scale);
        let mut s = String::new();
        if neg && scaled != 0 {
            s.push('-');
        }
        s.push_str(&int.to_string());
        if digits > 0 {
            let mut fs = format!("{frac:0width$}", width = digits);
            if trim {
                while fs.ends_with('0') {
                    fs.pop();
                }
                if fs.is_empty() {
                    fs.push('0');
                }
            }
            s.push('.');
            s.push_str(&fs);
        }
        f.pad_integral(true, "", &s)
    }
}

impl fmt::Debug for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

// --- Datendateien und Spielstände: Dezimalzahlen rein, Dezimalzahlen raus ---------------------

impl Serialize for Fx {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.to_f64())
    }
}

struct FxVisitor;

impl Visitor<'_> for FxVisitor {
    type Value = Fx;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("eine Zahl")
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Fx, E> {
        Ok(Fx::from_f64(v))
    }
    fn visit_f32<E: de::Error>(self, v: f32) -> Result<Fx, E> {
        Ok(Fx::from_f32(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Fx, E> {
        Ok(Fx::int(v))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Fx, E> {
        Ok(Fx::int(v.min(i64::MAX as u64) as i64))
    }
}

impl<'de> Deserialize<'de> for Fx {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Fx, D::Error> {
        d.deserialize_any(FxVisitor)
    }
}

// =============================================================================================
// Vec2
// =============================================================================================

/// Punkt oder Vektor in der Ebene, Bestandteile in [`Fx`]. Dieselben Namen wie `bevy::Vec2`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(from = "(Fx, Fx)", into = "(Fx, Fx)")]
pub struct Vec2 {
    pub x: Fx,
    pub y: Fx,
}

impl From<(Fx, Fx)> for Vec2 {
    fn from(t: (Fx, Fx)) -> Vec2 {
        Vec2::new(t.0, t.1)
    }
}
impl From<Vec2> for (Fx, Fx) {
    fn from(v: Vec2) -> (Fx, Fx) {
        (v.x, v.y)
    }
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 {
        x: Fx::ZERO,
        y: Fx::ZERO,
    };
    pub const ONE: Vec2 = Vec2 {
        x: Fx::ONE,
        y: Fx::ONE,
    };
    pub const X: Vec2 = Vec2 {
        x: Fx::ONE,
        y: Fx::ZERO,
    };
    pub const Y: Vec2 = Vec2 {
        x: Fx::ZERO,
        y: Fx::ONE,
    };
    pub const NEG_X: Vec2 = Vec2 {
        x: Fx(-ONE_RAW),
        y: Fx::ZERO,
    };
    pub const NEG_Y: Vec2 = Vec2 {
        x: Fx::ZERO,
        y: Fx(-ONE_RAW),
    };

    pub const fn new(x: Fx, y: Fx) -> Vec2 {
        Vec2 { x, y }
    }

    pub const fn splat(v: Fx) -> Vec2 {
        Vec2 { x: v, y: v }
    }

    pub fn from_bevy(v: bevy::math::Vec2) -> Vec2 {
        Vec2::new(Fx::from_f32(v.x), Fx::from_f32(v.y))
    }

    pub fn to_bevy(self) -> bevy::math::Vec2 {
        bevy::math::Vec2::new(self.x.to_f32(), self.y.to_f32())
    }

    /// Richtung für einen Winkel (0 = +x).
    pub fn from_angle(a: Fx) -> Vec2 {
        let (s, c) = a.sin_cos();
        Vec2::new(c, s)
    }

    /// Winkel zur x-Achse in (-π, π].
    pub fn to_angle(self) -> Fx {
        self.y.atan2(self.x)
    }

    pub fn dot(self, o: Vec2) -> Fx {
        // Beide Produkte in voller Breite addieren, erst dann runden (genauer, kein Zwischenlauf).
        let v = self.x.0 as i128 * o.x.0 as i128 + self.y.0 as i128 * o.y.0 as i128;
        sat(v >> FRAC)
    }

    /// Kreuzprodukt als Skalar (x₁·y₂ − y₁·x₂).
    pub fn perp_dot(self, o: Vec2) -> Fx {
        let v = self.x.0 as i128 * o.y.0 as i128 - self.y.0 as i128 * o.x.0 as i128;
        sat(v >> FRAC)
    }

    /// Um 90° gegen den Uhrzeigersinn gedreht.
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }

    pub fn length_squared(self) -> Fx {
        self.dot(self)
    }

    pub fn length(self) -> Fx {
        // Q64 → Q32; die Wurzel verträgt jede Länge ohne Überlauf.
        sat(isqrt(self.len_sq128()) as i128)
    }

    /// `|self| > r` ohne Wurzel (exakt, kein Überlauf) – für Abstandsabfragen in heißen Schleifen.
    pub fn longer_than(self, r: Fx) -> bool {
        r.0 < 0 || self.len_sq128() > (r.0 as i128 * r.0 as i128) as u128
    }

    /// `|self| < r` ohne Wurzel.
    pub fn shorter_than(self, r: Fx) -> bool {
        r.0 > 0 && self.len_sq128() < (r.0 as i128 * r.0 as i128) as u128
    }

    /// Länge zum Quadrat in voller Breite (Q64).
    fn len_sq128(self) -> u128 {
        (self.x.0 as i128 * self.x.0 as i128 + self.y.0 as i128 * self.y.0 as i128) as u128
    }

    pub fn distance(self, o: Vec2) -> Fx {
        (self - o).length()
    }

    pub fn distance_squared(self, o: Vec2) -> Fx {
        (self - o).length_squared()
    }

    pub fn normalize_or_zero(self) -> Vec2 {
        let l = self.length();
        if l.0 == 0 { Vec2::ZERO } else { self / l }
    }

    pub fn normalize_or(self, fallback: Vec2) -> Vec2 {
        let l = self.length();
        if l.0 == 0 { fallback } else { self / l }
    }

    /// Wie `normalize_or_zero`; der Aufrufer sorgt für eine Länge > 0.
    pub fn normalize(self) -> Vec2 {
        self.normalize_or_zero()
    }

    pub fn clamp_length_max(self, max: Fx) -> Vec2 {
        let l = self.length();
        if l > max && l.0 > 0 {
            self * (max / l)
        } else {
            self
        }
    }

    pub fn clamp_length(self, min: Fx, max: Fx) -> Vec2 {
        let l = self.length();
        if l.0 == 0 {
            return self;
        }
        if l > max {
            self * (max / l)
        } else if l < min {
            self * (min / l)
        } else {
            self
        }
    }

    pub fn abs(self) -> Vec2 {
        Vec2::new(self.x.abs(), self.y.abs())
    }

    pub fn clamp(self, min: Vec2, max: Vec2) -> Vec2 {
        Vec2::new(self.x.clamp(min.x, max.x), self.y.clamp(min.y, max.y))
    }

    pub fn min(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x.min(o.x), self.y.min(o.y))
    }

    pub fn max(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x.max(o.x), self.y.max(o.y))
    }

    pub fn lerp(self, to: Vec2, t: Fx) -> Vec2 {
        self + (to - self) * t
    }

    /// Anteil von `self` entlang `onto`.
    pub fn project_onto(self, onto: Vec2) -> Vec2 {
        let d = onto.dot(onto);
        if d.0 == 0 {
            Vec2::ZERO
        } else {
            onto * (self.dot(onto) / d)
        }
    }

    /// Um den Winkel `a` drehen.
    pub fn rotate_by(self, a: Fx) -> Vec2 {
        let (s, c) = a.sin_cos();
        Vec2::new(c * self.x - s * self.y, s * self.x + c * self.y)
    }

    pub fn is_zero(self) -> bool {
        self.x.0 == 0 && self.y.0 == 0
    }

    pub fn min_element(self) -> Fx {
        self.x.min(self.y)
    }

    pub fn max_element(self) -> Fx {
        self.x.max(self.y)
    }

    /// Vorzeichenbehafteter Winkel von `self` zu `o` in (-π, π].
    pub fn angle_to(self, o: Vec2) -> Fx {
        self.perp_dot(o).atan2(self.dot(o))
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}
impl Mul<Fx> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: Fx) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}
impl Mul<Vec2> for Fx {
    type Output = Vec2;
    fn mul(self, v: Vec2) -> Vec2 {
        v * self
    }
}
impl Mul for Vec2 {
    type Output = Vec2;
    fn mul(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x * o.x, self.y * o.y)
    }
}
impl Div<Fx> for Vec2 {
    type Output = Vec2;
    fn div(self, k: Fx) -> Vec2 {
        Vec2::new(self.x / k, self.y / k)
    }
}
impl Div for Vec2 {
    type Output = Vec2;
    fn div(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x / o.x, self.y / o.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        *self = *self + o;
    }
}
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Vec2) {
        *self = *self - o;
    }
}
impl MulAssign<Fx> for Vec2 {
    fn mul_assign(&mut self, k: Fx) {
        *self = *self * k;
    }
}
impl DivAssign<Fx> for Vec2 {
    fn div_assign(&mut self, k: Fx) {
        *self = *self / k;
    }
}
ref_ops!(Vec2: Add add, Sub sub);

impl Sum for Vec2 {
    fn sum<I: Iterator<Item = Vec2>>(it: I) -> Vec2 {
        it.fold(Vec2::ZERO, |a, b| a + b)
    }
}

impl fmt::Debug for Vec2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Vec2({}, {})", self.x, self.y)
    }
}
impl fmt::Display for Vec2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

// =============================================================================================
// Brücke zu f32 / bevy::Vec2 – nur für Anzeige und Eingabegeräte, nie in der Simulation
// (der Test `no_floats_in_sim` verbietet dort jeden Float-Typ). Das Ergebnis solcher
// Mischrechnungen ist immer ein Float.
// =============================================================================================

type BVec2 = bevy::math::Vec2;
type BVec3 = bevy::math::Vec3;

macro_rules! float_bridge {
    ($($tr:ident $f:ident $op:tt),*) => {$(
        impl $tr<f32> for Fx {
            type Output = f32;
            fn $f(self, o: f32) -> f32 {
                self.to_f32() $op o
            }
        }
        impl $tr<Fx> for f32 {
            type Output = f32;
            fn $f(self, o: Fx) -> f32 {
                self $op o.to_f32()
            }
        }
        impl $tr<f32> for &Fx {
            type Output = f32;
            fn $f(self, o: f32) -> f32 {
                self.to_f32() $op o
            }
        }
        impl $tr<&Fx> for f32 {
            type Output = f32;
            fn $f(self, o: &Fx) -> f32 {
                self $op o.to_f32()
            }
        }
    )*};
}
float_bridge!(Add add +, Sub sub -, Mul mul *, Div div /);

impl Rem for Fx {
    type Output = Fx;
    fn rem(self, o: Fx) -> Fx {
        if o.0 == 0 { Fx::ZERO } else { Fx(self.0 % o.0) }
    }
}
impl Rem<f32> for Fx {
    type Output = f32;
    fn rem(self, o: f32) -> f32 {
        self.to_f32() % o
    }
}

impl AddAssign<f32> for Fx {
    fn add_assign(&mut self, o: f32) {
        *self += Fx::from_f32(o);
    }
}
impl SubAssign<f32> for Fx {
    fn sub_assign(&mut self, o: f32) {
        *self -= Fx::from_f32(o);
    }
}
impl MulAssign<f32> for Fx {
    fn mul_assign(&mut self, o: f32) {
        *self *= Fx::from_f32(o);
    }
}

impl Sum<Fx> for f32 {
    fn sum<I: Iterator<Item = Fx>>(it: I) -> f32 {
        it.map(Fx::to_f32).sum()
    }
}
impl<'a> Sum<&'a Fx> for f32 {
    fn sum<I: Iterator<Item = &'a Fx>>(it: I) -> f32 {
        it.map(|x| x.to_f32()).sum()
    }
}
impl Sum<Vec2> for BVec2 {
    fn sum<I: Iterator<Item = Vec2>>(it: I) -> BVec2 {
        it.map(Vec2::to_bevy).sum()
    }
}

impl AddAssign<Fx> for f32 {
    fn add_assign(&mut self, o: Fx) {
        *self += o.to_f32();
    }
}
impl SubAssign<Fx> for f32 {
    fn sub_assign(&mut self, o: Fx) {
        *self -= o.to_f32();
    }
}
impl MulAssign<Fx> for f32 {
    fn mul_assign(&mut self, o: Fx) {
        *self *= o.to_f32();
    }
}

impl PartialEq<f32> for Fx {
    fn eq(&self, o: &f32) -> bool {
        self.to_f32() == *o
    }
}
impl PartialOrd<f32> for Fx {
    fn partial_cmp(&self, o: &f32) -> Option<std::cmp::Ordering> {
        self.to_f32().partial_cmp(o)
    }
}
impl PartialEq<Fx> for f32 {
    fn eq(&self, o: &Fx) -> bool {
        *self == o.to_f32()
    }
}
impl PartialOrd<Fx> for f32 {
    fn partial_cmp(&self, o: &Fx) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&o.to_f32())
    }
}

impl From<Fx> for f32 {
    fn from(x: Fx) -> f32 {
        x.to_f32()
    }
}

impl Vec2 {
    /// Für die Anzeige: als bevy-Vektor mit z.
    pub fn extend(self, z: f32) -> BVec3 {
        self.to_bevy().extend(z)
    }
}

impl From<Vec2> for BVec2 {
    fn from(v: Vec2) -> BVec2 {
        v.to_bevy()
    }
}

macro_rules! vec_bridge {
    ($($tr:ident $f:ident $op:tt),*) => {$(
        impl $tr<BVec2> for Vec2 {
            type Output = BVec2;
            fn $f(self, o: BVec2) -> BVec2 {
                self.to_bevy() $op o
            }
        }
        impl $tr<Vec2> for BVec2 {
            type Output = BVec2;
            fn $f(self, o: Vec2) -> BVec2 {
                self $op o.to_bevy()
            }
        }
    )*};
}
vec_bridge!(Add add +, Sub sub -);

impl Mul<f32> for Vec2 {
    type Output = BVec2;
    fn mul(self, k: f32) -> BVec2 {
        self.to_bevy() * k
    }
}
impl Mul<Vec2> for f32 {
    type Output = BVec2;
    fn mul(self, v: Vec2) -> BVec2 {
        self * v.to_bevy()
    }
}
impl Mul<BVec2> for Vec2 {
    type Output = BVec2;
    fn mul(self, o: BVec2) -> BVec2 {
        self.to_bevy() * o
    }
}
impl Mul<Fx> for BVec3 {
    type Output = BVec3;
    fn mul(self, k: Fx) -> BVec3 {
        self * k.to_f32()
    }
}
impl Mul<Fx> for BVec2 {
    type Output = BVec2;
    fn mul(self, k: Fx) -> BVec2 {
        self * k.to_f32()
    }
}
impl Div<Fx> for BVec2 {
    type Output = BVec2;
    fn div(self, k: Fx) -> BVec2 {
        self / k.to_f32()
    }
}
impl Div<f32> for Vec2 {
    type Output = BVec2;
    fn div(self, k: f32) -> BVec2 {
        self.to_bevy() / k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Fx, b: f64, tol: f64) {
        assert!((a.to_f64() - b).abs() <= tol, "{a} gegen {b}");
    }

    #[test]
    fn arithmetic_matches_floats() {
        let (a, b) = (fx!(3.25), fx!(-1.5));
        close(a + b, 1.75, 1e-9);
        close(a - b, 4.75, 1e-9);
        close(a * b, -4.875, 1e-9);
        close(a / b, -3.25 / 1.5, 1e-9);
        assert_eq!(fx!(2.0) * fx!(0.5), Fx::ONE);
        // Teilen durch null stürzt nicht ab und ist überall gleich.
        assert_eq!(Fx::ONE / Fx::ZERO, Fx::MAX);
        assert_eq!(-Fx::ONE / Fx::ZERO, Fx::MIN);
        assert_eq!(Fx::ZERO / Fx::ZERO, Fx::ZERO);
        // Sättigung statt Überlauf.
        assert_eq!(Fx::MAX + Fx::ONE, Fx::MAX);
        assert_eq!(Fx::MAX * fx!(2.0), Fx::MAX);
    }

    #[test]
    fn rounding_and_integers() {
        assert_eq!(fx!(2.5).round_int(), 3);
        assert_eq!(fx!(-2.5).round_int(), -3);
        assert_eq!(fx!(-1.2).floor_int(), -2);
        assert_eq!(fx!(-1.2).to_int(), -1);
        assert_eq!(fx!(1.2).ceil_int(), 2);
        assert_eq!(fx!(7.75).fract(), fx!(0.75));
        assert_eq!(fx!(-7.0).rem_euclid(fx!(3.0)), fx!(2.0));
        assert_eq!(Fx::from(7usize), fx!(7.0));
        assert_eq!(fx!(2.0).powi(5), fx!(32.0));
        assert_eq!(fx!(2.0).powi(-2), fx!(0.25));
    }

    #[test]
    fn sqrt_is_exact_for_squares_and_close_otherwise() {
        assert_eq!(fx!(16.0).sqrt(), fx!(4.0));
        assert_eq!(fx!(0.25).sqrt(), fx!(0.5));
        close(fx!(2.0).sqrt(), 2f64.sqrt(), 3e-10);
        assert_eq!(fx!(-1.0).sqrt(), Fx::ZERO);
        // Länge ohne Überlauf auch bei großen Koordinaten.
        let big = Vec2::new(fx!(30000.0), fx!(40000.0));
        close(big.length(), 50000.0, 1e-6);
        close(Vec2::new(fx!(3.0), fx!(4.0)).length(), 5.0, 1e-9);
    }

    #[test]
    fn trigonometry_stays_within_a_few_billionths() {
        let mut worst = 0f64;
        for k in -2000..=2000 {
            let a = k as f64 * 0.0173;
            let (s, c) = Fx::from_f64(a).sin_cos();
            worst = worst
                .max((s.to_f64() - a.sin()).abs())
                .max((c.to_f64() - a.cos()).abs());
        }
        assert!(worst < 5e-9, "{worst}");
        for k in 0..720 {
            let a = (k as f64 / 720.0) * std::f64::consts::TAU - std::f64::consts::PI;
            let (y, x) = (a.sin() * 3.7, a.cos() * 3.7);
            let got = Fx::from_f64(y).atan2(Fx::from_f64(x)).to_f64();
            let want = y.atan2(x);
            let raw = (got - want).abs();
            let d = raw.min(std::f64::consts::TAU - raw);
            assert!(d < 5e-9, "{a}: {got} gegen {want}");
        }
        assert_eq!(Fx::ZERO.atan2(Fx::ZERO), Fx::ZERO);
        close(Fx::ZERO.atan2(fx!(-1.0)), std::f64::consts::PI, 1e-9);
        // Kleine Vektoren verlieren keine Genauigkeit.
        close(
            Fx::from_raw(3).atan2(Fx::from_raw(4)),
            (3f64).atan2(4.0),
            1e-9,
        );
    }

    #[test]
    fn angles_wrap_and_vectors_rotate() {
        close(
            fx!(3.5).angle_to(fx!(-3.0)),
            -3.0 - 3.5 + std::f64::consts::TAU,
            1e-8,
        );
        let v = Vec2::X.rotate_by(Fx::HALF_PI);
        assert!(v.x.abs() < fx!(0.000001) && (v.y - Fx::ONE).abs() < fx!(0.000001));
        let n = Vec2::new(fx!(0.0), fx!(7.0)).normalize_or_zero();
        close(n.y, 1.0, 1e-9);
        assert_eq!(Vec2::ZERO.normalize_or_zero(), Vec2::ZERO);
        close(
            Vec2::new(fx!(1.0), fx!(1.0)).to_angle(),
            std::f64::consts::FRAC_PI_4,
            1e-9,
        );
    }

    #[test]
    fn display_follows_float_formatting() {
        assert_eq!(format!("{:.1}", fx!(3.25)), "3.3");
        assert_eq!(format!("{:.0}", fx!(2.5)), "3");
        assert_eq!(format!("{:.2}", fx!(-0.004)), "0.00");
        assert_eq!(format!("{:.1}", fx!(-12.34)), "-12.3");
        assert_eq!(format!("{}", fx!(1.5)), "1.5");
        assert_eq!(format!("{}", fx!(2.0)), "2.0");
        assert_eq!(format!("{:>6.1}", fx!(4.04)), "   4.0");
    }

    #[test]
    fn data_files_round_trip() {
        let v: Vec<Fx> = ron::from_str("[1, 2.5, -0.125, 300]").unwrap();
        assert_eq!(v, vec![fx!(1.0), fx!(2.5), fx!(-0.125), fx!(300.0)]);
        let text = ron::ser::to_string(&Vec2::new(fx!(1.5), fx!(-2.0))).unwrap();
        let back: Vec2 = ron::from_str(&text).unwrap();
        assert_eq!(back, Vec2::new(fx!(1.5), fx!(-2.0)));
        // Spielstände mit Dezimalzahlen bleiben lesbar.
        let p: (Fx, Fx) = ron::from_str("(0.6, 1500.25)").unwrap();
        assert_eq!(p, (fx!(0.6), fx!(1500.25)));
    }
}
