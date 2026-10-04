# DriftCrew

**Jede Taste ein Triebwerk. Der Rest ist Physik.**

Ein Koop-Weltraumspiel in einer offenen Welt: Die Crew teilt sich ein Schiff, jede Person übernimmt
einzelne Triebwerke oder Werkzeuge. Schwerelose Physik, Raumstationen, Werften, Mini-Planeten,
Asteroidenfelder, Missionen und eine gemeinsame Kasse, über die abgestimmt wird.

Inspiriert vom Grundprinzip von *Rakete* (rakete.li, Mario von Rickenbach). Es ist kein Klon:
eigener Name, eigener Look, alle Grafiken und Klänge werden beim Start prozedural erzeugt.

![Titelbildschirm](docs/screenshots/titel.jpg)

| | |
|---|---|
| ![Lobby](docs/screenshots/lobby.jpg) | ![Abheben](docs/screenshots/abheben.jpg) |
| ![Planet Viridia](docs/screenshots/planet.jpg) | ![Asteroidenfeld](docs/screenshots/asteroiden.jpg) |
| ![Werft Orion](docs/screenshots/werft.jpg) | ![Gravitationsanomalie](docs/screenshots/anomalie.jpg) |
| ![Abstimmung](docs/screenshots/abstimmung.jpg) | ![Sektorkarte](docs/screenshots/karte.jpg) |
| ![Schwarzes Loch](docs/screenshots/schwarzes_loch.jpg) | ![Rettungskapsel](docs/screenshots/rettungskapsel.jpg) |
| ![Landezone auf Viridia](docs/screenshots/landezone.jpg) | ![Schwerlast am Kran](docs/screenshots/schwerlast.jpg) |
| ![Auftraggeber im Stationsmenü](docs/screenshots/auftraggeber.jpg) | ![Auswertung nach dem Auftrag](docs/screenshots/auswertung.jpg) |
| ![Wrack ausschlachten](docs/screenshots/wrack.jpg) | ![Karte mit Fog of War](docs/screenshots/karte_nebel.jpg) |
| ![Trümmerzone](docs/screenshots/truemmer.jpg) | ![Schleiernebel](docs/screenshots/nebel.jpg) |
| ![Relais Ost, verstummt](docs/screenshots/relais_verstummt.jpg) | ![Relais Ost nach Etappe 2](docs/screenshots/relais_ausgebaut.jpg) |
| ![Sperrige Bergung am Kran](docs/screenshots/sperrig.jpg) | ![Präzisionsarbeit an einer Erzader](docs/screenshots/praezision.jpg) |
| ![Parcours im Stationsmenü](docs/screenshots/parcours_menue.jpg) | ![Grundkurs: Drehen](docs/screenshots/training_drehen.jpg) |
| ![Zeitrennen durch den Nova-Ring](docs/screenshots/nova_ring.jpg) | ![Wrackring mit rotierender Öffnung](docs/screenshots/wrackring.jpg) |
| ![Messflug am Anomalierand](docs/screenshots/messflug.jpg) | ![Lastaufnahme an Kepler](docs/screenshots/lastaufnahme.jpg) |
| ![Ziel mit Bestenliste](docs/screenshots/parcours_ziel.jpg) | ![Grundkurs: Bremsen](docs/screenshots/training_bremsen.jpg) |
| ![Schiff auf Kredit](docs/screenshots/werft_kredit.jpg) | ![Markt mit Preistrend und Nachfrage](docs/screenshots/markt_trend.jpg) |
| ![Abrechnung nach dem Auftrag](docs/screenshots/abrechnung.jpg) | ![Triebwerk im Takt flicken](docs/screenshots/takt_triebwerk.jpg) |
| ![Notreparatur der Hülle](docs/screenshots/notreparatur.jpg) | ![Andockport hacken](docs/screenshots/port_hacken.jpg) |
| ![Ersatzteil mit dem Kran einsetzen](docs/screenshots/kran_ersatzteil.jpg) | ![Werft-Editor: Bauplätze](docs/screenshots/werft_bau.jpg) |
| ![Module für einen Bauplatz](docs/screenshots/bauplatz_module.jpg) | ![Umgebauter Driftkutter](docs/screenshots/umbau.jpg) |
| ![Upgrades pro Schiffsteil](docs/screenshots/upgrades_teile.jpg) | ![Händlerin Juno landet im Leitstrahl](docs/screenshots/verkehr_leitstrahl.jpg) |
| ![Piratendrohnen am Nest](docs/screenshots/piraten.jpg) | ![Geleitschutz mit Hinterhalt](docs/screenshots/geleitschutz.jpg) |
| ![Zollscan beim Schmuggeln](docs/screenshots/zollscan.jpg) | ![Sonderangebot der Händlerin](docs/screenshots/haendlerin.jpg) |
| ![Karte mit Verkehr und Piratennest](docs/screenshots/karte_verkehr.jpg) | ![Fremdes Signal: ein Artefakt](docs/screenshots/artefakt_signal.jpg) |
| ![Signalturm in Resonanz](docs/screenshots/signalturm_resonanz.jpg) | ![Der Signalturm erwacht](docs/screenshots/signalturm_erwacht.jpg) |
| ![Das Tor am Schlund ist offen](docs/screenshots/tor_offen.jpg) | ![Karte mit Monumenten und Fundorten](docs/screenshots/karte_geschichte.jpg) |
| ![Logbuch](docs/screenshots/logbuch.jpg) | |

*(Screenshots aus dem automatischen Vorführmodus, gerendert mit Software-Vulkan.)*

---

## Starten

### Fertige Testversion

- **Windows:** `driftcrew-windows.zip` entpacken, `driftcrew.exe` starten. Braucht eine
  Grafikkarte mit DirectX 12 oder Vulkan (alles ab etwa 2016). Windows Defender meldet
  unsignierte Programme manchmal („Weitere Informationen → Trotzdem ausführen“).
- **Linux:** `driftcrew-linux.tar.gz` entpacken, `./driftcrew` starten (Vulkan-Treiber nötig,
  glibc ab 2.39).
- **NixOS:** `driftcrew-nixos.tar.gz` entpacken und `./start.sh` ausführen. Das Skript holt
  über `shell.nix` die Bibliotheken (Vulkan, ALSA, udev, X11/Wayland) und startet das Binary mit
  dem glibc-Lader aus Nix – ohne steam-run, ohne Unfreies. Die beiden Dateien liegen auch unter
  `packaging/nixos/`. Alternativ aus dem Quellcode: `nix run .` (baut über `flake.nix`).

