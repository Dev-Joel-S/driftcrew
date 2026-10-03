//! Prozedurale Meshes: abgeschrägte Blöcke im Stil der Vorlage (mit Rahmen und
//! eingelassener Platte), Kristalle, facettierte Asteroiden.

use bevy::asset::RenderAssetUsages;
use bevy::math::{Vec2, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;

use super::textures::fbm3;

#[derive(Default)]
struct Builder {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl Builder {
    /// Viereck mit automatischer Ausrichtung: die Normale zeigt Richtung `out`.
    fn quad(&mut self, v: [Vec3; 4], uv: [[f32; 2]; 4], out: Vec3) {
        let mut v = v;
        let mut uv = uv;
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        if n.dot(out) < 0.0 {
            v.swap(1, 3);
            uv.swap(1, 3);
        }
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        let base = self.pos.len() as u32;
        for i in 0..4 {
            self.pos.push(v[i].to_array());
            self.nrm.push(n.to_array());
            self.uv.push(uv[i]);
        }
        self.idx
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn tri(&mut self, v: [Vec3; 3], out: Vec3) {
        let mut v = v;
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        if n.dot(out) < 0.0 {
            v.swap(1, 2);
        }
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        let base = self.pos.len() as u32;
        for p in v {
            self.pos.push(p.to_array());
            self.nrm.push(n.to_array());
            self.uv.push([p.x * 0.5 + 0.5, 0.5 - p.y * 0.5]);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2]);
    }

    fn build(self, tangents: bool) -> Mesh {
        let mut m = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_indices(Indices::U32(self.idx));
        if tangents {
            let _ = m.generate_tangents();
        }
        m
    }
}

/// Ring aus vier Trapezen zwischen zwei Rechtecken.
fn ring(b: &mut Builder, outer: (Vec2, f32), inner: (Vec2, f32), size: Vec2, out_hint: f32) {
    let (oh, oz) = outer;
    let (ih, iz) = inner;
    let uvp = |p: Vec3| [p.x / size.x + 0.5, 0.5 - p.y / size.y];
    let corners = |h: Vec2, z: f32| {
        [
            Vec3::new(-h.x, -h.y, z),
            Vec3::new(h.x, -h.y, z),
            Vec3::new(h.x, h.y, z),
            Vec3::new(-h.x, h.y, z),
        ]
    };
    let o = corners(oh, oz);
    let i = corners(ih, iz);
    let sides = [Vec3::NEG_Y, Vec3::X, Vec3::Y, Vec3::NEG_X];
    for k in 0..4 {
        let k2 = (k + 1) % 4;
        let quad = [o[k], o[k2], i[k2], i[k]];
        let hint = (sides[k] * out_hint + Vec3::Z).normalize();
        b.quad(quad, quad.map(uvp), hint);
    }
}

/// Abgeschrägter Block wie in der Vorlage: Fase außen, flacher Rahmen, eingelassene Platte.
pub fn beveled_block(size: Vec3, bevel: f32, frame: f32, recess: f32) -> Mesh {
    let mut b = Builder::default();
    let h = Vec2::new(size.x * 0.5, size.y * 0.5);
    let z_back = -size.z * 0.5;
    let z0 = size.z * 0.5 - bevel;
    let z1 = size.z * 0.5;
    let s2 = Vec2::new(size.x, size.y);

    // Seiten
    let side_uv = |a: f32, z: f32, len: f32| [a / len + 0.5, (z1 - z) / size.z];
    for (axis, len, other) in [(Vec3::X, size.y, h.x), (Vec3::NEG_X, size.y, h.x)] {
        let x = axis.x * other;
        let q = [
            Vec3::new(x, -h.y, z_back),
            Vec3::new(x, h.y, z_back),
            Vec3::new(x, h.y, z0),
            Vec3::new(x, -h.y, z0),
        ];
        b.quad(q, q.map(|p| side_uv(p.y, p.z, len)), axis);
    }
    for (axis, len, other) in [(Vec3::Y, size.x, h.y), (Vec3::NEG_Y, size.x, h.y)] {
        let y = axis.y * other;
        let q = [
            Vec3::new(-h.x, y, z_back),
            Vec3::new(h.x, y, z_back),
            Vec3::new(h.x, y, z0),
            Vec3::new(-h.x, y, z0),
        ];
        b.quad(q, q.map(|p| side_uv(p.x, p.z, len)), axis);
    }

    let front = h - Vec2::splat(bevel);
    ring(&mut b, (h, z0), (front, z1), s2, 1.0);
    if frame > 0.0 && recess > 0.0 {
        let inner = front - Vec2::splat(frame);
        ring(&mut b, (front, z1), (inner, z1), s2, 0.0);
        let deep = inner - Vec2::splat(recess * 0.6);
        ring(&mut b, (inner, z1), (deep, z1 - recess), s2, -1.0);
        let q = [
            Vec3::new(-deep.x, -deep.y, z1 - recess),
            Vec3::new(deep.x, -deep.y, z1 - recess),
            Vec3::new(deep.x, deep.y, z1 - recess),
            Vec3::new(-deep.x, deep.y, z1 - recess),
        ];
        b.quad(q, q.map(|p| [p.x / s2.x + 0.5, 0.5 - p.y / s2.y]), Vec3::Z);
    } else {
        let q = [
            Vec3::new(-front.x, -front.y, z1),
            Vec3::new(front.x, -front.y, z1),
            Vec3::new(front.x, front.y, z1),
            Vec3::new(-front.x, front.y, z1),
        ];
        b.quad(q, q.map(|p| [p.x / s2.x + 0.5, 0.5 - p.y / s2.y]), Vec3::Z);
    }
    b.build(true)
}

/// Länglicher Kristall (Doppelpyramide mit sechs Seiten).
pub fn crystal(radius: f32, height: f32) -> Mesh {
    let mut b = Builder::default();
    let top = Vec3::new(0.0, height * 0.5, 0.0);
    let bottom = Vec3::new(0.0, -height * 0.5, 0.0);
    let n = 6;
    let ring: Vec<Vec3> = (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            Vec3::new(a.cos() * radius, height * 0.12, a.sin() * radius)
        })
        .collect();
    for i in 0..n {
        let a = ring[i];
        let c = ring[(i + 1) % n];
        let mid = (a + c) * 0.5;
        b.tri([top, a, c], mid - Vec3::Y * height * 0.1);
        b.tri([bottom, c, a], mid - Vec3::Y * height);
    }
    b.build(false)
}

/// Facettierter Asteroid: verformte Ikosphäre mit flachen Normalen.
pub fn asteroid(seed: u32) -> Mesh {
    let mut mesh = Sphere::new(1.0).mesh().ico(2).expect("Ikosphäre");
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for p in pos.iter_mut() {
            let v = Vec3::from_array(*p);
            let n = fbm3(v * 1.6 + Vec3::splat(seed as f32 * 0.37), 4, seed);
            let k = 0.72 + n * 0.55;
            *p = (v * k * Vec3::new(1.0, 0.92, 0.85)).to_array();
        }
    }
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    let _ = mesh.generate_tangents();
    mesh
}

