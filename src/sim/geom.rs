//! 2D-Geometrie und Kontakterzeugung: konvexe Polygone (bis 8 Ecken) und Kreise.
//! Bewusst klein gehalten, damit ein späterer Umstieg auf Fixed-Point leicht fällt.

use bevy::math::Vec2;

pub fn rot(v: Vec2, angle: f32) -> Vec2 {
    let (s, c) = angle.sin_cos();
    Vec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// 2D-Kreuzprodukt (Skalar).
pub fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// ω × r für einen Skalar ω.
pub fn cross_sv(w: f32, r: Vec2) -> Vec2 {
    Vec2::new(-w * r.y, w * r.x)
}

/// Richtung der Schiffsnase für einen Winkel (0 = +y).
pub fn forward(angle: f32) -> Vec2 {
    Vec2::new(-angle.sin(), angle.cos())
}

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x
            && self.max.x >= o.min.x
            && self.min.y <= o.max.y
            && self.max.y >= o.min.y
    }
    pub fn around(c: Vec2, r: f32) -> Aabb {
        Aabb {
            min: c - Vec2::splat(r),
            max: c + Vec2::splat(r),
        }
    }
    pub fn expand(&self, m: f32) -> Aabb {
        Aabb {
            min: self.min - Vec2::splat(m),
            max: self.max + Vec2::splat(m),
        }
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    /// Schneidet die Strecke a→b die Box? (Slab-Test)
    pub fn hits_segment(&self, a: Vec2, b: Vec2) -> bool {
        self.segment_entry(a, b).is_some()
    }
    /// Wo (0..1 entlang a→b) die Strecke in die Box eintritt.
    pub fn segment_entry(&self, a: Vec2, b: Vec2) -> Option<f32> {
        let d = b - a;
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for (o, dd, lo, hi) in [
            (a.x, d.x, self.min.x, self.max.x),
            (a.y, d.y, self.min.y, self.max.y),
        ] {
            if dd.abs() < 1e-6 {
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }
            let (mut ta, mut tb) = ((lo - o) / dd, (hi - o) / dd);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return None;
            }
        }
        Some(t0)
    }
    pub fn corners(&self) -> [Vec2; 4] {
        [
            self.min,
            Vec2::new(self.max.x, self.min.y),
            self.max,
            Vec2::new(self.min.x, self.max.y),
        ]
    }
}

pub const MAX_VERTS: usize = 8;

/// Konvexes Polygon (3–8 Ecken), Ecken gegen den Uhrzeigersinn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Poly {
    pub v: [Vec2; MAX_VERTS],
    pub n: [Vec2; MAX_VERTS],
    pub count: usize,
}

impl Poly {
    pub fn obb(center: Vec2, half: Vec2, angle: f32) -> Poly {
        let local = [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(half.x, half.y),
            Vec2::new(-half.x, half.y),
        ];
        Poly::transformed(&local, center, angle)
    }

    /// Lokale Umrisspunkte (CCW) drehen und verschieben.
    pub fn transformed(local: &[Vec2], center: Vec2, angle: f32) -> Poly {
        let mut pts = [Vec2::ZERO; MAX_VERTS];
        let n = local.len().min(MAX_VERTS);
        for i in 0..n {
            pts[i] = center + rot(local[i], angle);
        }
        Poly::from_points(&pts[..n])
    }

    pub fn from_points(pts: &[Vec2]) -> Poly {
        let count = pts.len().clamp(3, MAX_VERTS).min(pts.len());
        let mut v = [Vec2::ZERO; MAX_VERTS];
        v[..count].copy_from_slice(&pts[..count]);
        let mut n = [Vec2::ZERO; MAX_VERTS];
        for i in 0..count {
            let e = v[(i + 1) % count] - v[i];
            n[i] = Vec2::new(e.y, -e.x).normalize_or_zero();
        }
        Poly { v, n, count }
    }

    pub fn verts(&self) -> &[Vec2] {
        &self.v[..self.count]
    }

    pub fn aabb(&self) -> Aabb {
        let mut min = self.v[0];
        let mut max = self.v[0];
        for p in &self.verts()[1..] {
            min = min.min(*p);
            max = max.max(*p);
        }
        Aabb { min, max }
    }

    pub fn center(&self) -> Vec2 {
        self.verts().iter().copied().sum::<Vec2>() / self.count as f32
    }

    pub fn contains(&self, p: Vec2) -> bool {
        (0..self.count).all(|i| self.n[i].dot(p - self.v[i]) <= 0.0)
    }
}

/// Kontakt zwischen A und B. `normal` zeigt von A nach B.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub point: Vec2,
    pub normal: Vec2,
    pub depth: f32,
}

fn max_separation(a: &Poly, b: &Poly) -> (usize, f32) {
    let mut best = (0, f32::MIN);
    for i in 0..a.count {
        let n = a.n[i];
        let vi = a.v[i];
        let mut s = f32::MAX;
        for p in b.verts() {
            s = s.min(n.dot(*p - vi));
        }
        if s > best.1 {
            best = (i, s);
        }
    }
    best
}

