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

## Plan: Änderungswünsche Runde 3 (Punkte 33–63)

Kam während Phase 4 dazu und wird mit den offenen Phasen 5–7 aus Runde 2 zusammengelegt.
Reihenfolge wie vereinbart: erst Anpassungen an Bestehendem, dann nach Nutzen/Aufwand, die
großen Architekturthemen nach Rückfrage (beantwortet, siehe „Entscheidungen“).
Abarbeitung: Phase 5 → 6 → 8 → 7 → 9 → 10 → 11 → 12 → 13.

Abgleich mit dem, was schon da ist:

- **51** Gespeichert wird schon beim Andocken und nach jedem Auftrag (zusätzlich bei Käufen,
  Verkäufen, Bergung und beim Beenden – bleibt so, schadet nicht). Der Spielstand enthält Kasse,
  Schiffe, Upgrades, Ruf, Lack und entdeckte Gebiete. „Online beim Host“ kommt mit dem Netzcode.
- **52** Die Hülle regeneriert schon nicht; Reparatur kostet. Neu: Reparaturdrohnen als Upgrade.
- **54** Bergung führt schon zur letzten angedockten Station (= letzter Savepoint), Kosten aus
  der Kasse (Punkt 26).
- **50** „Keine Bots in Slots“ gilt bereits. NPC-Schiffe (17, 21, 40, 44) sind eigene Weltobjekte
  mit eigener Steuerung, keine Mitspieler.

### Phase 5 – Koop und Crew

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 24 | Ping pro Spieler | wie geplant (Befehl durch die Simulation, Marker in Spielerfarbe) |
| 25 | Hot-Join | wie geplant |
| 50 | Empfohlene Crewgröße | `crew: (min, max)` pro Schiff und pro Auftragsvorlage in den Daten, Anzeige in Werft, Lobby und Auftragsliste |

### Phase 6 – Welt, Ereignisse, Erkunden

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 14 | Sektoren mit Effekten | wie geplant (Trümmer, Nebel, Sonnenwind) |
| 16 | Zufallsereignisse | wie geplant (Meteoritenschauer, Notsignale, Sonneneruption) |
| 37 | Scanner/Sonar | neues Werkzeug `Scanner` (Slot): Impuls mit Reichweite, zeigt Wracks, Erzvorkommen, Kapseln; Upgrade für Reichweite |
| 42 | Kartografie | Scans füllen Sektordaten; an Stationen verkaufbar (Preis nach neuem, unverkauftem Gebiet) |

### Phase 8 – Regeln, Aufträge, Atmosphäre (klein bis mittel)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 53 | Schild lädt nach | nach einigen Sekunden ohne Treffer, Rate pro Schiff in den Daten |
| 52 | Reparaturdrohnen | Upgrade: Hülle flickt sich im Flug langsam (nicht über ein Maximum hinaus) |
| 43 | Zeit- und Sauberkeitsbonus | Bonus, wenn schneller als Richtzeit und/oder ohne Kollision; in der Auswertung ausgewiesen |
| 39 | Passagiere | Auftrag mit Passagieren: Beschleunigungsspitzen und harte Stöße senken die Bezahlung (Zufriedenheit in der Auswertung) |
| 46 | Funk beim Andocken | kurze Funksprüche der Station/Auftraggeber beim Anflug und Andocken (Daten) |

### Phase 9 – Finanzen (erledigt, siehe Protokoll)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 47 | Dockgebühr, Versicherung, Kredit | Dockgebühr je Station (Daten, Ruf senkt sie), Versicherung als Abo pro Auftrag/Tag (übernimmt Teil der Bergung), Schiffskredit in der Werft mit Raten nach jedem Auftrag |
| 48 | Schwankende Preise | Angebot/Nachfrage je Station: Verkäufe drücken den Preis, er erholt sich langsam; deterministisch, im Spielstand |
| 49 | Crew-Abrechnung | Auswertung zeigt Einnahmen minus Kosten des Auftrags (Treibstoff, Reparatur, Gebühren); Rest geht in die Kasse |

### Phase 10 – Minispiele für einzelne Slots (erledigt, siehe Protokoll)

Laufen in der Simulation (deterministisch, nur Slot-Tasten als Eingabe), damit sie später auch
online funktionieren.

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 35 | Reparatur im Takt | Slot-Besitzer drückt im Takt einer Anzeige; Treffer flicken Hülle oder ein ausgefallenes Triebwerk |
| 36 | Präzisionsarbeit | Bohren: Ertrag hängt von ruhiger Zielhand ab (Zielwinkel-Schwankung), Kran: weiches Anheben gibt Bonus, Ruck kostet |
| 38 | Andockport hacken | kurzes Tastenmuster an Piraten-/Schmugglerstationen, Fehler lösen Alarm aus |

### Phase 11 – Upgrades, Modulbau, Schiffseditor (erledigt, siehe Protokoll)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 55–57 | Upgrades pro Teil mit Nachteil, Material- und Artefaktkosten | Upgrades hängen an Teilen (Triebwerk, Hülle, Schild, Fracht, Kran, Bohrer, Kanone, Scanner), ändern Masse und Schwerpunkt, kosten Credits + Material, starke Stufen + Artefakt; Kauf über Abstimmung |
| 33 | Modular erweitern (Raft-Prinzip) | Jeder Rumpf hat feste Bauplätze (Daten). Teile aus Material und Bauteilen craften und dort anbauen; Masse/Schwerpunkt/Trägheit rechnen sich wie bisher aus den Teilen |
| 34 | Schiffseditor | in der Werft: Bauplätze belegen und umbauen; die Belegung steht im Spielstand und lässt sich als RON-Datei exportieren. Neue Rümpfe nur über die Daten |

