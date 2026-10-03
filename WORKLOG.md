# WORKLOG – DriftCrew

Laufendes Arbeitsprotokoll: Pläne, Entscheidungen, erledigte Schritte, offene Fragen.
Neueste Einträge stehen oben im jeweiligen Abschnitt.

Grundprinzipien, die bei jeder Änderung gelten (aus dem Konzept):

- **Slot-System:** Die Simulation kennt nur Slots (Triebwerke/Werkzeuge), keine Spieler.
- **Schiffe als Daten:** Teile, Formen, Layouts, Preise stehen in RON, nicht im Code.
- **Simulation getrennt und deterministisch:** feste 60 Hz, Eingabe = Slot-Bitmaske +
  Zielwinkel + Befehle, Zufall nur über den geseedeten RNG, keine Hash-Iteration.
  Alles, was das Spielgeschehen verändert, läuft als `Command` durch die Simulation
  (damit es später übers Netz geht). Reine Anzeige (HUD, Flugbahn-Vorschau, Seil-Optik)
  darf außerhalb liegen, liest den Simulationszustand aber nur.

---

## Plan: Änderungswünsche Runde 2 (32 Punkte)

Reihenfolge: erst Fixes, dann nach Nutzen/Aufwand. Phase 7 enthält die Punkte mit
größerer Architekturänderung – dort wird vorher nachgefragt.

### Phase 1 – UI-Fixes (klein, sofort spürbar)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 1 | Gleiche Tasten verschiedener Spieler unterscheidbar | Slot-Kästchen bekommen ein Spieler-Abzeichen (Spielerfarbe + Geräte-Kürzel ⌨ / Pad / Joy-Con) |
| 32 | Slot-Leiste: Rahmen im Ruhezustand, gefüllt nur beim Drücken | Rahmen in Slotfarbe, Füllung in Slotfarbe nur solange gedrückt, Text dann dunkel |
| 2 | Belohnung bricht nicht um, Wegmarken hinter dem Menü weg | Rechte Spalte `no_wrap` + `flex_shrink: 0`; Marker, die unter dem offenen Stationsmenü liegen, werden ausgeblendet |
| 4 | Andock-Assist als Ampel | Tempo, Winkel, Drehung je grün/gelb/rot (gelb = bis doppelter Grenzwert), Plattform-Leuchten folgt der Ampel |
| 5 | Zielen nur durch den Slot-Besitzer | ist bereits so verdrahtet; wird als reine Funktion herausgezogen und getestet, dazu ein Zielmarker pro Werkzeug in Slotfarbe |

### Phase 2 – Flughilfen und erwachsener Look (hoher Nutzen, mittlerer Aufwand)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 3 | Geschwindigkeitsvektor + Flugbahn-Vorschau | dünne Linien (Gizmos); Vorschau integriert den aktuellen Zustand ohne Eingabe ~4 s voraus, markiert den ersten Aufprallpunkt |
| 30 | Weniger Spielzeug-Look, Leuchten nur gezielt | gedecktere Lackierungen, Akzentstreifen als Lack statt Leuchtfarbe, dunkles Cockpitglas, schmalere Slot-Ringe; Leuchten nur Flammen, Positionslichter, Slotfarben |
| 31 | Schrägen, Fasen, Keile statt Würfel – auch bei Stationen | Teile bekommen eine Form in den Daten (`Box`, `Trapez`, `Keil` …); allgemeiner Mesh-Generator für abgeschrägte konvexe Prismen. Kollision folgt der Form (konvexe Polygone statt nur Rechtecke). Stationsraster bekommt Schrägen-Zeichen (`/ \ 7 r`) |
| 8 | Plattformen wie Druckplatten mit Details und Deko | Plattform mit Fase, Warnstreifen, Lauflichtern, Andockmarkierung; Deko-Generator für Kräne, Container, Antennen, Lichtmasten |