fn clip(points: &[(Vec2, bool)], n: Vec2, offset: f32) -> Vec<(Vec2, bool)> {
    let mut out = Vec::with_capacity(2);
    if points.len() < 2 {
        return out;
    }
    let d0 = n.dot(points[0].0) - offset;
    let d1 = n.dot(points[1].0) - offset;
    if d0 <= 0.0 {
        out.push(points[0]);
    }
    if d1 <= 0.0 {
        out.push(points[1]);
    }
    if d0 * d1 < 0.0 {
        let t = d0 / (d0 - d1);
        out.push((points[0].0 + (points[1].0 - points[0].0) * t, false));
    }
    out
}

/// Polygon gegen Polygon (SAT + Clipping, wie in Box2D). Liefert 0–2 Kontakte.
pub fn poly_poly(a: &Poly, b: &Poly, out: &mut Vec<Contact>) {
    let (ea, sa) = max_separation(a, b);
    if sa > 0.0 {
        return;
    }
    let (eb, sb) = max_separation(b, a);
    if sb > 0.0 {
        return;
    }
    let (reference, incident, edge, flip) = if sb > sa + 0.001 {
        (b, a, eb, true)
    } else {
        (a, b, ea, false)
    };
    let ref_n = reference.n[edge];
    // Inzidente Kante: Normale am stärksten entgegengesetzt zur Referenznormale.
    let mut inc = 0;
    let mut min_dot = f32::MAX;
    for i in 0..incident.count {
        let d = ref_n.dot(incident.n[i]);
        if d < min_dot {
            min_dot = d;
            inc = i;
        }
    }
    let pts = [
        (incident.v[inc], true),
        (incident.v[(inc + 1) % incident.count], true),
    ];
    let v1 = reference.v[edge];
    let v2 = reference.v[(edge + 1) % reference.count];
    let t = (v2 - v1).normalize_or_zero();
    let c1 = clip(&pts, -t, -t.dot(v1));
    let c2 = clip(&c1, t, t.dot(v2));
    let front = ref_n.dot(v1);
    let normal = if flip { -ref_n } else { ref_n };
    for (p, _) in c2 {
        let sep = ref_n.dot(p) - front;
        if sep <= 0.0 {
            out.push(Contact {
                point: p - ref_n * (sep * 0.5),
                normal,
                depth: -sep,
            });
        }
    }
}

/// Polygon gegen Kreis. Normale zeigt vom Polygon zum Kreis.
pub fn poly_circle(a: &Poly, c: Vec2, r: f32) -> Option<Contact> {
    let mut edge = 0;
    let mut sep = f32::MIN;
    for i in 0..a.count {
        let s = a.n[i].dot(c - a.v[i]);
        if s > r {
            return None;
        }
        if s > sep {
            sep = s;
            edge = i;
        }
    }
    let v1 = a.v[edge];
    let v2 = a.v[(edge + 1) % a.count];
    if sep < 1e-5 {
        // Mittelpunkt im Viereck.
        let n = a.n[edge];
        return Some(Contact {
            point: c - n * r,
            normal: n,
            depth: r - sep,
        });
    }
    let u1 = (c - v1).dot(v2 - v1);
    let u2 = (c - v2).dot(v1 - v2);
    let corner = if u1 <= 0.0 {
        Some(v1)
    } else if u2 <= 0.0 {
        Some(v2)
    } else {
        None
    };
    match corner {
        Some(v) => {
            let d = c - v;
            let dist = d.length();
            if dist > r || dist < 1e-6 {
                return None;
            }
            Some(Contact {
                point: v,
                normal: d / dist,
                depth: r - dist,
            })
        }
        None => {
            let n = a.n[edge];
            Some(Contact {
                point: c - n * r,
                normal: n,
                depth: r - sep,
            })
        }
    }
}

/// Kreis gegen Kreis. Normale zeigt von A nach B.
pub fn circle_circle(a: Vec2, ra: f32, b: Vec2, rb: f32) -> Option<Contact> {
    let d = b - a;
    let dist2 = d.length_squared();
    let r = ra + rb;
    if dist2 > r * r {
        return None;
    }
    let dist = dist2.sqrt();
    let normal = if dist > 1e-6 { d / dist } else { Vec2::Y };
    Some(Contact {
        point: a + normal * ra,
        normal,
        depth: r - dist,
    })
}