Stürzt das Spiel ab, steht der Grund in `crash.txt` neben dem Spielstand (Pfade unten).

**Koop:** Gespielt wird an *einem* Rechner – jede Person mit Tastatur, Gamepad oder Joy-Con.
Online-Koop gibt es noch nicht. Über Streaming (z. B. Steam Remote Play Together oder Parsec)
kann jemand von woanders mitspielen: Der Host startet DriftCrew, der Gast schickt seine
Gamepad-Eingaben über das Streaming-Programm.

### Aus dem Quellcode

Voraussetzungen: Rust ab 1.95 (stabil), eine Grafikkarte mit Vulkan, Metal oder DirectX 12.

Unter Linux braucht Bevy zusätzlich:

```bash
sudo apt install libasound2-dev libudev-dev pkg-config
```

Dann:

```bash
cargo run --release
```

Der erste Build dauert eine Weile (Bevy wird komplett übersetzt), danach geht es schnell.
`cargo test` startet die Tests der Simulation.

Unter NixOS: `nix develop` öffnet eine Shell mit Rust und allen Bibliotheken, danach wie oben
`cargo run --release`. Für Windows lässt sich unter Linux mit `rustup target add
x86_64-pc-windows-gnu`, MinGW (`mingw-w64`) und `cargo build --release --target
x86_64-pc-windows-gnu` übersetzen.

Der Spielstand der Crew liegt unter

- Linux: `~/.local/share/driftcrew/crew.ron`
- Windows: `%APPDATA%\driftcrew\crew.ron`
- macOS: `~/Library/Application Support/driftcrew/crew.ron`

(mit `DRIFTCREW_SAVE=/pfad/datei.ron` lässt sich ein anderer Ort wählen).

---

## So wird gespielt

### Lobby: Slots verteilen

1. **Weiterspielen** oder **Neues Spiel** wählen.
2. In der Lobby ist immer ein Slot markiert. Wer eine **beliebige Taste** drückt (Tastatur, Maus
   rechts/mitte oder eine Gamepad-Taste), übernimmt diesen Slot. Danach springt die Markierung zum
   nächsten freien Slot.
3. Reihenfolge der Triebwerke: links/rechts, dann Mitte, dann außen links/rechts.
   **Nicht belegte Triebwerke verschwinden, die belegten werden symmetrisch neu angeordnet** – das
   sieht man live am Schiff, inklusive Tastenbeschriftung.
4. Werkzeuge (Kanone, Kran, Bohrer) bekommen ebenfalls je eine Taste. Gezielt wird mit der **Maus**,
   wenn das Werkzeug per Tastatur belegt wurde, sonst mit dem **Stick** des jeweiligen Gamepads.
5. **Enter / Start** – los geht's. Mindestens zwei Triebwerke müssen belegt sein.

Tastatur+Maus zählt als ein Crewmitglied, jedes Gamepad als eigenes – auch **jeder einzelne
Joy-Con**. Wer will, nimmt beliebig viele Slots; fünf Leute können sich auch je einen Slot teilen.
Über das Pausemenü lassen sich Slots jederzeit **neu verteilen**. Jedes Schiff nennt eine
**empfohlene Crewgröße** (Kolibri 1–2, Driftkutter 2–3, Pelikan 3–5, Lastesel 4–6), Aufträge
ebenso. Slots belegen nur Menschen – es gibt keine Bots.

**Hot-Join:** Wer später dazukommt, drückt mitten im Flug eine Taste an seinem Gerät und übernimmt
den nächsten freien Slot. Das Schiff wird dabei umgebaut, ohne Schwung zu verlieren.

**Ping:** `^` (Taste links neben der 1) markiert die Mausposition, am Gamepad markiert ein
Stick-Klick die Stickrichtung (ohne Ausschlag: voraus). Alle sehen den Ping in der Farbe des
Spielers, mit Beschriftung, was dort ist („Wrack“, „Kepler-Außenposten“ …).

| In der Lobby | Wirkung |
|---|---|
| beliebige Taste | markierten Slot übernehmen |
| Tab / ↑ ↓ / Stick / Klick | anderen Slot markieren |
| eigene Taste ~1 s halten | Slot abgeben |
| Rücktaste | letzte Tastatur-Belegung lösen |
| Enter / Start (Joy-Con − / +) | Spiel starten |
| Esc / Select (Joy-Con Foto / Home) | zurück |

Der letzte Slot **„Bremse / rückwärts“** ist freiwillig: Wer ihn belegt, bremst mit dieser Taste
wie mit ↓ – gedacht für Geräte ohne Steuerkreuz (einzelner Joy-Con).

**Joy-Cons:** Jeder einzelne Joy-Con ist ein eigenes Crewmitglied und wird **hochkant**
gehalten wie eine Hälfte eines Controllers – der Stick zeigt, wohin man ihn drückt. Tasten zählen
nach ihrer Lage: unten bestätigt, rechts geht zurück, − / + wirkt wie Start, Foto / Home wie
Select. Beschriftet sind Slots mit dem Aufdruck
(←, ↓, SL, A …). Unter Linux liest das Spiel einzelne Joy-Cons direkt über die Kernelcodes
(hid-nintendo), weil die übliche Zuordnungstabelle dort falsche Tasten liefert
(`src/pads.rs`). Gekoppelte Paare (joycon „Combined“) laufen als normales Gamepad.

### Fliegen

Alle Triebwerke sitzen am Heck und schieben nach vorne. Wer **links** schiebt, dreht das Schiff nach
**rechts** und umgekehrt; die äußeren Triebwerke haben den längeren Hebel und drehen stärker, die
Mitte schiebt nur geradeaus. Es gibt keine Reibung: **alles driftet weiter, bis jemand gegensteuert.**

Masse, Schwerpunkt und Trägheitsmoment werden aus den Schiffsteilen berechnet. Fracht verändert
beides – ein seitlich verstauter Container oder ein Asteroid am Kranseil zieht das Schiff spürbar
in eine Richtung.