### Phase 3 – Kleine Systeme in der Simulation (mittel)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 15 | Schwerkraft nur bei Anomalien/Schwarzen Löchern | Planeten verlieren ihre Anziehung (Daten), neuer Typ „Schwarzes Loch“ mit stärkerem Sog und Ereignishorizont. **Konzept wird angepasst** |
| 7 | Landen auf Planeten | Landezonen als Daten (Winkel auf der Oberfläche); Andocken wie an Stationen; gelandet darf gebohrt werden |
| 26 | Rettungskapsel statt Game Over | Bei Hülle 0 wird eine Kapsel ausgestoßen (sichtbar, Kamera folgt), Bergungskosten aus der gemeinsamen Kasse (Text zeigt den Betrag) |
| 20 | Einzelne Triebwerke beschädigbar | Treffer nahe einem Triebwerk senken dessen Zustand: Stottern (geseedeter Zufall), Ausfall; Reparatur im Stationsservice; Anzeige an der Slot-Leiste |
| 22 | Treibstoff | Tank pro Schiff (Daten), Verbrauch pro Triebwerk; Tanken an Stationen; leer = Notreserve mit 25 % Schub, damit niemand festsitzt |
| 23 | Marktpreise je Station | Preisfaktoren pro Station in den Daten (Erz, Treibstoff, Service) |
| 9 | Mehrere Werften mit eigenem Angebot | Schiffe gibt es schon in vier Layouts; neu: Angebot pro Werft in den Daten + zweite Werft |
| 6 | Fracht am Kran schleppen, pendelnde Seilphysik | Schwere Container werden als Kiste am Kran geschleppt statt verstaut; Seil als harte Längenbeschränkung (straff/schlaff) statt weicher Feder; Seiloptik mit durchhängenden Segmenten |

### Phase 4 – Missionen und Fortschritt (mittel)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 10 | NPCs vergeben Missionen | `npcs.ron`: Name, Station, Rolle, Porträt-Seed, Sprüche; Aufträge haben einen Auftraggeber, Porträt im Stationsmenü |
| 11 | Missionstypen Abbau / Material verschicken / Notrufe | vorhanden; ergänzt um Lieferungen von Planeten-Außenposten und schwere Schlepp-Container (siehe 6) |
| 12 | Auswertung mit Spaßstatistik | Simulation zählt pro Slot Schubzeit, Kollisionen, Schüsse; Auswertungsfenster nach jedem Auftrag |
| 13 | Ruf pro Station | Ruf steigt mit erledigten Aufträgen; höhere Stufen = mehr/bessere Angebote, Rabatt |
| 18 | Fog of War | Orte und erkundete Gebiete werden gespeichert; Karte/Radar zeigen nur Entdecktes |
| 19 | Wracks ausschlachten | Wrackfelder in den Daten; Bohrer gewinnt Schrott, Kran reißt Bauteile ab; Verkauf am Markt |
| 27 | Lackierung und Flammenfarben | Lackiererei im Stationsmenü, Paletten für Rumpf/Akzent/Flammen, im Spielstand gespeichert |

### Phase 5 – Koop

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 24 | Ping pro Spieler | fester Ping-Knopf je Gerät (Tastatur: `^`-Taste oder mittlere Maustaste, Gamepad: Stick-Klick), Ping läuft als Befehl durch die Simulation, alle sehen Marker in Spielerfarbe |
| 25 | Hot-Join | Ein neues Gamepad, das während des Flugs eine Taste drückt, übernimmt den nächsten freien Slot; Umbau des Schiffs läuft als Befehl durch die Simulation |

### Phase 6 – Offene Welt und Ereignisse

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 14 | Sektoren mit Effekten | Regionen bekommen Effekte: Trümmer (Splitterzone), Nebel (Sicht + Radar gestört), Sonnenwind (seitliche Kraft) |
| 16 | Zufallsereignisse | geseedeter Ereignis-Takt: Meteoritenschauer in der Nähe, spontane Notsignale, Sonneneruption (Schild/Radar gestört) |

### Phase 7 – Größere Architekturänderungen (vorher nachfragen)