/// Schneiden sich die Strecken a0–a1 und b0–b1? (Berührung zählt.)
pub fn segments_cross(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> bool {
    let r = a1 - a0;
    let s = b1 - b0;
    let den = cross(r, s);
    if den.abs() < 1e-9 {
        return false;
    }
    let t = cross(b0 - a0, s) / den;
    let u = cross(b0 - a0, r) / den;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

/// Strahl gegen Kreis: Abstand entlang des Strahls (dir normiert) oder None.
pub fn ray_circle(origin: Vec2, dir: Vec2, max: f32, c: Vec2, r: f32) -> Option<f32> {
    let m = origin - c;
    let b = m.dot(dir);
    let cc = m.length_squared() - r * r;
    if cc > 0.0 && b > 0.0 {
        return None;
    }
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()).max(0.0);
    if t <= max { Some(t) } else { None }
}

/// Strahl gegen konvexes Polygon (Slab-Test über die Kanten).
pub fn ray_poly(origin: Vec2, dir: Vec2, max: f32, q: &Poly) -> Option<f32> {
    let mut t_enter = 0.0f32;
    let mut t_exit = max;
    for i in 0..q.count {
        let n = q.n[i];
        let denom = n.dot(dir);
        let dist = n.dot(q.v[i] - origin);
        if denom.abs() < 1e-8 {
            if dist < 0.0 {
                return None;
            }
            continue;
        }
        let t = dist / denom;
        if denom < 0.0 {
            t_enter = t_enter.max(t);
        } else {
            t_exit = t_exit.min(t);
        }
        if t_enter > t_exit {
            return None;
        }
    }
    Some(t_enter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resting_box_on_floor_gives_two_contacts() {
        let floor = Poly::obb(Vec2::new(0.0, -1.0), Vec2::new(10.0, 1.0), 0.0);
        let boxq = Poly::obb(Vec2::new(0.0, 0.45), Vec2::new(0.5, 0.5), 0.0);
        let mut out = Vec::new();
        poly_poly(&floor, &boxq, &mut out);
        assert_eq!(out.len(), 2);
        for c in &out {
            assert!((c.normal - Vec2::Y).length() < 1e-4, "{:?}", c.normal);
            assert!((c.depth - 0.05).abs() < 1e-3);
        }
    }

    #[test]
    fn separated_boxes_no_contact() {
        let a = Poly::obb(Vec2::ZERO, Vec2::splat(1.0), 0.3);
        let b = Poly::obb(Vec2::new(5.0, 0.0), Vec2::splat(1.0), 0.0);
        let mut out = Vec::new();
        poly_poly(&a, &b, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn crossing_thin_bars_collide() {
        // Zwei dünne Balken, die sich kreuzen, ohne dass eine Ecke im anderen liegt.
        let a = Poly::obb(Vec2::ZERO, Vec2::new(5.0, 0.2), 0.0);
        let b = Poly::obb(Vec2::ZERO, Vec2::new(5.0, 0.2), 1.2);
        let mut out = Vec::new();
        poly_poly(&a, &b, &mut out);
        assert!(!out.is_empty());
    }

    #[test]
    fn circle_hits_box_face() {
        let q = Poly::obb(Vec2::ZERO, Vec2::splat(1.0), 0.0);
        let c = poly_circle(&q, Vec2::new(0.0, 1.4), 0.5).unwrap();
        assert!((c.normal - Vec2::Y).length() < 1e-4);
        assert!((c.depth - 0.1).abs() < 1e-4);
    }

    #[test]
    fn ray_hits() {
        let t = ray_circle(Vec2::ZERO, Vec2::X, 10.0, Vec2::new(5.0, 0.0), 1.0).unwrap();
        assert!((t - 4.0).abs() < 1e-4);
        let q = Poly::obb(Vec2::new(5.0, 0.0), Vec2::splat(1.0), 0.0);
        let t = ray_poly(Vec2::ZERO, Vec2::X, 10.0, &q).unwrap();
        assert!((t - 4.0).abs() < 1e-4);
    }

    #[test]
    fn box_slides_on_slope() {
        // Schräge: volle Ecke unten rechts, Hypotenuse zeigt nach oben links.
        let slope = Poly::from_points(&[
            Vec2::new(-2.0, -2.0),
            Vec2::new(2.0, -2.0),
            Vec2::new(2.0, 2.0),
        ]);
        let boxq = Poly::obb(Vec2::new(0.0, 0.55), Vec2::splat(0.5), 0.0);
        let mut out = Vec::new();
        poly_poly(&slope, &boxq, &mut out);
        assert!(!out.is_empty());
        let n = Vec2::new(-1.0, 1.0).normalize();
        for c in &out {
            assert!((c.normal - n).length() < 1e-3, "Normale {:?}", c.normal);
        }
    }

    #[test]
    fn shapes_are_convex_and_ccw() {
        use crate::sim::data::PartShape;
        for shape in [
            PartShape::Box,
            PartShape::Taper(0.5, 1.0),
            PartShape::Taper(0.0, 1.0),
            PartShape::Chamfer(0.4),
            PartShape::Nose(0.3),
            PartShape::Tail(0.3),
            PartShape::Wing(0.5, 1.0),
        ] {
            let pts = shape.outline(Vec2::new(1.0, 0.8));
            assert!(pts.len() >= 3 && pts.len() <= MAX_VERTS, "{shape:?}");
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                let c = pts[(i + 2) % pts.len()];
                assert!(cross(b - a, c - b) >= -1e-5, "{shape:?} nicht konvex/CCW");
            }
        }
    }
}