### Phase 12 – NPC-Schiffe (Architektur, Rückfrage)

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 17 | NPC-Schiffe pendeln und docken | Schiffe mit echter Physik und Autopilot |
| 21 | Gegner | Piratendrohnen, konkurrierende Schürfroboter |
| 40 | Eskorte und Konvoi | NPC-Frachter begleiten, Gegner abwehren |
| 41 | Schmuggel | Kontrollpunkte (Scans), Risiko, hoher Gewinn; Schmugglerstation mit Hack (38) |
| 44 | Wiederkehrende NPCs | Händler mit Spezialsortiment, Mechaniker (Triebwerkstuning), Rivalen-Crew mit eigenem Schiff, die dieselben Aufträge jagt |

### Phase 13 – Story, Lore, Artefakte, Monumente

| Nr. | Punkt | Umsetzung |
|---|---|---|
| 58 | Lore indirekt | Funkfetzen, Logbücher in Wracks, Stationsnamen: verlassene Kolonien, gescheitertes Terraforming, verstummte Stationen |
| 45, 59 | Roter Faden | Signal aus einer Anomalie; Kampagne als Auftragskette, Kapitel per Ruf und Artefakten; offenes Ende, Welt läuft weiter |
| 60, 61e | Logbuch | Texte, Funkmitschnitte, „X von Y gefunden“ ohne Orte |
| 61, 61a, 62, 63 | Artefakte | je Spielstand genau einmal, Ort per Seed beim Erzeugen festgelegt und gespeichert, kein Respawn; mit Masse und Nebenwirkung (stören Instrumente, ziehen Piraten an); Schlüssel für Kapitel und starke Upgrades |
| 61d | Monumente | große feste Strukturen (Tor, Signalturm), reagieren auf Artefakte, Scans und Kapitel |
| 61f | Artefakt bleibt im Wrack | bei Hülle 0 bleibt das Artefakt im Schiffswrack und muss geholt werden (abschaltbar) |

### Phase 7 – Training und Zeitrennen (erledigt, siehe Protokoll)

| Nr. | Punkt |
|---|---|
| 28 | Trainingsmission Drehen/Bremsen/Andocken |
| 29 | Zeit-Herausforderungen mit Bestenliste |
| 74 | Flugmanöver als Missionsziele (aus dem Backlog dazugenommen) |

---

## Backlog: Erweiterungen (Punkte 63–82)

Kam während Phase 8 dazu. Vorgabe: zuerst **65, 71 und 75** in kleinen spielbaren Varianten
(→ Phase 8b, direkt nach Phase 8), Energieverwaltung (70) und Wiederholungen (80) später.
Der Rest wird in die bestehenden Phasen einsortiert bzw. bekommt eigene Phasen.

| Nr. | Punkt | Einordnung |
|---|---|---|
| 63 | Artefakte als Schlüssel und Sammlung | gehört zu Phase 13 (Artefakte, Logbuch) – dort schon geplant, noch nicht gebaut |
| 64 | Fracht verschieben und arretieren | Phase 14 (Fracht und Bergung): Befestigungspunkte = Frachtmodule/Bauplätze, Umladen im Stationsmenü und im Flug (langsam), Schwerpunkt rechnet sich wie bisher |
| 65 | Sperrige Bergungsobjekte | **Phase 8b** |
| 66 | Zwei Kräne, eine Last; Seilbelastung sichtbar | Seilbelastung schon in Phase 8b (für 65/71), zwei Kräne an einer Last in Phase 14 |
| 67 | Notabwurf und Wiederaufnahme | Phase 14: Abwurf als Befehl, Fracht bleibt als Körper in der Welt, Position und Zustand im Spielstand |
| 68 | Fracht mit Flugeigenschaften | Phase 14: Tanks schwappen (gedämpfte Zusatzmasse), empfindliche Geräte (Stoßgrenze), instabile Funde (Überlast) – vor Annahme angezeigt |
| 69 | Triebwerks-Übersteuerung mit Hitze | Phase 15 (Zusammenarbeit): eigene Eingabe pro Gerät (z. B. Taste halten + Doppeltipp), Hitze pro Triebwerk, Warnstufen |
| 70 | Gemeinsame Energiereserve | später (nach 69), Grundfunktionen bleiben immer nutzbar |
| 71 | Präzisionsarbeit im Flug | **Phase 8b** (deckt auch einen Teil von 36 ab) |
| 72 | Physische Notfallreparatur mit dem Kran | **erledigt in Phase 10** (Ersatzteil am Kran) |
| 73 | Manöveransagen | Phase 15: kurze Signale („Bremsen“, „Schub aus“, „Links drehen“, „Werkzeug bereit“) in Slotfarbe, optionaler Ton, keine Rollen |
| 74 | Flugmanöver als Missionsziele | **erledigt in Phase 7** (Wrackring, Messflug, Lastaufnahme) |
| 75 | Stationen sichtbar wiederaufbauen | **Phase 8b** |
| 76 | Lokale Folgen von Aufträgen | Phase 16 (Welt reagiert) |
| 77 | Freiwillige Zusatzbergung | Phase 14 |
| 78 | Rettung mit Platz- und Gewichtsentscheidung, NPCs tauchen wieder auf | Phase 14 (Entscheidung) + Phase 12/13 (Wiederauftauchen) |
| 79 | Verborgene Wege durch Gefahrenzonen | Phase 16, Hinweise im Logbuch (Phase 13) |
| 80 | Unfall-Wiederholung | später: die Simulation ist deterministisch, also genügen Zustandsschnappschuss + Eingaben der letzten Sekunden |
| 81 | Schiffsname und Plaketten | Phase 17 (Persönlichkeit) |
| 82 | Crew-Logbuch mit Erlebnissen und Notizen | Phase 17, zusammen mit dem Logbuch aus Phase 13 |