Statt Sofort-Tod gibt es einen **Lebensbalken**: Schaden hängt von der Aufprallgeschwindigkeit ab,
der Schild fängt zuerst ab. Meteoriten, Asteroiden, rotierende Balken, die Gravitationsanomalie und
das Schwarze Loch sind gefährlich. Treffer in der Nähe eines Triebwerks beschädigen es: erst
**stottert** es (setzt zufällig aus und schiebt schwächer), dann **fällt es aus**. Die Slot-Leiste
zeigt das an, repariert wird im Stationsservice.

Der **Schild lädt sich** nach ein paar Sekunden ohne Treffer wieder auf, die **Hülle nicht** –
Reparaturen kosten an Stationen (oder man baut Reparaturdrohnen ein, die langsam flicken).

Bei Hülle 0 gibt es **kein Game Over**: Die Crew wird in einer **Rettungskapsel** ausgestoßen
(die Kamera folgt ihr), das Schiff wird zur letzten Station geborgen. Die Bergungskosten
(fester Betrag + Anteil der Kasse) gehen von der gemeinsamen Kasse ab und werden angezeigt.

**Schwerkraft** gibt es nur an Anomalien und am **Schwarzen Loch** – Planeten ziehen nicht an.
Im Sog des Schwarzen Lochs zeigt ein roter gestrichelter Kreis, ab wo auch Vollschub nicht mehr
reicht; wer den Ereignishorizont berührt, verliert das Schiff.

**Treibstoff:** Jedes Schiff hat einen Tank, jedes feuernde Triebwerk verbraucht je nach Schub.
Ist er leer, bleibt eine **Notreserve mit 25 % Schub** – festsitzen kann man ohne Reibung also nie.
Getankt wird an Stationen.

### Werkzeuge

- **Kanone** – schießt in Zielrichtung, mit Rückstoß. 10 Schuss, Nachschub an Stationen.
  Zerlegt Asteroiden (große zerbrechen, erzhaltige hinterlassen Erzbrocken) und Meteoriten.
- **Kran** – feuert einen Greifer. Kleines (Erzbrocken, Rettungskapseln) wird eingeholt und verstaut,
  Großes (Wracks, Schwerlastkisten, Asteroiden) hängt am Seil und wird geschleppt. Das Seil ist eine
  feste Längenbegrenzung: schlaff hängt es durch, straff zieht es – die Last pendelt und zerrt am
  Schiff. Ein zu harter Ruck reißt es. Nochmal drücken = loslassen.
- **Bohrer** – baut Erz an den leuchtenden Kristallvorkommen der Mini-Planeten und an erzhaltigen
  Asteroiden ab. Der Bohrer drückt das Schiff dabei leicht zurück.
- **Scanner** – sendet einen Sonar-Impuls rundum (450 m, Upgrade: mehr). Wracks, Erz, Vorkommen,
  Kapseln und Fracht werden 30 s lang markiert (Radar und im Bild), das Gebiet wird aufgedeckt
  und **kartiert**. Neue Kartendaten verkauft man an Stationen (Markt).

### Andocken

Die zentrale Fähigkeit: **langsam (< 2,6 m/s), gerade (Nase in Plattformrichtung) und ohne Drehung**
auf eine Landeplattform setzen. Die Plattform leuchtet gelb beim Anflug und grün, wenn alles passt;
unten im HUD stehen Tempo, Ausrichtung und Drehung. Ein Triebwerk zünden = abdocken.

Angedockt öffnet sich das **Stationsmenü**: Service (Munition, Schild, Reparatur, Triebwerke,
Tanken), Upgrades, Aufträge, Markt (Erz verkaufen, Preistafel mit dem besten Ort je Erz) und in
Werften der Schiffshandel. **Jeder Ort hat eigene Preise**, jede Werft ihr **eigenes Angebot**.
Planeten-Außenposten nehmen Erz an.

**Lackiererei:** An Stationen mit Werkstatt lassen sich Rumpf, Akzent und Flammen umlackieren
(abgestimmt wie jeder Kauf). Eigene Flammenfarben färben die Außenflamme – Kern, Kennringe und
Slot-Leiste bleiben in Slotfarbe, damit man weiter sieht, wer schiebt.

**Wracks ausschlachten:** Im Schiffsfriedhof (und vereinzelt anderswo) treiben Wracks. Der Bohrer
holt **Schrott** heraus (wird wie Erz verkauft), der Kran reißt mit einem kräftigen Ruck
**Bauteile** ab, die man einholt und am Markt verkauft.

**Seilbelastung:** Das Kranseil färbt sich von der Slotfarbe über Gelb nach Rot, je stärker es
gezogen wird; kurz vor dem Reißen gibt es eine Warnung.

**Präzisionsarbeit:** Manche Asteroiden haben eine reiche **Erzader**, manche Wracks
**Verbindungsbolzen** (goldene bzw. türkise Markierung). Der Bohrer muss ein paar Sekunden genau
darauf bleiben, während der Rest der Crew das Schiff ruhig hält. Fortschritt und Abweichung
stehen für alle sichtbar unten in der Mitte. Belohnung: ein wertvoller Kristallkern bzw. ein
Bauteil aus dem Wrack.

**Stationen wieder aufbauen:** **Relais Ost** im Nordosten ist verstummt – dunkle Fenster,
kein Leuchtfeuer, nur eine Plattform. Im Reiter „Aufbau“ gibt die Crew Material und Bauteile ab.
Jede Etappe ändert das Aussehen (Licht, neuer Andockflügel, Sendemast) und bringt Dienste zurück;
die Station bezahlt jede Etappe, der Fortschritt steht im Spielstand.

**Sektoren und Ereignisse:** Manche Gegenden haben eigene Bedingungen – in der **Trümmerzone**
treibt Schrott, im **Schleiernebel** sind Sicht, Radar und Scanner gestört, im
**Sonnenwind-Korridor** schiebt es das Schiff seitlich. Unterwegs passieren ab und zu Dinge:
**Meteoritenschauer** (mit Vorwarnung und Himmelsrichtung), **spontane Notsignale** in der Nähe
(besser bezahlt, auf der Karte annehmbar) und **Sonneneruptionen** (Schild geschwächt, Radar und
Scanner gestört).

**Fog of War:** Karte und Radar zeigen nur, was die Crew schon gesehen hat. Bekannte Stationen
sind von Anfang an eingetragen, Planeten, Felder, Wracks und manche Stationen (Werft Vega) muss
man erst finden. Die erkundeten Gebiete stehen im Spielstand.