| Nr. | Punkt | Warum Rückfrage |
|---|---|---|
| 17 | NPC-Schiffe pendeln und docken | Die Simulation kennt heute genau ein Schiff. Für echte NPC-Schiffe (gleiche Physik, Andocken, Kollisionen) müsste sie mehrere Schiffe führen – das ist auch die Grundlage für Gegner und späteres Online-Spiel mit mehreren Crews |
| 21 | Gegner (Piratendrohnen, Schürfroboter) | hängt an derselben Entscheidung wie 17 |
| 28 | Trainingsmission | braucht Szenarien (eigene Welt-/Missionsdateien, Checkpoints) neben der offenen Welt |
| 29 | Zeit-Herausforderungen mit Bestenliste | baut auf den Szenarien aus 28 auf |

---

## Entscheidungen

- **Schwerkraft (Punkt 15):** Nur Anomalien und Schwarze Löcher ziehen an. Planeten haben keine
  Anziehung mehr (in Runde 1 hatte ich ihnen welche gegeben). Das Konzept „keine Schwerkraft im
  freien Raum“ wird damit konsequent; Planeten sind Hindernisse und Landeorte.
- **Treibstoff (Punkt 22):** Leerer Tank bedeutet nicht Stillstand, sondern Notreserve mit 25 %
  Schub. Grund: Ohne Reibung könnte eine Crew sonst endgültig festsitzen.
- **Bergung (Punkt 26):** fester Sockel (40 Cr) + 12 % der Kasse, höchstens die Kasse. Vorher
  15 % ohne Sockel – mit leerer Kasse war Zerstörung folgenlos, mit voller Kasse sehr teuer.
- **Kranseil (Punkt 6):** harte Längenbegrenzung statt Feder. Die Feder schwang nach und
  dehnte sich unter Last; mit fester Länge entsteht echtes Pendeln, und Masse am Seil zieht
  das Schiff spürbar herum.
- **Zielen (Punkt 5):** Werkzeuge zielen ausschließlich mit dem Gerät des Spielers, dem der Slot
  gehört (Tastatur → Maus, Gamepad → dessen Stick). Ein Spieler mit Tastatur zielt alle seine
  Werkzeuge gleichzeitig mit der Maus.

---

## Protokoll

### Runde 2 – Phase 4: Missionen und Fortschritt (erledigt)

- **10** Auftraggeber: neue Datei `npcs.ron` (Name, Rolle, Ort, vergebene Auftragsarten,
  Porträtfarben, Sprüche). Aufträge tragen ihren Auftraggeber (`Mission::giver`), das
  Stationsmenü zeigt im Reiter „Aufträge“ die Personen vor Ort mit Porträt (aus UI-Formen
  gebaut: Schultern, Kopf, Haare oder Helm) und einem Spruch.
- **11** Neue Variante „Material verschicken“: Planeten-Außenposten mit Auftraggeber bieten
  Erzladungen an (`Delivery { from: Owner::Planet }`), die Ladung liegt im Frachtraum und
  verschiebt Masse und Schwerpunkt. Abbau, Lieferung, Schwerlast und Notrufe gab es schon.
  `Mission::origin` ist dafür jetzt ein `Owner` (Station oder Planet).
- **12** Auswertung: `sim::stats` zählt pro Slot Schubzeit, Kollisionen (dem Teil zugeordnet,
  das dem Aufprall am nächsten war), Schüsse, abgebaute Tonnen und Kran-Griffe, dazu Dauer,
  Strecke, Höchsttempo und Schaden. Bei Annahme wird der Stand gemerkt, beim Abschluss entsteht
  ein `MissionReport` mit Auszeichnungen (Schubmeister, Sparfuchs, Bruchpilot, Scharfschütze,
  Bohrkönig, Greifarm). Das Fenster (`ui/report.rs`) zeigt Slotname und Spieler-Abzeichen,
  schließt sich nach 14 s oder per Klick (reine Anzeige, kein Befehl nötig).
- **13** Ruf pro Station (`Crew::reputation`, im Spielstand): +1 pro Auftrag, +2 für Schwerlast
  und Abschleppen; bei Notrufen und Außenposten-Lieferungen zählt die Zielstation. Stufen
  Neu/Bekannt/Geschätzt/Partner (0/3/7/12 Punkte): +1 Angebot und +10 % Belohnung je Stufe,
  größere Abbauaufträge, Schwerlast ab „Bekannt“, 5 % Service-Rabatt je Stufe. Anzeige im
  Stationsmenü mit Sternen und Punkten bis zur nächsten Stufe.