## Entscheidungen

- **Runde 3, Rückfragen (beantwortet):**
  - NPC-Schiffe, Gegner, Eskorte, Rivalen (17, 21, 40, 41, 44): **volle Physik**. Die Simulation
    führt künftig eine Liste von Schiffen mit derselben Physik; NPCs steuern per Autopilot über
    echte Triebwerke. Grundlage auch für spätere Online-Crews.
  - Modulbau und Editor (33, 34): **feste Bauplätze** pro Rumpf (Daten). Der Werft-Editor belegt
    diese Plätze; kein freies Raster.
  - Material (33, 55–57): **gemeinsames Crew-Lager** wie die Kasse, an jeder Station befüllbar,
    in jeder Werft verbaubar.
  - Training und Zeitrennen (28, 29): **in der offenen Welt** mit Toren, Bestenliste pro Spielstand.

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

### Runde 3 – Phase 11: Crew-Lager, Bauplätze, Werft-Editor, Upgrades pro Teil (erledigt)

Wie in der Rückfrage entschieden: feste Bauplätze pro Rumpf, gemeinsames Crew-Lager.

- **Crew-Lager** (`sim/workshop.rs`): `Crew::storage` (t pro Erzsorte), `storage_parts`
  (Bauteile aus Wracks), `artifacts` (vorbereitet für Phase 13). `Command::StoreCargo` bringt an
  jeder Station Erz, Schrott und Bauteile aus dem Frachtraum ins Lager – was Abbau-Aufträge
  brauchen, bleibt an Bord. Alles im Spielstand.
- **Bauplätze (33):** `ShipDef::mounts` – feste Stellen am Rumpf mit erlaubten Modularten
  (Engine, Cargo, Tool, Armor), für alle fünf Schiffe. `modules.ron`: Zusatztriebwerk,
  Frachtmodul, Panzerplatte, Zusatzkran, Zusatzbohrer mit Masse, Form, Kosten (Credits, Material,
  Bauteile). `GameData::built_ship` setzt Grundschiff und Module zu einem normalen `ShipDef`
  zusammen (ein Modul = ein Teil mit `fixed: true`), `SimState::current_def` liefert das für das
  aktuelle Schiff. Schiffsbau, Lobby, Umbau und Schiffswechsel nutzen jetzt diesen Bauplan –
  deshalb erscheinen Modul-Triebwerke und -Werkzeuge als eigene Slots, und Masse, Schwerpunkt
  und Trägheit folgen wie immer aus den Teilen.
- **Feste Triebwerke:** `thruster_layouts` ordnet nur die Grundtriebwerke symmetrisch an;
  angebaute (feste) Triebwerke behalten ihre Position und hängen in der Slot-Reihenfolge hinten
  an. Ein Triebwerk am Heck links dreht das Schiff, wenn es allein schiebt – Absicht.
- **Werft-Editor (34):** Reiter „Bau“ in Werften. Übersicht der Bauplätze (belegt/leer, was
  passt), Auswahl eines Bauplatzes → Module mit Kosten und Wirkung, Abbauen (die Hälfte des
  Materials zurück). Kopfzeilen zeigen das Lager und Masse/Schwerpunkt/Trägheit/Hülle/Fracht des
  Schiffs. Am Schiff markiert das Overlay die Bauplätze (leer gestrichelt, belegt doppelt,
  gewählt pulsierend). Neue oder entfernte Slots lösen `ShipChanged` aus → Lobby zum
  Neuverteilen, Fracht- und Panzermodule nicht. „Bauplan exportieren“ schreibt den
  zusammengesetzten `ShipDef` als RON nach `bauplaene/<schiff>_umbau.ron` neben dem Spielstand
  (dafür sind die Schiffsdatentypen jetzt auch serialisierbar).
- **Upgrades pro Teil (55–57):** `UpgradeDef` hat jetzt `part` (Triebwerke, Hülle, Schild,
  Fracht, Kran, Bohrer, Kanone, Scanner, Tank, Bordsysteme), `tier`, `mass`, `drawback`,
  `materials`, `parts`, `artifact`. Neue Wirkungen: Treibstoffverbrauch, Feuerrate und Schaden
  der Kanone, Tragkraft des Krans (Seilgrenze). Nachteil: die Masse sitzt am betroffenen Teil
  (je Triebwerk, je Frachtmodul, je Werkzeug; Hülle/Schild/Systeme im Rumpf; der Tank verteilt
  sich auf die Triebwerke am Heck und zieht den Schwerpunkt nach hinten). Stufe 3 braucht ein
  Artefakt, das dabei verbraucht wird. Upgrades bleiben crew-weit (wandern beim Schiffswechsel
  mit) – vereinfachte Entscheidung, damit alte Spielstände ohne Umbau weiterlaufen; Module
  gehören dagegen zum Rumpf.
- **Nebenbei:** Die Upgrade-Liste zeigt Teil, Stufe, Wirkung, Nachteil und Kosten. Bestehende
  Tests, die Upgrades kaufen, füllen jetzt das Crew-Lager auf.