**Landen auf Planeten:** Neben den Außenposten haben die Mini-Planeten freie **Landezonen** mit
einem Erzvorkommen direkt daneben. Gelandet wird wie beim Andocken; dort bleiben die Werkzeuge
aktiv – das Schiff steht still, während der Bohrer arbeitet.

### Missionen

Aufträge kommen von **Auftraggebern** mit Namen, Rolle und Porträt (Frachtmeisterin,
Bergbauingenieur, Außenpostenleiter …). Nach jedem erledigten Auftrag gibt es eine **Auswertung**:
Belohnung, Ruf, Dauer, Strecke, Schaden – und kleine Auszeichnungen pro Slot
(*Schubmeister*, *Sparfuchs*, *Bruchpilot*, *Scharfschütze*, *Bohrkönig*, *Greifarm*), bei mehreren
Spielern mit Spieler-Abzeichen.

**Ruf** gibt es pro Station (Neu → Bekannt → Geschätzt → Partner). Höhere Stufen bringen mehr und
besser bezahlte Aufträge, größere Abbauaufträge, Schwerlast-Aufträge (ab „Bekannt“) und Rabatt im
Service.

- **Liefern** – Container an Station A abholen (landet im Frachtraum), zu Station B bringen.
- **Material verschicken** – an einem Planeten-Außenposten eine Erzladung aufnehmen und zu einer
  Station bringen. Die Ladung verschiebt Masse und Schwerpunkt.
- **Schwerlast** – eine Kiste, die in keinen Frachtraum passt, am Kran zu Station B schleppen.
  Kepler und Vega haben eine **Lastaufnahme**: eine U-förmige Halterung, in die die Kiste
  präzise gesetzt werden muss – ruhig hineinlegen und vom Kran lösen.
- **Messflug** – ein oder zwei Messfelder anfliegen und darin ein paar Sekunden stillhalten.
  Am Anomalierand zieht der Sog, im Sonnenwind treibt das Schiff, im Wrackring dreht sich die
  Öffnung, durch die man hinein muss. Die Daten gehen per Funk raus.
- **Bergung** – sperrige Objekte (Antennenmast, Ringsegment, Rumpfplatte, Sonnensegel-Träger)
  passen in keinen Frachtraum. Sie werden außen am Kran geschleppt und müssen in die
  **Ablagezone** der Station – bei Nova-Hub nur durch die schmalen Ringöffnungen. Lange Teile
  pendeln, wenn man sie am Ende greift. Angeboten wird nur, was das aktuelle Schiff mit seiner
  Ausrüstung schaffen kann.
- **Passagiere** – Leute von A nach B bringen. Harte Beschleunigung, Stöße und Kreiseln senken
  ihre Zufriedenheit und damit die Bezahlung.

Jeder Auftrag hat eine **Richtzeit**. Wer darunter bleibt, bekommt einen Zeitbonus, wer ohne
Kollision und Schaden fliegt, einen Sauberkeitsbonus. Beim Anflug und Andocken meldet sich die
Station per **Funk**.
- **Abbauen** – bestimmtes Erz abbauen und an der Station abliefern.
- **Notrufe** – überall annehmbar (Karte mit **Tab**): treibendes Wrack mit dem Kran zur Station
  schleppen oder Rettungskapseln einsammeln und abliefern. Die Rivalen-Crew **Kestrel** jagt
  dieselben Notrufe – wer zuerst annimmt, hat ihn.
- **Geleitschutz** – ein Frachter legt mit euch ab (abdocken und in der Nähe bleiben) und fliegt
  zum Ziel. Unterwegs greifen Piratendrohnen ihn an; über ihm steht seine Hülle. Kommt er an,
  ist der Auftrag erfüllt.
- **Schmuggel** – Vex im Nebelhafen gibt heiße Ware mit. Vor Nova, Kepler und Vega stehen
  **Zollbojen**: Wer mit der Ware 3,5 s in ihrem Radius bleibt, verliert sie, zahlt Strafe und
  Ruf. Im Bogen fliegen oder schnell durch.

### Verkehr: andere Schiffe

Die Welt ist nicht leer. Frachter pendeln zwischen den Stationen, Schürfroboter bauen im
Kobaltschwarm ab, Piratendrohnen kommen aus ihren Nestern, wenn ihr in der Nähe seid. Alle
NPC-Schiffe sind echte Schiffe mit derselben Physik: Man kann sie anstoßen, abschießen, sie
blockieren Plattformen. Stationen führen sie beim Start und bei der Landung mit einem
**Leitstrahl** (die hellblauen Linien).

- **Juno**, die Händlerin, fliegt mit ihrem Pelikan eine feste Route. Wo sie angedockt hat, gibt
  es den Reiter **Händlerin** mit Material und Bauteilen direkt ins Crew-Lager.
- **Orsk**, der Mechaniker der Werft Orion, verkauft dort Feintuning-Upgrades ohne Zusatzmasse.
- **Piraten** halten Abstand und schießen mit Vorhalt (rote Geschosse). Zerstörte Drohnen
  hinterlassen ein Bauteil. Die Nester stehen auf der Karte, sobald sie entdeckt sind.

### Minispiele für einzelne Slots

Alles läuft mit den eigenen Slot-Tasten und in der Simulation (also später auch online).

- **Triebwerk im Takt flicken:** Fällt ein Triebwerk aus, ist seine Taste tot – wer den Slot
  hat, tippt sie im Takt der Leiste unten (grüne Zonen). Acht Treffer, dann läuft es wieder,
  stottert aber. Das geht auch mitten im Flug, während die anderen steuern.
- **Notreparatur der Hülle:** Liegt die Hülle unter 75 % und das Schiff ein paar Sekunden ruhig,
  ohne dass jemand etwas drückt, beginnt die Notreparatur. Dann flicken alle Slot-Tasten im Takt
  die Hülle (bis 75 %) – mehr Leute, mehr Treffer. Eine Taste lange halten: weiterfliegen.
- **Ersatzteil mit dem Kran:** Liegt ein Bauteil aus einem Wrack im Frachtraum, richtet die
  Person am Kran ihn auf das ausgefallene Triebwerk und hält die Taste: das Teil wird eingesetzt,
  solange Ziel und Schiff ruhig bleiben.