/// Flaches Rechteck in der XY-Ebene (zeigt zur Kamera).
pub fn quad(size: Vec2) -> Mesh {
    let mut b = Builder::default();
    let h = size * 0.5;
    let q = [
        Vec3::new(-h.x, -h.y, 0.0),
        Vec3::new(h.x, -h.y, 0.0),
        Vec3::new(h.x, h.y, 0.0),
        Vec3::new(-h.x, h.y, 0.0),
    ];
    b.quad(q, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], Vec3::Z);
    b.build(false)
}

/// Sternenschicht: viele kleine Quads in einem Mesh, mit Vertexfarben.
pub fn star_layer(stars: &[(Vec3, f32, [f32; 4])]) -> Mesh {
    let mut pos = Vec::with_capacity(stars.len() * 4);
    let mut nrm = Vec::with_capacity(stars.len() * 4);
    let mut uv = Vec::with_capacity(stars.len() * 4);
    let mut col = Vec::with_capacity(stars.len() * 4);
    let mut idx = Vec::with_capacity(stars.len() * 6);
    for (p, s, c) in stars {
        let base = pos.len() as u32;
        for (dx, dy, u, v) in [
            (-1.0, -1.0, 0.0, 1.0),
            (1.0, -1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0, 0.0),
            (-1.0, 1.0, 0.0, 0.0),
        ] {
            pos.push([p.x + dx * s, p.y + dy * s, p.z]);
            nrm.push([0.0, 0.0, 1.0]);
            uv.push([u, v]);
            col.push(*c);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
    .with_inserted_indices(Indices::U32(idx))
}

/// Kugel mit planarer UV-Projektion (für frontal betrachtete Planeten).
pub fn planet_sphere() -> Mesh {
    let mut mesh = Sphere::new(1.0).mesh().uv(96, 64);
    let uvs: Vec<[f32; 2]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) => pos
            .iter()
            .map(|p| [p[0] * 0.5 + 0.5, 0.5 - p[1] * 0.5])
            .collect(),
        _ => Vec::new(),
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

/// Konvexes Polygon (CCW) nach innen versetzen (Gehrung an den Ecken).
pub fn inset_convex(pts: &[Vec2], d: f32) -> Vec<Vec2> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let prev = pts[(i + n - 1) % n];
            let cur = pts[i];
            let next = pts[(i + 1) % n];
            let e1 = (cur - prev).normalize_or_zero();
            let e2 = (next - cur).normalize_or_zero();
            // Innennormalen eines CCW-Polygons zeigen nach links.
            let n1 = Vec2::new(-e1.y, e1.x);
            let n2 = Vec2::new(-e2.y, e2.x);
            let denom = (1.0 + n1.dot(n2)).max(0.15);
            cur + (n1 + n2) * (d / denom)
        })
        .collect()
}