- Vorführszene `DRIFTCREW_SCENE=workshop`.
- Tests: 101 grün, neu in `workshop_tests.rs`: Einlagern (Auftragserz bleibt, Spielstand),
  Frachtmodul (ohne Material nicht, Schwerpunkt wandert, Kosten, keine Lobby, Spielstand),
  Triebwerksmodul (eigener Slot, feste Position, Grundlayout bleibt, dreht allein das Schiff),
  Passprüfung und Abbau mit halber Rückgabe, nur in Werften, Upgrade-Masse und Material (Panzerung
  schwerer und träger, Tank verschiebt den Schwerpunkt, Feuerrate, Tragkraft), Stufe 3 braucht
  und verbraucht ein Artefakt, exportierter Bauplan ist wieder als `ShipDef` lesbar.

### Runde 3 – Phase 10: Minispiele für einzelne Slots (erledigt)

Alle Spiele laufen in der Simulation (`sim/minigame.rs`), Eingabe sind nur Slot-Tasten und
Zielwinkel – deterministisch wie der Rest. Gehört eine Taste gerade einem Minispiel, bekommt die
Steuerung in diesem Tick eine leere Eingabe (das Schiff treibt einfach weiter).

- **Takt:** ein fester Schlag alle 0,7 s (aus der Simulationszeit), ±0,14 s zählen als Treffer.
  Die Leiste unten zeigt eine wandernde Marke und die grünen Zonen an den Enden, darunter die
  letzten Tastendrücke in Slotfarbe (✓/✕).
- **35 Triebwerk im Takt flicken:** Ein ausgefallenes Triebwerk hat eine tote Taste – genau die
  wird zum Reparaturknopf. Acht Treffer (Fehlgriffe kosten einen halben) setzen den Zustand auf
  0,45: es läuft wieder, stottert aber. Geht im Flug, die anderen steuern weiter.
- **35 Notreparatur der Hülle:** Hülle unter 75 %, Schiff ruhig (unter 0,6 m/s, kaum Drehung)
  und 3 s lang keine Taste gedrückt → Notreparatur. Dann ist jede Slot-Taste ein Hammer: Treffer
  +2 Hülle, höchstens bis 75 % („den Rest macht die Station“); ausgefallene Triebwerke lassen
  sich dabei mit ihrer eigenen Taste flicken. Eine Taste 0,45 s halten beendet die Notreparatur
  und wirkt sofort normal – so kommt niemand in einer Lage fest, in der Tippen nicht schiebt.
  Entscheidung: Auslösen durch Ruhe statt durch eine eigene Taste, weil alle festen Tasten schon
  vergeben sind und jede freie Taste ein Slot sein kann.
- **36 Präzisionsarbeit:** `Tool::jitter` misst, wie schnell sich der Zielwinkel dreht
  (geglättet). Der Bohrertrag hängt davon ab: ruhig ×1,25, zittrig bis ×0,7
  (`steady_factor`); die Slot-Leiste zeigt „ruhige Hand ▲“ bzw. „zittrig ▼“. Beim Kran zählt
  `Mission::max_strain` die höchste Seilbelastung, solange die Last des Auftrags (Schwerlast,
  Bergung, Abschleppen) am Haken hing; unter 55 % gibt es den Bonus „Last sanft geführt“
  (10 %), sonst steht in der Auswertung, wie hoch das Seil belastet war. Zusammen mit
  Erzadern und Wrackbolzen aus Phase 8b ist Punkt 36 damit abgedeckt.
- **38 Andockport hacken:** neue Station **Nebelhafen** (Schmugglernest im Schleiernebel, erst
  nach Entdeckung auf der Karte, Hehlerin Vex Moreau, hohe Preise für Ionit/Solarit/Schrott,
  keine Dockgebühr). `hack` in `world.ron`: Ohne Hack bietet der Port keine Plattform an. In
  30 m Nähe (langsamer als 5 m/s) beginnt der Hack: ein Muster aus fünf belegten Slots, nie
  derselbe zweimal hintereinander, 9 s Zeit. Jede Person drückt ihren Slot in der Reihenfolge.
  Fehler oder Zeit um → Störimpuls (Schild auf 0, Nachladen beginnt neu), Port 30 s gesperrt.
  Geschafft → Port 10 Minuten offen. Wegfliegen bricht ohne Alarm ab.
- **72 Ersatzteil mit dem Kran:** `CraneState::Patching`. Liegt ein Bauteil (aus einem Wrack)
  im Frachtraum und zielt der Kran beim Drücken auf ein ausgefallenes Triebwerk (±15°, in
  Reichweite), fährt er das Teil dorthin statt auszufahren. Taste halten, Ziel halten, nicht
  kreiseln: nach 2,5 s ist das Teil verbaut und das Triebwerk läuft eingeschränkt (0,45).
  Abweichen lässt den Fortschritt wieder sinken, Loslassen bricht ab. Im Bild: rote Ringe an
  ausgefallenen Triebwerken, grüner Fortschrittsbogen beim Einsetzen.
- Töne: Treffer im Takt (Blip), daneben (Klick), geflickt/Port offen (Klonk), Alarm über die
  rote Meldung.