- **Ruhige Hand beim Bohren:** Je ruhiger gezielt wird, desto mehr Ertrag (bis +25 %, zittrig
  bis −30 %); die Slot-Leiste zeigt es an. Lasten, die nie hart ans Seil gerissen wurden, bringen
  einen Bonus („Last sanft geführt“).
- **Andockport hacken:** Der **Nebelhafen** im Schleiernebel ist ein Schmugglernest mit
  gesichertem Port. In Portnähe erscheint ein Muster aus Slots – der Reihe nach drücken, jede
  Person ihren Slot. Ein Fehler oder Zeitablauf löst einen Störimpuls aus (Schild weg, Port eine
  Weile gesperrt). Geknackt bleibt der Port zehn Minuten offen; drinnen zahlt man gut für Ionit,
  Solarit und Schrott.

### Parcours: Training und Zeitrennen

Draußen in der Welt stehen **Tore** mit Pfosten und Leuchtkappen. Wer in Pfeilrichtung durch ein
**Starttor** fliegt, startet den Lauf – oder wählt den Parcours im Stationsmenü (Reiter
**Parcours**) und fliegt dann hin. Oben in der Mitte stehen Schritt, Zeit, Strafzeit und ein
Hinweis; das nächste Ziel leuchtet, ein Pfeil am Rand zeigt die Richtung.

- **Grundkurs** (Training vor Nova, zugleich die **Einführung**) – Abdocken, Starttor, Nase auf
  eine Boje drehen und halten, Schub durch ein Tor, mit ↓ im Feld zum Stillstand bremsen, an Nova
  andocken (gern rückwärts). Eine neue Crew findet ihn ganz oben im Stationsmenü als
  „Einführung fliegen (optional)“ – wer lieber gleich losfliegt, fliegt einfach los. Einmal
  geschafft gibt es einen Ausbildungszuschuss.
- **Nova-Ring** – einmal um den Hub und mitten durch den Ring.
- **Dockprüfung** – durch die rechte Öffnung auf die obere Plattform: Versatz zur Mitte und
  Aufsetzgeschwindigkeit kosten Strafsekunden.
- **Wrackring** – im Schiffsfriedhof dreht sich ein alter Ringfrachter mit zwei Öffnungen:
  hinein, warten, wieder hinaus.

Jede Kollision kostet 2 s. Für Zeitrennen gibt es **Bronze, Silber, Gold** (die Prämie je
Medaille einmal pro Spielstand) und eine **Bestenliste** mit den fünf besten Läufen (Zeit,
Schiff, Crewgröße). Weit vom Kurs abkommen, woanders andocken oder „Parcours abbrechen“ im
Pausemenü beendet den Lauf ohne Wertung; durch das eigene Starttor fliegen startet neu.

### Werkstatt: Crew-Lager, Bauplätze, Upgrades

- **Crew-Lager:** Erz, Schrott und Bauteile aus Wracks lassen sich an jeder Station einlagern
  (Markt oder Werft: „Ins Crew-Lager einlagern“). Das Lager gehört der Crew wie die Kasse und
  ist in jeder Werft verbaubar.
- **Bauplätze (Reiter „Bau“ in der Werft):** Jeder Rumpf hat feste Bauplätze (Seite, Heck,
  Bug, Ausleger …). Dort lassen sich Module anbauen: Zusatztriebwerk (eigener Slot), Frachtmodul,
  Panzerplatte, Zusatzkran oder -bohrer – bezahlt mit Credits, Material und Bauteilen. Masse,
  Schwerpunkt und Trägheit rechnen sich aus den Teilen: Ein einseitiges Frachtmodul zieht den
  Schwerpunkt zur Seite, ein Triebwerk am Heck links dreht das Schiff mit. Am Schiff sind die
  Bauplätze markiert; neue Slots heißen „Slots neu verteilen“. Abbauen gibt die Hälfte des
  Materials zurück.
- **Bauplan exportieren:** schreibt das Schiff mit allen Modulen als RON-Datei (`bauplaene/`
  neben dem Spielstand) – lässt sich in `ships.ron` als neuer Rumpf übernehmen.
- **Upgrades pro Schiffsteil** (Triebwerke, Hülle, Schild, Fracht, Kran, Bohrer, Kanone,
  Scanner, Tank, Bordsysteme) in bis zu drei Stufen. Jedes hat einen Nachteil: Zusatzmasse am
  betroffenen Teil (Panzerung macht träge, der Zusatztank zieht den Schwerpunkt nach hinten).
  Kosten: Credits plus Material aus dem Lager, höhere Stufen auch Bauteile; die stärkste Stufe
  braucht ein bestimmtes Artefakt in der Sammlung als Schlüssel (es wird nicht verbraucht).
  Gekauft wird über die Abstimmung.
- **Upgrades verkaufen:** Eingebaute Upgrades lassen sich in jeder Upgrade-Werkstatt wieder
  ausbauen – zweimal bestätigen, es gibt die Hälfte des Kaufpreises (Material und Bauteile
  nicht). Stufen bauen aufeinander auf: erst die höchste ausbauen. Ein größerer Frachtraum geht
  nur, wenn die Fracht danach noch passt.

### Geschichte: das Signal, Artefakte, Monumente

Aus der Gravitationsanomalie kommt ein Muster, das kein Rauschen ist. Nach dem ersten Auftrag
meldet sich das verstummte Relais Ost per Funk – der Anfang eines roten Fadens in vier Kapiteln.
Neue Kapitel beginnen, wenn Ruf und Sammlung reichen; das Ende ist offen.

- **Artefakte** liegen pro Spielstand genau einmal irgendwo im Sektor (der Seed des
  Spielstands wählt die Orte). Der **Scanner** zeigt sie als „Fremdes Signal“. An Bord haben sie
  Masse und eine Nebenwirkung – einer stört die Instrumente, einer lockt Piraten, einer heizt
  den Tank leer, einer bringt das Schiff ins Trudeln. Erst an einer Station abgeliefert, gehören
  sie zur **Sammlung** der Crew. Wird das Schiff mit einem Artefakt an Bord zerstört, bleibt es
  im Wrack (im Logbuch abschaltbar).
- **Monumente:** Ein Signalturm und ein verschlossenes Tor, gebaut aus fremdem Stein. Sie
  glimmen, wenn Artefakte in der Nähe sind, und erwachen bei einem Scan, wenn die Sammlung groß
  genug ist.
