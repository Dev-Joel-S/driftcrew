//! 2D-Geometrie und Kontakterzeugung: konvexe Vierecke (OBB) und Kreise.
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
#[cfg(test)]
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
}

/// Konvexes Viereck, Ecken gegen den Uhrzeigersinn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub v: [Vec2; 4],
    pub n: [Vec2; 4],
}

impl Quad {
    pub fn obb(center: Vec2, half: Vec2, angle: f32) -> Quad {
        let local = [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(half.x, half.y),
            Vec2::new(-half.x, half.y),
        ];
        let v = local.map(|p| center + rot(p, angle));
        Quad::from_verts(v)
    }

    pub fn from_verts(v: [Vec2; 4]) -> Quad {
        let mut n = [Vec2::ZERO; 4];
        for i in 0..4 {
            let e = v[(i + 1) % 4] - v[i];
            n[i] = Vec2::new(e.y, -e.x).normalize_or_zero();
        }
        Quad { v, n }
    }

    pub fn aabb(&self) -> Aabb {
        let mut min = self.v[0];
        let mut max = self.v[0];
        for p in &self.v[1..] {
            min = min.min(*p);
            max = max.max(*p);
        }
        Aabb { min, max }
    }

    pub fn center(&self) -> Vec2 {
        (self.v[0] + self.v[1] + self.v[2] + self.v[3]) * 0.25
    }

    pub fn contains(&self, p: Vec2) -> bool {
        (0..4).all(|i| self.n[i].dot(p - self.v[i]) <= 0.0)
    }
}

/// Kontakt zwischen A und B. `normal` zeigt von A nach B.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub point: Vec2,
    pub normal: Vec2,
    pub depth: f32,
}

fn max_separation(a: &Quad, b: &Quad) -> (usize, f32) {
    let mut best = (0, f32::MIN);
    for i in 0..4 {
        let n = a.n[i];
        let vi = a.v[i];
        let mut s = f32::MAX;
        for p in &b.v {
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

/// Viereck gegen Viereck (SAT + Clipping, wie in Box2D). Liefert 0–2 Kontakte.
pub fn quad_quad(a: &Quad, b: &Quad, out: &mut Vec<Contact>) {
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
    for i in 0..4 {
        let d = ref_n.dot(incident.n[i]);
        if d < min_dot {
            min_dot = d;
            inc = i;
        }
    }
    let pts = [(incident.v[inc], true), (incident.v[(inc + 1) % 4], true)];
    let v1 = reference.v[edge];
    let v2 = reference.v[(edge + 1) % 4];
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

/// Viereck gegen Kreis. Normale zeigt vom Viereck zum Kreis.
pub fn quad_circle(a: &Quad, c: Vec2, r: f32) -> Option<Contact> {
    let mut edge = 0;
    let mut sep = f32::MIN;
    for i in 0..4 {
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
    let v2 = a.v[(edge + 1) % 4];
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

/// Strahl gegen Viereck (Slab-Test über die Kanten).
pub fn ray_quad(origin: Vec2, dir: Vec2, max: f32, q: &Quad) -> Option<f32> {
    let mut t_enter = 0.0f32;
    let mut t_exit = max;
    for i in 0..4 {
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
        let floor = Quad::obb(Vec2::new(0.0, -1.0), Vec2::new(10.0, 1.0), 0.0);
        let boxq = Quad::obb(Vec2::new(0.0, 0.45), Vec2::new(0.5, 0.5), 0.0);
        let mut out = Vec::new();
        quad_quad(&floor, &boxq, &mut out);
        assert_eq!(out.len(), 2);
        for c in &out {
            assert!((c.normal - Vec2::Y).length() < 1e-4, "{:?}", c.normal);
            assert!((c.depth - 0.05).abs() < 1e-3);
        }
    }

    #[test]
    fn separated_boxes_no_contact() {
        let a = Quad::obb(Vec2::ZERO, Vec2::splat(1.0), 0.3);
        let b = Quad::obb(Vec2::new(5.0, 0.0), Vec2::splat(1.0), 0.0);
        let mut out = Vec::new();
        quad_quad(&a, &b, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn crossing_thin_bars_collide() {
        // Zwei dünne Balken, die sich kreuzen, ohne dass eine Ecke im anderen liegt.
        let a = Quad::obb(Vec2::ZERO, Vec2::new(5.0, 0.2), 0.0);
        let b = Quad::obb(Vec2::ZERO, Vec2::new(5.0, 0.2), 1.2);
        let mut out = Vec::new();
        quad_quad(&a, &b, &mut out);
        assert!(!out.is_empty());
    }

    #[test]
    fn circle_hits_box_face() {
        let q = Quad::obb(Vec2::ZERO, Vec2::splat(1.0), 0.0);
        let c = quad_circle(&q, Vec2::new(0.0, 1.4), 0.5).unwrap();
        assert!((c.normal - Vec2::Y).length() < 1e-4);
        assert!((c.depth - 0.1).abs() < 1e-4);
    }

    #[test]
    fn ray_hits() {
        let t = ray_circle(Vec2::ZERO, Vec2::X, 10.0, Vec2::new(5.0, 0.0), 1.0).unwrap();
        assert!((t - 4.0).abs() < 1e-4);
        let q = Quad::obb(Vec2::new(5.0, 0.0), Vec2::splat(1.0), 0.0);
        let t = ray_quad(Vec2::ZERO, Vec2::X, 10.0, &q).unwrap();
        assert!((t - 4.0).abs() < 1e-4);
    }
}