- Vorführszene `DRIFTCREW_SCENE=minigames`.
- Tests: 94 grün, neu in `minigame_tests.rs`: Triebwerk im Takt (daneben zählt nicht, Taste
  schiebt nicht, nach acht Treffern geflickt), Notreparatur (Ruhe startet sie, Treffer flicken,
  Tippen schiebt nicht, Halten beendet, Obergrenze 75 %), Hack (Port gesperrt bis geknackt,
  Muster ohne Doppel, Tasten steuern nicht), falsche Taste → Alarm und Sperre, Zeit abgelaufen,
  Ersatzteil am Kran (ohne Teil normaler Kran), ruhige Hand bohrt mehr, Bonus für sanfte Last.

### Runde 3 – Phase 9: Finanzen (erledigt)

- **47 Dockgebühr:** `finance.dock_fee` (12 Cr) × Ortsfaktor `prices.dock` (Nova 1,0, Kepler 1,5,
  Orion 0,75, Vega 1,25, Relais 0 – wer beim Wiederaufbau hilft, zahlt nicht), minus 20 % pro
  Rufstufe. Abgebucht beim Andocken an einer Station (nicht beim Start, nicht auf Planeten).
  Wer innerhalb von 120 s wieder an derselben Station andockt, zahlt nicht noch einmal – sonst
  würden Anflugversuche und die Dockprüfung teuer. Die Gebühr steht im Stationsmenü neben dem Ruf.
- **47 Versicherung:** `Purchase::Insurance(bool)`, läuft über die Abstimmung wie jeder Kauf.
  Kein Einstiegspreis, sondern ein Abo: 8 % jeder Auftragsbelohnung (mindestens 10 Cr) gehen bei
  der Abrechnung ab. Dafür übernimmt sie 70 % der Bergungskosten (`salvage_fee_now` rechnet die
  Versicherung schon ein, `salvage_fee_gross` ist der Betrag davor).
- **47 Schiffskredit:** `Purchase::ShipOnCredit` in der Werft, wenn das Schiff bar zu teuer ist:
  25 % Anzahlung, der Rest plus 12 % Zinsen in 8 Raten. Die Rate geht nach jedem erledigten
  Auftrag von den Einnahmen ab – nie mehr, als der Auftrag gebracht hat, damit die Kasse nicht
  ins Minus rutscht. Nur ein Kredit zugleich; `Purchase::RepayLoan` tilgt den Rest in einer
  Werft. Offener Kredit steht in der Karte.
- **48 Schwankende Preise** (`sim/finance.rs`): ein Preisfaktor pro Ort (Stationen, dann
  Planeten) und Erzsorte. Jede verkaufte Tonne senkt ihn um 3,5 % (nicht unter 55 %), er erholt
  sich exponentiell mit 150 s Zeitkonstante. Alle 4–7 Minuten (Simulationszeit, Seed-RNG) sucht
  ein Ort ein Erz: Faktor steigt in ~20 s Richtung 135 % und hält 5 Minuten. Markt-Reiter zeigt
  Trend (▼/▲ in 5-%-Schritten) und die laufende Nachfrage. Faktoren und Nachfrage stehen im
  Spielstand (`CrewSave::market`, `demand`).
- **49 Crew-Abrechnung:** `CrewStats::expenses` zählt Dockgebühren, Service-Käufe und
  Bergungskosten mit; die Auswertung nimmt wie bei der Spaßstatistik die Differenz seit
  Auftragsannahme. Am Auftragsende: Einnahmen (Grundbelohnung + Boni) − Versicherung − Kreditrate
  = was in die Kasse geht. Darunter „Unterwegs: Dock · Service · Bergung“ und der **Gewinn**
  (Einnahmen − Prämie − unterwegs bezahlt; die Kreditrate zählt nicht als Kosten, sie zahlt das
  Schiff ab).
- **Nebenbei behoben:** Das Stationsmenü scrollte bei Tastatur/Gamepad nicht mit – Einträge
  unterhalb des Rands waren nur mit der Maus erreichbar. Jetzt hält `ScrollList` den gewählten
  Eintrag im Bild (Lage aus den Höhen der Einträge davor, weil die Bildschirmpositionen dem
  Scrollwert einen Frame hinterherhinken), und die Position überlebt einen Neuaufbau der Liste.
  Beim Andocken an einer anderen Station beginnt das Menü beim ersten Reiter.
- Vorführszene `DRIFTCREW_SCENE=finance`.
- Tests: 86 grün, neu in `finance_tests.rs`: Dockgebühr (Ort, Ruf, kurze Wiederkehr, Relais),
  Versicherung (Prämie, Bergung, Kündigung), Kredit (Anzahlung, Raten, zweiter Kredit, Tilgen
  nur in der Werft), Rate höchstens so hoch wie die Einnahmen, Markt (Verkauf drückt, Erholung,
  Nachfrage, Spielstand), Nachfrage deterministisch, Kosten unterwegs in der Auswertung.
  `ore_prices_differ_between_stations` prüft jetzt auch, dass der Verkauf den Preis drückt.

### Runde 3 – Phase 7: Training, Zeitrennen, Flugmanöver als Auftragsziel (erledigt)

Wie in der Rückfrage entschieden: alles in der offenen Welt, Bestenliste pro Spielstand.

- **Parcours als Daten** (`assets/data/courses.ron`, `sim/course.rs`): ein Parcours ist eine Folge
  von Schritten – `Gate` (Tor mit Flugrichtung und Breite), `Pass` (Punkt erreichen), `Face` (Nase
  auf eine Boje richten und halten), `Hold` (im Feld zur Ruhe kommen und stillhalten), `Dock`
  (andocken, optional an einer bestimmten Plattform). Jeder Schritt kann einen Hinweistext haben.
  Die Prüfung läuft in der Simulation auf dem Schiffszustand: Tor = Flugstrecke des Ticks
  schneidet die Torlinie in Pfeilrichtung; `Pass` prüft die Strecke, damit schnelle Schiffe nicht
  durchrutschen; Halte-Schritte bauen Fortschritt auf und verlieren ihn doppelt so schnell wieder.