- **Logbuch** (Pausemenü): Kapitel, Artefakte und Funde – Archive öffnen sich beim ersten
  Andocken, Bordbücher in Wracks und Funkfetzen fängt der Scanner auf. Gezählt wird „X von Y“,
  ohne zu verraten, wo der Rest liegt.

### Geld: Gebühren, Versicherung, Kredit, Markt

- **Dockgebühr** an Stationen (je Ort verschieden, Ruf senkt sie um 20 % pro Stufe). Wer kurz
  danach wieder an derselben Station andockt, zahlt nicht noch einmal. Relais Ost und die
  Planeten-Außenposten nehmen nichts.
- **Versicherung** (Reiter Service): kostet nichts beim Abschluss, aber 8 % jeder
  Auftragsbelohnung; dafür übernimmt sie 70 % der Bergungskosten nach einer Zerstörung.
  Kündbar jederzeit.
- **Schiffskredit** (Werft): Ist ein Schiff zu teuer, gibt es es auf Kredit – 25 % Anzahlung,
  der Rest plus 12 % Zinsen in acht Raten, je eine nach jedem erledigten Auftrag. In der Werft
  lässt sich der Rest auf einmal tilgen.
- **Schwankende Preise:** Jede verkaufte Tonne drückt den Erzpreis an diesem Ort, er erholt
  sich in ein paar Minuten. Ab und zu sucht ein Ort ein bestimmtes Erz und zahlt eine Weile
  deutlich mehr (Meldung, Markt, Karte).
- **Abrechnung** in der Auswertung: Einnahmen, Versicherung und Kreditrate, was in die Kasse
  geht – und was unterwegs schon ausgegeben wurde (Dock, Service, Bergung), dazu der Gewinn.

### Gemeinsame Kasse und Abstimmung

Credits gehören der Crew. Wer im Menü einen Kauf auswählt, startet eine **Abstimmung**: Alle sehen
Artikel, Preis und Kassenstand danach. Jede Person stimmt mit **ihrer eigenen Slot-Taste** ab
(Ja ↔ Nein umschalten). Nach kurzer Zeit gilt: Wer nicht reagiert hat, enthält sich, die Mehrheit
entscheidet, **Gleichstand bedeutet Nein**. Allein gespielt wird direkt gekauft.

### Feste Tasten (nie als Slot belegbar)

| Taste | Gamepad | Funktion |
|---|---|---|
| Esc | Start (im Flug) | Pause |
| Tab | Select | Sektorkarte |
| Pfeiltasten | Steuerkreuz | Menüs bedienen |
| Enter | Start | Bestätigen |
| Mausrad | – | Zoom |
| `^` | Stick drücken | Ping |
| F5 / F6 / F7 / F8 / F9 | – / – / Steuerkreuz ← / → / ↑ | Zuruf: Bremsen / Schub aus / Links drehen / Rechts drehen / Werkzeug bereit |
| ↓ (halten) | Steuerkreuz ↓ (halten) oder belegte Bremstaste | Bremsen; steht das Schiff, weiter halten = langsam rückwärts (Landen). Steuerdüsen zeigen, wohin gebremst wird |
| F11 | – | Vollbild (wie in den Einstellungen) |

**Flugassistenz** (Standard an): sanfterer Schub, das Schiff driftet nach dem Schub lange weiter
und rollt nur langsam aus (die Fahrt halbiert sich nach etwa 14 s), ein kleiner Stabilisator
dämpft Drehungen. In den Einstellungen abschaltbar – dann gilt die reibungsfreie Physik.

**Erze – was gibt es wo?** Erz leuchtet in seiner Farbe aus den Adern (Ferrit orange, Kobalt
blau, Solarit gelb, Ionit türkis), taubes Gestein bleibt dunkel. Der Bohrer zeigt bis 16 m
voraus, was im Ziel steckt („Kobalt · 4,2 t“, „taubes Gestein – kein Erz“, außerhalb der
Bohrreichweite mit „näher ran“), die Karte nennt die Erze jedes Felds. Jedes Feld hat ein
Haupterz und Beimischungen (`mixed` in `world.ron`): das Kieselfeld nördlich von Nova ist Ferrit
mit etwas Kobalt und Solarit, der Splittergürtel ebenso (dazu selten Ionit); Kobalt gibt es im
Kobaltschwarm und am Westwall, Solarit im Ascheriff und Glimmerband, Ionit in der Frostwolke
und an den Ostklippen. Bohrleistung 2,4 t/s, ein Asteroid trägt etwa 2,2 t pro Meter Radius.

**Bremse:** Die Grund-Bremse ist bewusst schwach (2 m/s², rückwärts höchstens 1,2 m/s). Stärker
wird sie mit den Upgrades **Bremsdüsen I/II** und dem **Gegenschub-Ring** (Bordsysteme).

**Ausrüstung:** Eine neue Crew hat nur Triebwerke, Bohrer und Scanner. Kran, Kanone und
Schildgenerator gibt es in Werften (Reiter „Upgrades“) oder versteckt in Wracks. Danach eine
freie Taste drücken – sie steuert dann das neue Werkzeug. Der Schild lädt nicht von selbst:
Ladung wird wie Munition an Stationen gekauft und ist bei einer Zerstörung weg.

**Einstellungen** (Titel oder Pause): Gesamt-, Effekt- und Musiklautstärke, Vollbild, Ton bei
Zurufen, Bildschirmwackeln und „Ton testen“. Gespeichert in `settings.ron` neben dem Spielstand.

### Zusammenarbeit, Fracht und Andenken (Phasen 14–18)

- **Übersteuern:** eigene Slot-Taste zweimal tippen und halten – +60 % Schub, das Triebwerk
  heizt auf, warnt zweimal und schaltet bei voller Hitze 4 s ab. Normales langes Drücken
  übersteuert nie.
- **Zusatzenergie:** Übersteuern, Schild-Schnellladen und Werkzeug-Boost (Bohrer, Kran) teilen
  sich einen Speicher (violetter Balken). Leer fällt nur der Bonus weg.
- **Zurufe:** kurze Signale in der eigenen Farbe über dem Schiff, Ton im Pausemenü abschaltbar.
- **Ladeplan** (Pause): Fracht zwischen Modulen umladen (Schwerpunkt!) oder abwerfen – sie
  treibt dann in der Welt und lässt sich später wieder einsammeln. Tanks schwappen,
  empfindliche Geräte vertragen keine Stöße, instabile Fracht keine harte Beschleunigung.