- **18** Fog of War: `sim::explore` – Raster mit 100-m-Zellen, alle 15 Ticks wird ein Kreis von
  380 m um das Schiff aufgedeckt. Im Spielstand als Hex-Bitfeld. Karte: weicher, runder Nebel
  als Textur (vierfach aufgelöst, bilinear überblendet), dazu nur entdeckte Planeten, Felder,
  Zonen, Anomalien und Wracks. Radar und Wegmarken genauso. Stationen mit `known: true` (alle
  außer Werft Vega) sind von Anfang an eingetragen. Aufträge und Notrufe bleiben immer sichtbar.
- **19** Wracks: `wrecks` in `world.ron` (Schiffsfriedhof im Nordwesten plus zwei Einzelwracks).
  Bohrer gewinnt **Schrott** (neue Erzsorte `Ore::Schrott`, am Markt handelbar). Der Kran reißt
  bei einem Ruck (Seilimpuls über `TEAR_IMPULSE`) ein **Bauteil** ab, das sofort am Haken hängt,
  eingeholt und am Markt zum festen Wert verkauft wird. Wracks treiben träge an ihren Platz
  zurück; glimmende Bruchstellen zeigen, dass noch Schrott drin ist.
- **27** Lackiererei: Reiter „Lack“ an Stationen mit Werkstatt. Rumpf und Akzent aus zehn
  gedeckten Farben, Flammen in sechs Varianten (Standard: Slotfarben). Kauf über die
  Abstimmung (`Purchase::Paint`), gespeichert pro Schiff. Eigene Flammenfarbe färbt nur die
  Außenflamme; Kern, Kennringe und Slot-Leiste bleiben in Slotfarbe. Menüeinträge haben jetzt
  optional ein Farbfeld (`Item::swatch`).
- Vorführszene `DRIFTCREW_SCENE=progress`.
- Tests: 50 grün (neu u. a. `station_offers_come_from_local_npcs`, `outpost_ships_ore_to_a_station`,
  `reputation_brings_more_offers_and_discounts`, `completed_mission_has_a_report_with_awards`,
  `collisions_are_counted_per_slot`, `wreck_gives_scrap_and_tears_parts`,
  `paint_and_exploration_survive_saving`, `reveal_and_roundtrip`, `awards_pick_the_extremes`).

### Runde 2 – Phase 3: Systeme in der Simulation (erledigt)

- **15** Schwerkraft nur an Anomalien und Schwarzen Löchern. `PlanetDef` hat kein
  `surface_gravity`/`influence` mehr, `World::gravity` summiert nur noch Anomalien. Neuer
  Anomalie-Typ `kind: BlackHole` (Schlund im Südwesten): Sog `strength · k² · (1 + 2·h/d)`, wird
  zum Horizont hin steiler; wer den Ereignishorizont (`core_radius`) berührt, verliert das Schiff,
  Körper werden verschluckt. Anzeige: roter gestrichelter Kreis „Kein Zurück“
  (`Anomaly::no_return_radius`, aus dem aktuell möglichen Schub inkl. Schäden und Notreserve),
  Warnung beim Eintritt in den Sog. Optik: schwarze Kugel, dünner Photonenring (Torus),
  schräge Akkretionsscheibe, dunkler Hof – bewusst ohne das Rot der Anomalie.
  **Konzept angepasst.**
- **7** Landezonen: `landing_zones` (Winkel) pro Planet. Jede Zone ist eine schmale Plattform
  (`Pad::zone`), daneben wird ein Erzvorkommen in Bohrreichweite gesetzt. Landen = Andocken,
  aber ohne Menü, und die Werkzeuge bleiben aktiv (Schiff steht still, Rückstoß wird von der
  Plattform aufgefangen). HUD: „Gelandet auf … · Werkzeuge frei“.
- **26** Rettungskapsel: Bei Hülle 0 wird `SimState::escape` ausgestoßen (Nase voraus,
  bremst langsam ab), die Kamera folgt ihr 5 s lang, dann steht das Schiff repariert an der
  Heimatstation (Tank mindestens halb voll, Triebwerke heil). Bergungskosten =
  `salvage_base` + `respawn_fee` · Kasse (höchstens die Kasse), Anzeige in Toast und
  Mittel-Text.