- **Start in der offenen Welt:** Ein Lauf beginnt beim Durchfliegen eines Starttors – aber nur,
  wenn die Flugrichtung höchstens ~30° vom Pfeil abweicht. Sonst würde jeder Vorbeiflug an Nova
  einen Lauf starten. Alternativ im Stationsmenü wählen (`Command::StartCourse`): Der Parcours
  ist dann „bereit“, die Zeit läuft erst am Starttor. Durch das eigene Starttor fliegen startet
  neu. Abbruch: mehr als 450 m vom nächsten Ziel, andocken außerhalb eines `Dock`-Schritts,
  falsche Plattform, Zerstörung, 15 Minuten, oder „Parcours abbrechen“ im Pausemenü bzw.
  Stationsmenü (`Command::AbortCourse`).
- **Wertung:** Flugzeit + Strafzeit. Jede Kollision 2 s; beim Präzisionsandocken zusätzlich
  1,5 s pro Meter Versatz zur Plattformmitte und 1 s pro m/s Aufsetzgeschwindigkeit. Gemessen wird
  im Tick des Aufsetzens, bevor das Schiff auf die Plattform gezogen wird; die Geschwindigkeit
  stammt aus dem Tick davor (`prev_vel`) – deshalb läuft `update_course` vor `update_tracking`.
- **Medaillen und Bestenliste:** Bronze/Silber/Gold pro Zeitrennen, die Prämie (60/120/250) gibt
  es je Medaille einmal pro Spielstand; wer gleich Gold holt, bekommt alle drei. Die Bestenliste
  (`CrewSave::records`) hält die fünf besten Läufe mit Zeit, Strafzeit, Schiff, Crewgröße und
  Laufnummer, dazu die beste Medaille und die Zahl der Abschlüsse.
- **28 Grundkurs** (Training vor Nova): Starttor → Boje anpeilen und halten (Drehen) → Schub durch
  ein Tor → im Feld vor der Boje stillstehen (Bremsen) → an Nova andocken. Boje, Tor und Feld
  liegen auf einer Linie, damit „Nase auf die Boje“ zugleich die Flugrichtung für die nächsten
  Schritte ist. 150 Credits Ausbildungszuschuss beim ersten Abschluss.
- **29 Zeitrennen:** *Nova-Ring* (um den Hub und mitten durch den Ring), *Dockprüfung*
  (Präzisionsandocken auf der oberen rechten Plattform im Ring), *Wrackring* (siehe unten).
- **74 Flugmanöver als Auftragsziel** – drei Varianten, alle mit derselben Physik:
  - **Rotierende Wracköffnung:** Rotoren können jetzt auch ein Ring mit Öffnungen sein
    (`SpinnerShape::Ring`, Segmente als kinematische Kollider wie die Rotorarme). Der
    **Wrackring** im Schiffsfriedhof dreht sich mit 18°/s und hat zwei Öffnungen. Er ist Teil
    des Zeitrennens und ein Messfeld.
  - **Messflug** (`MissionKind::Survey`, Auftraggeber Ivo, Selin, Ines): ein oder zwei Messfelder
    anfliegen und dort 6 s stillhalten (unter 0,6 m/s, kaum Drehung). Die Felder liegen dort, wo
    Stillhalten schwer ist: Anomalierand (Sog), Sonnenwind-Korridor, Schleiernebel, Splitterzone,
    Kobaltschwarm, Wrackring – und eine leichte Ruhezone bei Nova.
  - **Lastaufnahme** (`socket` in `world.ron`, Kepler und Vega): U-förmige Halterung aus drei
    statischen Kollidern. Schwerlast-Aufträge zu diesen Stationen sind erst erfüllt, wenn die
    Kiste ruhig (unter 0,5 m/s) im Inneren liegt, nicht mehr am Kran hängt und das 1 s lang.
    Andere Ziele behalten die alte Regel (Nähe der Station). Einflugpfeile und ein Ring für den
    Fortschritt zeigen, was gemeint ist.
- **Anzeige:** Torpfosten mit Leuchtkappe (Starttore grün, nächstes Ziel hell, übernächstes
  gedämpft), Torlinien mit wandernden Pfeilen, Bojen mit Peillinie von der Schiffsnase, Felder
  mit Fortschrittsring, die Zielplattform eingerahmt. Oben Mitte: Parcours, Schritt, nächste
  erreichbare Medaille, Zeit, Strafzeit, Hinweis, Fortschrittsbalken; Meldungen rücken darunter.
  Nach dem Ziel eine Tafel mit Wertung, Medaille, Prämie, nächster Medaille und Bestenliste
  (eigener Lauf hervorgehoben). Stationsreiter **Parcours** mit Medaillenzeiten und den drei
  besten Läufen. Karte: Starttore als Fähnchen (bei Nova zusammengefasst). Starttore im Bild
  werden beschriftet, das laufende Ziel bekommt einen Randpfeil.
