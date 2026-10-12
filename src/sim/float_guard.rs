//! Wächter: Die Simulation darf keine Floats enthalten (nur `fx.rs` als Rand zu Dateien, Anzeige
//! und Eingabe). Der Test liest den Quelltext von `src/sim` und prüft, dass nirgends `f32`/`f64`,
//! Float-Literale (außerhalb von `fx!(…)`), Bevy-Importe oder die Randfunktionen (`to_f32`,
//! `from_f32`, `to_bevy`, …) vorkommen. Zusätzlich verbietet `#![deny(clippy::float_arithmetic)]`
//! in `sim/mod.rs` jede Fließkommarechnung.

use std::fs;
use std::path::Path;

/// Nur Code behalten: Kommentare, Zeichenketten und Zeichenliterale durch Leerzeichen ersetzen.
fn code_only(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == '"' {
            i += 1;
            while i < b.len() && b[i] != '"' {
                i += if b[i] == '\\' { 2 } else { 1 };
            }
            i += 1;
            out.push_str("\"\"");
        } else if c == '\'' {
            // Zeichenliteral ('x', '\n') oder Lebensdauer ('a)
            let is_char = (b.get(i + 2) == Some(&'\'') && next != Some('\\'))
                || (next == Some('\\') && (i + 2..i + 6).any(|j| b.get(j) == Some(&'\'')));
            if is_char {
                i += 1;
                while i < b.len() && b[i] != '\'' {
                    i += if b[i] == '\\' { 2 } else { 1 };
                }
                i += 1;
                out.push_str("' '");
            } else {
                out.push(c);
                i += 1;
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Den Inhalt von `fx!( … )` entfernen (dort stehen die Dezimalzahlen, zur Compile-Zeit gewandelt).
fn without_fx_macros(code: &str) -> String {
    let mut out = String::new();
    let mut rest = code;
    while let Some(p) = rest.find("fx!(") {
        let before = &rest[..p];
        // Nur das echte Makro, nicht z. B. `to_fx!(`.
        let real = before
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        out.push_str(before);
        let body = &rest[p + 4..];
        if !real {
            out.push_str("fx!(");
            rest = body;
            continue;
        }
        let mut depth = 1;
        let mut end = body.len();
        for (k, ch) in body.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = k + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push_str("fx!()");
        rest = &body[end..];
    }
    out.push_str(rest);
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Verdächtige Stellen im Code (Zeilennummer, Text).
fn offences(code: &str) -> Vec<(usize, String)> {
    let mut bad = Vec::new();
    for (n, line) in code.lines().enumerate() {
        let ch: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < ch.len() {
            let c = ch[i];
            let prev = if i > 0 { Some(ch[i - 1]) } else { None };
            if c.is_ascii_digit() && !prev.is_some_and(|p| is_ident(p) || p == '.') {
                // Zahl einlesen
                let mut j = i;
                while j < ch.len() && (ch[j].is_ascii_digit() || ch[j] == '_') {
                    j += 1;
                }
                let mut float = false;
                if j + 1 < ch.len() && ch[j] == '.' && ch[j + 1].is_ascii_digit() {
                    float = true;
                    j += 1;
                    while j < ch.len() && (ch[j].is_ascii_digit() || ch[j] == '_') {
                        j += 1;
                    }
                }
                if j < ch.len() && (ch[j] == 'e' || ch[j] == 'E') {
                    let k = j + 1;
                    let k = if ch.get(k).is_some_and(|c| *c == '-' || *c == '+') {
                        k + 1
                    } else {
                        k
                    };
                    if ch.get(k).is_some_and(|c| c.is_ascii_digit()) {
                        float = true;
                    }
                }
                if float {
                    bad.push((n + 1, format!("Float-Literal: {}", line.trim())));
                }
                i = j.max(i + 1);
                continue;
            }
            i += 1;
        }
        for word in [
            "f32",
            "f64",
            "to_f32",
            "from_f32",
            "to_f64",
            "from_f64",
            "to_bevy",
            "from_bevy",
        ] {
            let mut from = 0;
            while let Some(p) = line[from..].find(word) {
                let a = from + p;
                let b = a + word.len();
                let before = line[..a].chars().next_back();
                let after = line[b..].chars().next();
                if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
                    bad.push((
                        n + 1,
                        format!("`{word}` in der Simulation: {}", line.trim()),
                    ));
                }
                from = b;
            }
        }
        if line.contains("bevy::") {
            bad.push((
                n + 1,
                format!("Bevy-Import in der Simulation: {}", line.trim()),
            ));
        }
    }
    bad
}

#[test]
fn no_floats_in_sim() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sim");
    let mut report = Vec::new();
    let mut files = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".rs") || name == "fx.rs" || name == "float_guard.rs" {
            continue;
        }
        files += 1;
        let src = fs::read_to_string(&path).unwrap();
        let code = without_fx_macros(&code_only(&src));
        for (line, what) in offences(&code) {
            report.push(format!("{name}:{line}: {what}"));
        }
    }
    assert!(
        files > 30,
        "Quelltext der Simulation nicht gefunden ({files} Dateien)"
    );
    assert!(
        report.is_empty(),
        "Floats in der Simulation:\n{}",
        report.join("\n")
    );
}

#[test]
fn the_guard_itself_catches_floats() {
    let bad = "let a = 0.5; let b: f32 = 1e-3; let c = x.to_f32(); use bevy::math::Vec2; let d = fx!(0.5) * 2.0;";
    let code = without_fx_macros(&code_only(bad));
    let found = offences(&code);
    assert!(found.len() >= 5, "{found:?}");
    // Erlaubtes bleibt unbeanstandet: fx!, Bereiche, Tupelfelder, Kommentare, Zeichenketten.
    let ok = "let a = fx!(0.5) + fx!(1e-3); for i in 0..5 {} let t = p.0.1; // 0.5 f32\n let s = \"1.5 f32\"; let r = 1..2;";
    let code = without_fx_macros(&code_only(ok));
    assert!(offences(&code).is_empty(), "{:?}", offences(&code));
}