- **20** Triebwerksschäden: `Thruster::health`. Hüllenschaden nahe einem Triebwerk (2,2 m)
  senkt dessen Zustand. Unter 60 % stottert es – deterministisch aus Tick und Teil-Index in
  Fenstern von 5 Ticks (`tools::thruster_works`), kein Zugriff auf den Haupt-RNG –, bei 0 fällt
  es aus; Schub sinkt mit dem Zustand auf bis zu 70 %. Schäden bleiben beim Neuverteilen der
  Slots am jeweiligen Teil. Slot-Leiste zeigt „⚠ stottert“ / „✕ AUSFALL“, Service
  „Triebwerke instand setzen“ (Preis anteilig).
- **22** Treibstoff: `fuel_capacity`/`fuel_burn` pro Schiff, Verbrauch = Schub · Rate · Zeit pro
  feuerndem Triebwerk. Leer = Notreserve mit 25 % Schub (`EMERGENCY_THRUST`). Warnungen bei
  20 % und leer, HUD-Balken „TREIBSTOFF“, Service „Volltanken“ (anteilig, Ortsfaktor),
  Upgrade „Zusatztank“.
- **23** Preise je Ort: `prices: (ore: [(Erz, Faktor)], fuel, service)` für Stationen und
  Planeten-Außenposten. Markt zeigt eine Preistafel mit Hinweis, wo es mehr gibt
  (`best_ore_price`). Erze sind nahe ihrer Quelle billig und anderswo gefragt.
- **9** Werften mit eigenem Angebot: `ships_for_sale` pro Station (validiert). Werft Orion:
  Driftkutter, Kolibri, Lastesel. Neue **Werft Vega** im Osten: Driftkutter, Hornisse und das
  neue Schiff **Pelikan** (Bergungsschlepper mit Auslegern, 6 Triebwerke, 2 Kräne, Bohrer).
  Der Werft-Reiter nennt, was nur anderswo zu haben ist.
- **6** Kranseil als harte Längenbegrenzung statt Feder (`tools::rope_constraint`): schlaff,
  solange die Last näher ist als die Seillänge; straff wird per Impuls die Trennungs-
  geschwindigkeit aufgehoben (nur Zug, nie Druck). Daraus entsteht das Pendeln. Ein zu harter
  Ruck reißt das Seil. Schwere Lasten werden auf 8 m eingeholt. Angedockt hält das Schiff die
  Last als fester Anker. Neue Schwerlast-Aufträge (`towed: true` in `missions.ron`): eine Kiste
  (`BodyKind::Crate`) wird am Kran zur Zielstation geschleppt. Seiloptik: 12 Segmente, schlaff
  als durchhängende Parabel.
- Kleinere Fixes: Wegmarken am Bildrand stapeln sich statt sich zu überlagern und meiden die
  Balken unten links; Andock-Hinweis verschwindet unter dem offenen Stationsmenü; leere
  Hinweis-Pille ausgeblendet; Zerstört-Text zweizeilig und kleiner.
- Vorführszene `DRIFTCREW_SCENE=systems` (Schäden, Treibstoff, Schwarzes Loch, Kapsel,
  Landezone, Seil, Werft Vega, Karte).
- Tests: 41 grün (neu u. a. `planets_do_not_pull`, `black_hole_pulls_and_swallows_into_escape_pod`,
  `fuel_burns_and_emergency_reserve_keeps_you_moving`, `damaged_thruster_stutters_deterministically`,
  `gentle_landing_on_zone_and_drilling_while_landed`, `crane_rope_is_a_hard_length_limit`,
  `heavy_haul_is_towed_and_completes_at_target`, `ore_prices_differ_between_stations`).

### Runde 2 – Phase 2: Flughilfen und erwachsener Look (erledigt)