- Vorführszene `DRIFTCREW_SCENE=courses`.
- Tests: 79 grün, neu in `course_tests.rs`: Starttor nur in Pfeilrichtung, Rennen mit
  Bestenliste/Medaillen/Speichern+Laden, Strafzeit, Grundkurs Schritt für Schritt,
  Präzisionsandocken (Versatz, Aufsetzgeschwindigkeit, falsche Plattform), Abbruch,
  Wrackring-Öffnungen, Messflug, Messflug-Angebote, Lastaufnahme (nur abgesetzt und gelöst),
  Wände der Lastaufnahme, alle Tore und Felder liegen frei. Der bestehende Schwerlast-Test liefert
  jetzt zur Werft Orion (ohne Lastaufnahme).

### Backlog – Phase 8b: Sperrige Bergung, Präzisionsarbeit, Wiederaufbau (erledigt)

Kleine spielbare Varianten wie vorgegeben.

- **65** Sperrige Bergungsobjekte: Körper können jetzt eine zusammengesetzte Form haben
  (`Body::circles`, Kette aus Kreisen entlang der Längsachse) – Kollisionen mit Stationen,
  Schiff, anderen Körpern und Geschossen laufen über alle Kreise, Stöße an einem Ende drehen den
  Körper. Neuer Körper `BodyKind::Bulky` (Antennenmast, Stationsring-Segment, Rumpfplatte,
  Sonnensegel-Träger in `missions.ron`), neuer Auftrag „Bergung“ (Mara, Kofi, Yara): Objekt am
  Fundort, muss außen am Kran in die **Ablagezone** der Station (`drop_zone` in `world.ron`;
  bei Nova-Hub im Ring, erreichbar nur durch die Seitenöffnungen – die Engstelle). Der Kran greift
  dort, wo er trifft (`CraneState::Attached { local }`), das Seil wirkt am Angriffspunkt mit
  Hebel: am Ende gegriffen pendelt und dreht ein langer Mast deutlich. Machbarkeit
  (`bulky_feasible`): Kran belegt und mit Last noch mindestens 1,6 m/s² Beschleunigung, sonst ist
  der Auftrag ausgegraut mit Begründung.
- **66 (Teil)** Seilbelastung sichtbar: `Tool::strain` (geglättet, schnell hoch, langsam runter),
  Seilfarbe Slotfarbe → Gelb → Rot und heller, Warnung kurz vor dem Reißen. Zwei Kräne an einer
  Last folgen in Phase 14.
- **71** Präzisionsarbeit (`sim/precision.rs`): 20 % der großen erzhaltigen Asteroiden haben
  eine **Erzader**, Wracks pro verbleibendem Bauteil einen **Verbindungsbolzen** an einer festen,
  mitdrehenden Stelle (aus dem Seed). Der Bohrpunkt muss 4 s (Ader) bzw. 3 s (Bolzen) innerhalb
  von 0,7 m bleiben; abgerutscht bröckelt der Fortschritt. Ergebnis: Kristallkern (wertvolles
  Bauteil) bzw. ein gelöstes Wrackteil. Markierungen im Bild (gold/türkis), Fortschritt und
  Abweichung unten Mitte für alle.
- **75** Wiederaufbau (`sim/project.rs`): neue Station **Relais Ost** (verstummt). Raster-Zeichen
  `1`–`3` sind Blöcke, `a`–`c` Plattformen, die es erst ab dieser Etappe gibt – ihre Kollider
  und Plattformen sind vorher abgeschaltet (`StaticCollider::enabled`, `Pad::enabled`). Drei
  Etappen (Notstrom/Licht → Andockflügel/Werkstatt → Sendemast) mit Material und Bauteilen,
  Abgabe als `Command::DeliverProject`, Teillieferungen möglich. Nach jeder Etappe: Fenster und
  Leuchtfeuer an (Etappe 1), neuer Flügel mit Plattform (2), Mast (3), dazu Dienste, Bezahlung,
  +3 Ruf, Meldung mit der Wirkung. Fortschritt im Spielstand (`projects`), neue
  Auftraggeberin Ines Varga ab Etappe 2.
- Vorführszene `DRIFTCREW_SCENE=rebuild`.
- Tests: 67 grün (neu `relay_station_is_rebuilt_in_stages`, `rod_collides_along_its_length`,
  `rod_grabbed_at_the_end_swings`, `bulky_salvage_needs_the_drop_zone_and_a_capable_ship`,
  `precision_drilling_frees_a_vein_only_when_held_on_target`).

### Runde 3 – Phase 8: Regeln, Aufträge, Atmosphäre (erledigt)

- **53** Schild lädt nach: `shield_regen`/`shield_delay` pro Schiff (Hornisse schnell, Lastesel
  langsam). Jeder Treffer setzt den Zähler zurück, während einer Sonneneruption lädt nichts.
  Beschädigte Triebwerke brauchen weiter Reparatur (oder später das Minispiel).
- **52** Hülle regeneriert nie von selbst. Neues Upgrade „Reparaturdrohnen“ flickt 0,5 Punkte
  pro Sekunde, auch im Flug.
- **43** Richtzeit pro Auftrag (`Mission::par`, aus Strecke und Art berechnet, im Menü
  angezeigt). Abrechnung: Grundbelohnung + Zeitbonus (15 %, wenn unter Richtzeit) +
  Sauberkeitsbonus (10 %, ohne Kollision und Schaden). Die Auswertung schlüsselt das auf.
- **39** Passagiere (`MissionKind::Passengers`, neuer Auftraggeber Lio Tanaka, Fährdienst, und
  Yara in Vega): Zufriedenheit sinkt bei Beschleunigung über 10 m/s², bei Stößen (−5 % pro
  Kollision) und bei schnellem Kreiseln. Bezahlt wird 30 % + 70 % × Zufriedenheit. Die
  HUD-Zeile zeigt die Zufriedenheit live.