- **Rettung:** havariertes Schiff, längsseits gehen holt die Besatzung an Bord – jede Person
  braucht Platz. Gerettete melden sich später mit einem Geschenk.
- **Die Welt reagiert:** erledigte Aufträge versorgen Stationen, räumen Trümmer oder bringen
  Sender zum Laufen (10 Minuten). Versteckte Routen (Abkürzungen und sichere Umwege) findet der
  Scanner, ein Gerücht an der Station oder ein Sender; wer sie abfliegt, bekommt eine Zeit.
- **Persönliches:** Schiff taufen (Pause), Plaketten für besondere Leistungen am Rumpf, Tagebuch
  im Logbuch (←→) mit eigenen Notizen, Kartenmarkierungen in der Karte.
- **Unfall-Wiederholung:** nach einem schweren Unfall Enter / Select – die letzten Sekunden mit
  allen Slot-Eingaben, am Ende in Zeitlupe. Enter/Esc überspringt.

---

## Die Welt

Eine offene 2D-Ebene von gut 5 km Durchmesser, dargestellt in 3D (2.5D) mit Parallaxe.

- **Nova-Hub** – große Raumstation im Zentrum (Service, Upgrades, Aufträge, Markt)
- **Kepler-Außenposten** – Turmstation im Osten, bewacht von einem Rotor
- **Werft Orion** – Hangar im Westen (Driftkutter, Kolibri, Lastesel)
- **Werft Vega** – Turmwerft im Osten (Driftkutter, Hornisse, Pelikan), mit Markt
- **Mini-Planeten** Viridia (Kobalt), Ember (Solarit), Azura (Ionit, mit Ringen), Ferrox (Ferrit) –
  ohne Anziehung, mit Erzvorkommen, Landezonen und kleinen Außenposten
- **Splittergürtel** und **Kobaltschwarm** – Asteroidenfelder
- **Glutstrom** und **Sturzfeld** – Meteoritenschauer
- **Wrackring** im Schiffsfriedhof – ein langsam drehender Ringfrachter mit zwei Öffnungen
- **Nebelhafen** im Schleiernebel – Schmugglernest mit gesichertem Andockport, nahe dem Nest
  der **Nebelpiraten**; ein zweites Nest liegt am Schlund
- **Gravitationsanomalie** im Südosten – wer zu nah kommt, wird hineingezogen
- **Schlund** im Südwesten – ein Schwarzes Loch mit starkem Sog und Ereignishorizont
- **Äußerer Ring** (Phase 19): **Südwacht** im Süden am Ascheriff (Meteore der Südschauer),
  **Polarstation** im Dunst des Kältesees im Nordwesten, **Ostklippen** (Ionit) bei den
  Klippenpiraten, **Westwall**, **Frostwolke** und **Nordsturm** im Norden, **Kieselfeld** als
  Übungsfeld direkt bei Nova – weiter draußen ist es reicher, aber gefährlicher

Schiffe: **Driftkutter** (Standard, 5 Triebwerke, Kanone, Kran, Bohrer), **Kolibri** (Scout,
2–3 Triebwerke), **Hornisse** (Abfangjäger, 2 Kanonen), **Lastesel** (Frachter, 7 Triebwerke,
2 Kräne, 4 Frachtmodule), **Pelikan** (Bergungsschlepper mit Auslegern, 6 Triebwerke, 2 Kräne).

---

## Alles ist Daten

Schiffe, Welt, Preise und Missionen stehen in RON-Dateien unter `assets/data/`. Sie sind zusätzlich
in die Binärdatei eingebettet; liegt `assets/data/` im Arbeitsverzeichnis (z. B. beim Start mit
`cargo run`), haben diese Dateien Vorrang – so lässt sich ohne Neukompilieren experimentieren.

- `ships.ron` – Schiffe als Liste von Teilen (Position, Größe, Masse, Typ, Schubkraft, Form).
  Formen: `Box`, `Taper(oben, unten)`, `Chamfer(anteil)`, `Nose(anteil)`, `Tail(anteil)`,
  `Wing(links, rechts)` – Optik und Kollision folgen der Form. Die Reihenfolge der Triebwerke ist
  die Belegungsreihenfolge, `thruster_layouts` legt die symmetrische Anordnung für jede Anzahl fest.
  `fuel_capacity` und `fuel_burn` bestimmen Tank und Verbrauch.
- `world.ron` – Stationen als ASCII-Raster (eine Zelle = 4 m):
  `#` Block, `X` Akzentblock, `W` Fensterblock, `^ v < >` Landeplattform (Pfeil = Richtung),
  `L` Leuchtfeuer, `.` leer, Schrägen `/ \ 7 r` (Zeichen = volle Ecke: unten rechts, unten links,
  oben rechts, oben links). Freiliegende Außenecken werden automatisch abgeschrägt
  (`auto_chamfer: false` schaltet das ab). `prices` setzt Preisfaktoren je Ort (Erz je Sorte,
  Treibstoff, Service, Dockgebühr), `ships_for_sale` das Angebot einer Werft. Planeten haben
  `landing_zones` (Winkel in Grad; daneben liegt jeweils ein Erzvorkommen). Anomalien mit
  `kind: BlackHole` sind Schwarze Löcher (`core_radius` = Ereignishorizont). Regionen können
  einen `effect` haben (`Debris(dichte)`, `Nebula(stärke)`, `SolarWind((x, y), m/s²)`), `events`
  steuert die Zufallsereignisse. `project` beschreibt den Wiederaufbau einer Station in Etappen
  (Raster-Zeichen `1`–`3` = Blöcke, `a`–`c` = Plattformen ab dieser Etappe), `drop_zone` die
  Ablagezone für Bergungsobjekte, `socket` eine Lastaufnahme (Versatz, Richtung der Öffnung).
  Rotoren können statt Armen ein Ring mit Öffnungen sein (`shape: Ring(...)`), `hack` sichert
  den Andockport (Musterlänge, Zeit, Sperre, Öffnungsdauer). `known: false`
  versteckt eine Station, bis sie entdeckt ist. `wrecks` legt Wracks fest (Modell, Schrott,
  Bauteile). Dazu Asteroidenfelder, Meteoritenzonen, Rotoren, Nebelregionen und Notruf-Orte.