- **3** siehe Phase 1 (vorgezogen); Vektor auf 0,6 s gekürzt, damit er nicht wie ein Laser wirkt.
- **31** Formen statt Würfel: `PartShape` in den Schiffsdaten (`Box`, `Taper`, `Chamfer`, `Nose`,
  `Tail`, `Wing`). Kollision folgt der Form: `geom::Quad` wurde zu `geom::Poly` (konvex, bis
  8 Ecken) verallgemeinert, SAT/Clipping unverändert. Neuer Mesh-Generator
  `render::meshes::beveled_prism` (beliebiger konvexer Umriss, Fase, optional eingelassenes
  Paneel). Stationen: Schrägen-Zeichen `/ \ 7 r` im Raster und `auto_chamfer` (Standard an),
  das freiliegende Außenecken automatisch abschrägt – der Ring von Nova-Hub ist jetzt eine
  glatte Schräge statt einer Treppe.
- **30** Gedeckte Lackierungen (Stahlgrau/Rostorange, Sand/Blau, Anthrazit/Messing,
  Grau/Oliv), Akzentstreifen als Lack, getöntes Cockpitvisier statt Leuchtkuppel, schmale
  Slot-Ringe mit wenig Leuchtkraft, dunkler Bohrer, kleinere Positionslicht-Höfe, gedämpfte
  Stationsfenster. Leuchten bleibt bei Flammen, Positionslichtern und Slotfarben.
- **8** Landeplattformen als Druckplatten: dunkles Gehäuse + Platte in Warnfarbe mit Streifen,
  Lauflichter an der Vorderkante (Ruhe gedimmt, beim Anflug Lauflicht in Ampelfarbe, angedockt
  grün), Andockwinkel an den Enden, Lichtmasten, Arbeitslicht. Deko-Generator: Kräne
  (Mast, Ausleger, Gegengewicht, Seil, Haken) und Containerstapel an Plattformenden und auf
  freien Oberseiten, alles hinter der Spielebene (keine Kollision, kein Verdecken).

### Runde 2 – Phase 1: UI-Fixes (erledigt)

- **1** Slot-Leiste: Abzeichen in Spielerfarbe mit Geräte-Kürzel (`⌨ 1`, `JC-R 2`, `◉ 3`),
  sobald mehr als ein Crewmitglied an Bord ist. Gleiche Tasten (z. B. „A“ auf Tastatur und
  Joy-Con) sind so unterscheidbar. Spielerfarben: `input::PLAYER_COLORS`.
- **32** Slot-Leiste: Ruhezustand nur Rahmen in Slotfarbe, gedrückt komplett in Slotfarbe gefüllt,
  Schrift dann dunkel.
- **2** Menüeinträge: rechte Spalte (Preis/Belohnung) bricht nie um (`no_wrap`, `flex_shrink: 0`),
  der Titel nimmt den Restplatz und bricht stattdessen um. Wegmarken, die unter dem offenen
  Stationsmenü liegen würden, werden ausgeblendet (`station::menu_open`).
- **4** Andock-Ampel: Tempo, Winkel, Drehung je grün/gelb/rot (gelb bis zum doppelten Grenzwert,
  `sim::dock::light`), dazu Höhe über der Plattform und „← zur Mitte“-Hinweis. Die Plattform
  leuchtet in derselben Ampelfarbe.
- **5** Zielen: `input::aim_device` liefert ausschließlich das Gerät, dessen Taste den
  Werkzeug-Slot belegt (vorher fiel ein unbekannter Spieler auf die Maus zurück). Test
  `only_the_owner_aims_a_tool_slot`. Neu: gestrichelte Ziellinie mit Kreis pro Werkzeug in
  Slotfarbe (`render/overlay.rs`).
- Vorgezogen aus Phase 2: **3** Geschwindigkeitsvektor (Pfeil = Strecke in 1 s) und
  Flugbahn-Vorschau über 4 s ohne Eingabe, erster Aufprall als X (rot = schädlich).
  Berechnung `SimState::predict_path` liest nur, Test `prediction_finds_wall_ahead`.
- Tests: 27 grün.

### Runde 1 (abgeschlossen)

- Deterministische Simulation, Daten in RON, Grafik, UI, Lobby, Missionen, Kasse mit Abstimmung,
  Sound, Vorführmodus, 24 Tests. Details: README und Commit-Historie.