- **46** Funk (`radio.ron`, `ui/radio.rs`): Sprüche beim Anflug, Andocken und Abdocken,
  allgemein und pro Ort, mit leisen Lore-Fetzen (Terraforming-Masten, ein Signal aus der Tiefe,
  eine Kolonie für zehntausend mit elf Leuten). Reine Anzeige, Panel links über den Balken.
- Vorführszene `DRIFTCREW_SCENE=rules`.
- Tests: 62 grün (neu `shield_recharges_but_hull_needs_drones`,
  `passengers_pay_by_comfort_and_bonuses_apply`, `every_offer_has_a_par_time`; der
  Schwerlast-Test prüft jetzt auch die Boni).

### Runde 3 – Phase 6: Welt, Ereignisse, Erkunden (erledigt)

- **14** Sektoren mit Effekten: `effect` an Regionen in `world.ron`, wirkt mit weichem Rand
  (voll bis 70 % des Radius, `SimState::sector_at`). **Trümmer** (Splitterzone,
  Schiffsfriedhof): die Simulation hält um das Schiff herum treibende Schrottteile
  (`BodyKind::Debris`, echte Körper mit Kollision), dazu reine Anzeige-Splitter hinter der
  Spielebene, die fest im Weltraster liegen. **Nebel** (neuer Schleiernebel): Bildschirm getönt,
  Radar sieht nur noch die Nähe und rauscht, Erkundungs- und Scannerreichweite sinken.
  **Sonnenwind** (neuer Sonnenwind-Korridor): seitliche Beschleunigung auf Schiff und lose
  Körper, Schlieren in Windrichtung. HUD-Zeile nennt den Effekt, die Karte beschriftet die
  Sektoren, sobald sie entdeckt sind.
- **16** Zufallsereignisse (`events` in `world.ron`, nur im Flug, Takt per geseedetem RNG):
  **Meteoritenschauer** mit 3 s Vorwarnung und Himmelsrichtung, Meteore zielen grob aufs Schiff;
  **spontanes Notsignal** 260–480 m entfernt, 30 % besser bezahlt, auf der Karte annehmbar;
  **Sonneneruption** (Schild auf 40 %, Radar und Scanner 20 s gestört, Bildschirm orange).
- **37** Scanner als neues Werkzeug (`ToolKind::Scanner`, eigener Slot) an Driftkutter, Kolibri,
  Lastesel und Pelikan: Sonar-Impuls läuft mit 320 m/s bis zur Reichweite (450 m, Upgrade
  „Weitbereichsscanner“ +60 %, im Nebel weniger). Markiert 30 s lang Wracks, erzhaltige
  Asteroiden, Planetenvorkommen, Kapseln und Fracht (Rauten im Bild, Punkte im Radar,
  Beschriftung für Wracks und Vorkommen).
- **42** Kartografie: Am Ende des Impulses wird das Gebiet aufgedeckt und in einem zweiten
  Raster als kartiert vermerkt (`surveyed`, im Spielstand). Neue Zellen sind Kartendaten, die
  man an Stationen verkauft (3 Cr pro Zelle, +10 % pro Rufstufe). Dieselbe Gegend zweimal zu
  scannen bringt nichts.
- Vorführszene `DRIFTCREW_SCENE=sectors`.
- Tests: 59 grün (neu `solar_wind_pushes_and_nebula_hides`, `debris_drifts_in_the_debris_zone`,
  `scanner_finds_wrecks_and_charts_sell`, `random_events_cover_all_kinds`, `compass_names`).

### Runde 3 – Phase 5: Koop und Crew (erledigt)

- **24** Ping: feste, nicht belegbare Tasten – Tastatur `^` (`KeyCode::Backquote`, markiert die
  Mausposition), Gamepad Stick-Klick L3/R3 (markiert die Stickrichtung, 45 m; ohne Ausschlag
  30 m voraus). Läuft als `Command::Ping { player, pos }` durch die Simulation; ein Ping pro
  Spieler, 6 s sichtbar, beschriftet mit dem, was dort ist (`SimState::describe_spot`).
  Anzeige: Kreis mit Welle und Fadenkreuz in Spielerfarbe, Wegmarke am Bildrand, Klang.
  L3/R3 sind dafür als Slot-Tasten gesperrt.
- **25** Hot-Join: ein Gerät, das noch nicht zur Crew gehört, drückt im Flug eine belegbare
  Taste und übernimmt den nächsten freien Slot (Reihenfolge wie in der Lobby). Der Umbau läuft
  als `Command::SetLoadout` durch die Simulation (`rebuild_ship` hält Position, Drehung und
  Schwung). Sind alle Slots belegt, gibt es einen Hinweis aufs Pausemenü.
- **50** Empfohlene Crewgröße: `crew: (von, bis)` pro Schiff in `ships.ron`, pro Auftragsart in
  `missions.ron`. Anzeige in der Werft, in Aufträgen und Notrufen sowie in der Lobby
  („ihr seid 2 – passt“). Keine Bots: Slots belegen nur Menschen.
- Vorführszene `DRIFTCREW_SCENE=coop`.
- Tests: 54 grün (neu `ping_names_the_spot_and_fades`, `hot_join_rebuilds_the_ship_in_flight`,
  `every_ship_and_mission_type_has_a_crew_size`, `ping_buttons_are_never_slots`).

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