/// Größter sinnvoller Versatz nach innen (grob: halber Inkreis).
fn safe_inset(pts: &[Vec2], want: f32) -> f32 {
    let c = pts.iter().copied().sum::<Vec2>() / pts.len() as f32;
    let n = pts.len();
    let mut min_d = f32::MAX;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let e = (b - a).normalize_or_zero();
        let nrm = Vec2::new(-e.y, e.x);
        min_d = min_d.min((c - a).dot(nrm).abs());
    }
    want.min(min_d * 0.35)
}

/// Abgeschrägtes Prisma aus einem konvexen Umriss (CCW): Seitenwände, Fase vorne und
/// optional ein Rahmen mit eingelassener Platte (wie bei den Stationsblöcken).
pub fn beveled_prism(outline: &[Vec2], depth: f32, bevel: f32, panel: Option<(f32, f32)>) -> Mesh {
    let mut b = Builder::default();
    let n = outline.len();
    let min = outline
        .iter()
        .copied()
        .fold(Vec2::splat(f32::MAX), Vec2::min);
    let max = outline
        .iter()
        .copied()
        .fold(Vec2::splat(f32::MIN), Vec2::max);
    let size = (max - min).max(Vec2::splat(0.01));
    let uvp = |p: Vec3| [(p.x - min.x) / size.x, 1.0 - (p.y - min.y) / size.y];
    let z_back = -depth * 0.5;
    let bevel = safe_inset(outline, bevel);
    let z0 = depth * 0.5 - bevel;
    let z1 = depth * 0.5;
    let centroid = outline.iter().copied().sum::<Vec2>() / n as f32;

    // Seitenwände
    for i in 0..n {
        let a = outline[i];
        let c = outline[(i + 1) % n];
        let e = c - a;
        let out = Vec3::new(e.y, -e.x, 0.0).normalize_or_zero();
        let len = e.length().max(0.01);
        let q = [
            a.extend(z_back),
            c.extend(z_back),
            c.extend(z0),
            a.extend(z0),
        ];
        let uv = [
            [0.0, 1.0],
            [len / size.x.max(size.y), 1.0],
            [len / size.x.max(size.y), 0.0],
            [0.0, 0.0],
        ];
        b.quad(q, uv, out);
    }
    // Ring zwischen zwei Umrissen; `tilt` gibt die Richtung der Normalen vor.
    let ring = |b: &mut Builder, outer: &[Vec2], oz: f32, inner: &[Vec2], iz: f32, tilt: f32| {
        for i in 0..n {
            let j = (i + 1) % n;
            let q = [
                outer[i].extend(oz),
                outer[j].extend(oz),
                inner[j].extend(iz),
                inner[i].extend(iz),
            ];
            let mid = (outer[i] + outer[j]) * 0.5;
            let side = (mid - centroid).normalize_or_zero().extend(0.0);
            b.quad(q, q.map(uvp), (side * tilt + Vec3::Z).normalize());
        }
    };
    let front = inset_convex(outline, bevel);
    ring(&mut b, outline, z0, &front, z1, 1.0);
    let face = |b: &mut Builder, pts: &[Vec2], z: f32| {
        for i in 1..pts.len() - 1 {
            let tri = [pts[0].extend(z), pts[i].extend(z), pts[i + 1].extend(z)];
            let base = b.pos.len() as u32;
            for p in tri {
                b.pos.push(p.to_array());
                b.nrm.push([0.0, 0.0, 1.0]);
                b.uv.push(uvp(p));
            }
            // CCW von vorne gesehen bleibt CCW (Umriss ist CCW).
            b.idx.extend_from_slice(&[base, base + 1, base + 2]);
        }
    };
    match panel {
        Some((frame, recess)) => {
            let fr = safe_inset(&front, frame);
            let inner = inset_convex(&front, fr);
            ring(&mut b, &front, z1, &inner, z1, 0.0);
            let deep = inset_convex(&inner, safe_inset(&inner, recess * 0.6));
            ring(&mut b, &inner, z1, &deep, z1 - recess, -1.0);
            face(&mut b, &deep, z1 - recess);
        }
        None => face(&mut b, &front, z1),
    }
    b.build(true)
}