- `shop.ron` – Services (inkl. Tanken und Triebwerksreparatur), Upgrades, Grundpreise für Erz und
  Schrott, Startkapital, Bergungsgebühr, Lackfarben, Flammenfarben, Bauteile aus Wracks,
  `finance` (Dockgebühr, Versicherung, Kredit, Marktschwankung, Nachfrage).
- `missions.ron` – Vorlagen für Aufträge und Notrufe; Fracht mit `towed: true` ist Schwerlast,
  `bulky` beschreibt sperrige Bergungsobjekte (Länge, Dicke, Masse).
- `npcs.ron` – Auftraggeber: Name, Rolle, Ort, welche Aufträge sie vergeben, Porträtfarben, Sprüche.
- `radio.ron` – Funksprüche beim Anflug, Andocken und Abdocken, allgemein und pro Ort.
- `modules.ron` – Module für die Bauplätze (Teil, Größe, Masse, Kosten, passende Bauplatzart).
  Die Bauplätze selbst stehen in `ships.ron` (`mounts`).
- `courses.ron` – Parcours aus Schritten (`Gate`, `Pass`, `Face`, `Hold`, `Dock`) mit Hinweisen,
  Medaillenzeiten und Prämien, Strafzeiten und die Orte für Messflüge.
- `story.ron` – Geschichte: Artefakte (Masse, Nebenwirkung), mögliche Fundorte, Logbuch-Einträge
  mit ihrer Quelle (Station, Wrack, Funkfetzen an einem Ort, Fund, Kapitel, Monument), Kapitel
  (Voraussetzungen, Ziel, Funksprüche, Prämie) und Monumente als Raster (`o` = Tor-Block).
- `traffic.ron` – NPC-Verkehr: Andockplätze mit Anflugwegen, Frachter mit Routen, Piratennester,
  Schürfroboter, Zollbojen, Händlerin mit Sortiment, Rivalen-Crew, Schiff und Hinterhalt für
  Geleitschutz. Die NPC-Schiffe selbst (`drohne`, `schuerfer` und alle Crew-Schiffe) stehen in
  `ships.ron`.

---

## Architektur

```
src/
  sim/        Deterministische Simulation, keine Abhängigkeit vom Rendering
    mod.rs      SimState, TickInput (Slot-Bitmaske + Zielwinkel + Befehle), Schritt-Reihenfolge
    physics.rs  eigene Starrkörperphysik: Kräfte an versetzten Punkten, Impuls-Kontakte
    geom.rs     SAT/Clipping-Kontakte für konvexe Polygone und Kreise, Strahltests
    ship.rs     Schiff aus Daten: Masse, Schwerpunkt, Trägheit, Fracht, Tank, Triebwerkszustand
    tools.rs    Triebwerke (Stottern, Treibstoff), Kanone, Kran (Seil als harte Grenze), Bohrer
    dock.rs     Andocken und Landen, Zerstörung, Rettungskapsel, Bergung
    hazards.rs  Asteroidenfelder, Meteoriten, Geschosse
    missions.rs Aufträge, Auftraggeber, Notrufe, Ruf
    economy.rs  Kasse, Käufe, Abstimmung, Upgrades, Lack, Ortspreise, Werftangebot
    explore.rs  Fog of War (Raster, im Spielstand als Hex-Bitfeld)
    stats.rs    Spaßstatistik pro Slot, Auswertung mit Auszeichnungen
    sector.rs   Sektoreffekte, Zufallsereignisse, Scanner-Impulse, Kartendaten
    precision.rs Präzisionsarbeit (Erzadern, Wrackverbindungen)
    project.rs  Wiederaufbau von Stationen in Etappen
    story.rs    Geschichte: Artefakte, Logbuch, Kapitel, Monumente
    npc.rs      NPC-Schiffe: Leitstrahl, Wegfindung um Hindernisse, Frachter, Drohnen,
                Schürfer, Konvois, Händlerin, Rivalen, Zollbojen
    world.rs    Stationen aus Rastern, Planeten, Plattformen, Landezonen, Anomalien
    rng.rs      PCG32 – kein Zufall ohne Seed
  input.rs    Geräte → Slots, reservierte Tasten, Zielen
  game.rs     Zustände, feste 60-Hz-Schleife, Spielstand
  render/     2.5D-Darstellung, prozedurale Texturen und Meshes, Partikel
  ui/         Titel, Lobby, HUD, Stationsmenü, Abstimmung, Karte, Pause
  audio.rs    synthetisierte Klänge
  demo.rs     Vorführmodus für automatische Screenshots
```

Die Simulation läuft mit fester Schrittweite (60 Hz) und bekommt pro Tick nur eine Bitmaske der
gedrückten Slots, Zielwinkel und Menübefehle – sie weiß nicht, wer gedrückt hat. Es gibt keinen
Zufall ohne Seed, keine Hash-Iteration und keine Abhängigkeit von der Framerate. Ein Test prüft,
dass zwei Läufe mit gleicher Eingabe bitgleich enden; ein anderer lässt einen einfachen Autopiloten
nur mit den zwei äußeren Triebwerken von Nova-Hub zum Kepler-Außenposten fliegen und andocken.
Damit ist der Weg zu Rollback-Netcode vorbereitet.

**Vorführmodus:** `DRIFTCREW_DEMO=<ordner> DRIFTCREW_SCENE=<szene> cargo run` fliegt ein
Skript ab, speichert Screenshots und beendet sich. Szenen: `tour`, `ui`, `systems`, `progress`,
`coop`, `sectors`, `rules`, `rebuild`, `courses`, `finance`, `minigames`, `workshop`, `traffic`,
`story`.

---

## Ausblick

- Online-Koop mit Rollback-Netcode (z. B. `ggrs` / `bevy_ggrs`)
- Simulation auf Fixed-Point umstellen, damit nur Eingaben übers Netz gehen
- Spielstand online beim Host
- Abstimmung auch für Missionen
- mehr Sektoren, Stationen und Missionstypen

---

## Lizenz

Code: GPL-3.0 (siehe `LICENSE`).
Schrift: DejaVu Sans (Bitstream-Vera-Lizenz, siehe `assets/fonts/LICENSE-DejaVu.txt`).
Alle Texturen, Modelle und Klänge werden vom Spiel selbst erzeugt. Es werden keine Namen, Grafiken
oder Assets aus *Rakete* verwendet.
