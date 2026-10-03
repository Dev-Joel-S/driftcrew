# Weltraum-Koop-Spiel (Arbeitstitel offen)

## Idee

Inspiriert vom Prinzip von *Rakete* (rakete.li, Mario von Rickenbach): Jede Taste ist ein Triebwerk, der Rest ist Physik. Daraus wird ein eigenes Spiel mit schwereloser Physik, Open World, Raumstationen, Werften und Missionen.

Wichtig: Kein Klon. Eigener Name, eigener Look, eigene Assets. Nur das Grundprinzip wird übernommen.

## Grundsatzentscheidungen

- **Darstellung:** 2.5D, also 3D-Grafik, aber Bewegung und Physik nur in einer 2D-Ebene
- **Stack:** Rust + Bevy (Begründung siehe Architektur)
- **Erste Version:** Flug, Stationen und erste Missionen

## Physik

- Keine Schwerkraft im freien Raum. Trägheit und Drehmoment sind die Kernmechanik, alles driftet weiter, bis Gegenschub gegeben wird
- **Schwerkraft gibt es nur an Anomalien und Schwarzen Löchern** (lokaler Sog). Planeten ziehen nicht an, sie sind Hindernisse, Landeorte und Erzquellen. Schwarze Löcher haben einen Ereignishorizont: Wer ihn berührt, verliert das Schiff *(geändert in Runde 2, siehe WORKLOG)*
- Triebwerke sitzen in einer Reihe am Heck und schieben alle in dieselbe Richtung
- Positionen beim Standardschiff: außen links, links, Mitte, rechts, außen rechts (max. 5 Schub-Slots)
- Mindestens 2 Triebwerke (links + rechts). Reihenfolge beim Hinzukommen: links/rechts, dann Mitte, dann außen links/rechts
- Wirkung:
  - Mitte schiebt nur geradeaus
  - Links/rechts drehen leicht, für Feinkorrektur
  - Außen links/rechts drehen stark (längerer Hebel)
- Masse, Schwerpunkt und Trägheitsmoment werden aus den Teilen des Schiffs berechnet
- Fracht verändert Masse und Schwerpunkt, seitlich hängende Ladung zieht das Schiff in eine Richtung
- **Lebensbalken** statt Sofort-Tod. Schaden abhängig von der Aufprallgeschwindigkeit
- Meteoriten verursachen Schaden bis hin zur Explosion
- Treffer können **einzelne Triebwerke** beschädigen: erst Stottern, dann Ausfall. Reparatur an Stationen
- **Schild lädt nach** einigen Sekunden ohne Treffer nach, die **Hülle nie von selbst** (Reparatur kostet; Upgrade: Reparaturdrohnen)
- **Kein Game Over:** Bei Hülle 0 wird eine Rettungskapsel ausgestoßen, das Schiff wird zur Heimatstation geborgen, die Bergungskosten zahlt die gemeinsame Kasse
- **Treibstoff:** Tank pro Schiff, Verbrauch je feuerndem Triebwerk nach Schub. Leer bedeutet nicht Stillstand, sondern Notreserve mit 25 % Schub (ohne Reibung könnte eine Crew sonst endgültig festsitzen). Tanken an Stationen

## Steuerung: Slot-System

- Das Spiel kennt keine festen Spieler oder Controller, sondern nur **Slots** am Schiff
- Ein Slot ist ein Triebwerk oder ein Werkzeug (Waffe, Kran, Bohrer/Miner usw.)
- **Lobby:** Wer eine Taste drückt, claimt einen Slot. Nicht belegte Triebwerke verschwinden, die belegten werden symmetrisch neu angeordnet
- Ein Spieler kann beliebig viele Slots haben, theoretisch alles außer der Waffe, oder fünf Leute je einen
- Jedes Werkzeug liegt auf einer eigenen Taste. Beispiel: Spieler A hat Triebwerk 2 + Waffe, Spieler B Triebwerk 1 + Kran
- Eingabegeräte: Tastatur, Maus, Gamepads inkl. Switch Joy-Cons (jeder Joy-Con einzeln als eigenes Gerät)
- Joystick oder Maus zum Zielen von Werkzeugen (Waffe, Kranarm, Bohrer)
- **Ping** pro Spieler (feste Taste), **Hot-Join** mitten im Flug, empfohlene Crewgröße pro Schiff und Auftrag. Slots belegen nur Menschen, keine Bots
- Solo: z. B. zwei Joy-Cons, linker steuert die linke Seite, rechter die rechte
- Slots können während des Spiels neu verteilt werden (z. B. wenn jemand ausfällt)

## Werkstatt und Umbau

- **Crew-Lager** für Material und Bauteile, an jeder Station befüllbar, in jeder Werft verbaubar
- **Feste Bauplätze** pro Rumpf (Daten in `ships.ron`), Module aus `modules.ron` (Triebwerk, Fracht, Panzerung, Kran, Bohrer); Masse, Schwerpunkt und Trägheit folgen aus den Teilen
- **Werft-Editor** im Stationsmenü (Reiter „Bau“) mit Markierungen am Schiff; Bauplan als RON exportierbar
- **Upgrades pro Schiffsteil** mit Nachteil (Zusatzmasse am Teil), Kosten aus Credits, Material, Bauteilen und für die stärkste Stufe einem Artefakt; Kauf über die Abstimmung. Upgrades gelten für die Crew und wandern beim Schiffswechsel mit, Module bleiben am Rumpf

## Minispiele für einzelne Slots

- Laufen in der Simulation und nur mit den Slot-Tasten
- **Reparatur im Takt:** ausgefallenes Triebwerk mit der eigenen (toten) Taste flicken, auch im Flug; **Notreparatur** der Hülle bis 75 %, wenn das Schiff ruht – dann flicken alle Slots im Takt, lange halten = weiterfliegen
- **Ersatzteil mit dem Kran** an ein ausgefallenes Triebwerk setzen und ruhig halten (eingeschränkte Funktion, volle Reparatur an der Station)
- **Ruhige Hand:** Bohrertrag hängt von der Zielhand ab, sanft geführte Lasten bringen einen Bonus
- **Andockport hacken** (Schmugglernest): Muster aus Slot-Tasten, jede Person ihren Slot; Fehler lösen einen Störimpuls aus

## Schiffe

- Schiffe werden als **Daten** beschrieben, nicht im Code: Liste von Teilen mit Position, Richtung, Masse, Typ und Parametern (z. B. Schubstärke)
- Verschiedene Schiffe mit verschiedenen Layouts, z. B. Scout mit 2 Triebwerken oder Frachter mit 7 Triebwerken und 2 Kränen
- Die Lobby liest die Slot-Liste aus dem jeweiligen Schiff
- Schiffe werden **gekauft**, nur in Weltraumwerften. Jede Werft hat ein eigenes Angebot (Daten)

## Welt

- Open World in einer 2D-Ebene
- **Raumstationen:** Munition (z. B. 10 Schuss), Schild, Reparatur, Upgrades, Material abgeben oder aufladen, Missionen annehmen
- **Weltraumwerften:** Schiffe kaufen, jede mit eigenem Angebot
- **Mini-Planeten:** Abbau von Rohstoffen, Material abgeben oder aufladen. Freie **Landezonen** (Plattformen direkt neben einem Erzvorkommen): Landen wie Andocken, gelandet bleiben die Werkzeuge aktiv
- Kaufen geht nur an Stationen und Werften
- **Andocken** ist die zentrale Fähigkeit: niedrige Geschwindigkeit, richtige Ausrichtung
- Gefahren: Meteoriten, Asteroidenfelder, Anomalien, Schwarze Löcher
- **Wracks** zum Ausschlachten: Bohrer gewinnt Schrott, Kran reißt Bauteile ab
- **Fog of War:** Karte und Radar zeigen nur Erkundetes, gespeichert im Spielstand
- **Sektoren mit Effekten:** Trümmer, Nebel (Sicht/Radar/Scanner gestört), Sonnenwind (seitliche Kraft)
- **Zufallsereignisse:** Meteoritenschauer, spontane Notsignale, Sonneneruptionen
- **Scanner/Sonar** als Werkzeug-Slot: markiert Wracks und Rohstoffe, kartiert Gebiete; Kartendaten sind verkaufbar
- **Lackiererei:** Rumpf-, Akzent- und Flammenfarben; Slotfarben bleiben an Kern, Ringen und HUD erkennbar

## Wirtschaft

- Münzen verdient man durch Missionen und Erzverkauf
- **Preise je Ort:** Erzankauf, Treibstoff und Service haben pro Station/Außenposten eigene Faktoren (Daten)
- **Gemeinsame Kasse** der Crew, Fortschritt (Kasse, Schiffe, Upgrades) gehört dem Spielstand der Crew
- **Dockgebühr** je Station (Ruf senkt sie), **Versicherung** als Abo (Anteil jeder Auftragsbelohnung, übernimmt einen Teil der Bergungskosten), **Schiffskredit** in der Werft (Anzahlung, Raten nach jedem Auftrag)
- **Schwankende Erzpreise:** Verkäufe drücken den Preis am Ort, er erholt sich langsam; zeitweise Nachfrage nach einem Erz. Deterministisch und im Spielstand
- **Crew-Abrechnung** nach jedem Auftrag: Einnahmen minus Abzüge, was in die Kasse geht, und was unterwegs schon bezahlt wurde
- **Abstimmungssystem** für Käufe:
  - Jemand wählt im Menü einen Kauf aus
  - Alle sehen ein Pop-up mit Artikel, Preis und Kassenstand danach
  - Zustimmen per eigener Slot-Taste
  - Kurzer Timer, wer nicht reagiert, enthält sich
  - Mehrheit entscheidet, Gleichstand bedeutet nein
  - Solo: direkter Kauf ohne Abstimmung

## Missionen

Überall Missionen annehmbar:

- **Liefern** von A nach B
- **Abbauen** auf Mini-Planeten und Material an Stationen abgeben
- **Notrufe / Hilfe**, z. B. treibendes Schiff abschleppen oder Kapseln einsammeln
- **Schwerlast:** Fracht, die in keinen Frachtraum passt, wird als Kiste am Kran geschleppt. Das Kranseil ist eine harte Längenbegrenzung (schlaff/straff), die Last pendelt und zerrt am Schiff
- **Material verschicken:** Erzladungen von Planeten-Außenposten zu Stationen
- Aufträge kommen von **Auftraggebern** (Name, Rolle, Porträt, Sprüche; Daten in `npcs.ron`)
- **Ruf pro Station:** mehr und besser bezahlte Aufträge, Schwerlast ab Stufe 1, Rabatt im Service
- **Passagiere:** sanft fliegen, sonst sinkt die Bezahlung
- **Bergung sperriger Objekte:** außen am Kran, Form/Masse/Engstellen bestimmen die Schwierigkeit, nur machbare Aufträge werden angeboten
- **Präzisionsarbeit:** Erzadern und Wrackverbindungen verlangen, dass der Bohrer ruhig auf einer Stelle bleibt – die anderen stabilisieren
- **Stationen wieder aufbauen:** Etappen mit Material und Bauteilen, die Station ändert Aussehen und Dienste
- **Richtzeit, Zeit- und Sauberkeitsbonus** bei jedem Auftrag
- **Funk** beim Anflug und Andocken, erzählt nebenbei Bruchstücke der Welt
- **Auswertung** nach jedem Auftrag mit Spaßstatistik pro Slot (meiste Schubzeit, meiste Kollisionen …)
- **Flugmanöver als Auftragsziel:** Messflug (im Messfeld stillhalten – am Anomalierand, im Sonnenwind, im rotierenden Wrackring), Schwerlast präzise in eine **Lastaufnahme** setzen (Kepler, Vega). Gleiche Physik, keine Sonderregeln
- Weitere Typen später erweiterbar

## Training und Zeitrennen

- **In der offenen Welt**, nicht in eigenen Szenarien: Parcours sind Folgen von Toren, Punkten, Bojen, Halte-Feldern und Andockplätzen (Daten in `courses.ron`)
- Ein Lauf startet beim Durchfliegen des **Starttors in Pfeilrichtung** (oder nach Auswahl im Stationsmenü am Starttor). Abbruch durch Abkommen vom Kurs, Andocken anderswo oder im Pausemenü
- **Grundkurs** als Training: Drehen, Schub, Bremsen, Andocken – mit Hinweisen, einmaliger Zuschuss
- **Zeitrennen** mit Strafzeit (Kollisionen, beim Präzisionsandocken Versatz und Aufsetzgeschwindigkeit), Medaillen Bronze/Silber/Gold mit einmaliger Prämie und **Bestenliste pro Spielstand** (fünf beste Läufe mit Schiff und Crewgröße)
- Alles läuft in der deterministischen Simulation; die Anzeige liest nur

## Grafikstil

- Minimalistisch wie Rakete, aber mit kräftigen Farben wie ShellShock Live / No Man's Sky in Umgebung, Nebeln und Planeten
- Schiffe und Stationen erwachsener: gedeckte Lackierungen, Schrägen, Fasen und Paneele statt Würfel. **Leuchten nur gezielt:** Triebwerksflammen, Positionslichter, Slotfarben *(geändert in Runde 2)*
- Farben: Rot, Grün, Gelb, Orange, Blau, Türkis usw.
- Dunkler Hintergrund, Bloom, Partikel, Triebwerksglühen
- Jeder Slot hat eine eigene Farbe, die Triebwerksflamme leuchtet in dieser Farbe, damit sichtbar ist, wer gerade schiebt
- Orte farblich unterscheidbar (Stationen, Werften, Abbauplaneten, Notrufe)

## Umfang Version 1

- Flugphysik mit Slot-System und Lobby
- Local Coop (Tastatur + Gamepads)
- Ein Standardschiff mit bis zu 5 Triebwerken und Werkzeug-Slots (Waffe, Kran, Bohrer)
- Lebensbalken, Kollisionsschaden, Meteoriten
- Kleine Welt: 1 bis 2 Raumstationen, einige Mini-Planeten, ein Asteroidenfeld
- Andocken, Stationsmenü (Munition, Schild, Reparatur, Upgrades)
- Erste Missionen: Liefern, Abbauen, Notruf
- Gemeinsame Kasse mit Abstimmung

## Später

- **Online-Koop** mit Rollback-Netcode (z. B. ggrs / bevy_ggrs)
- **Deterministische Physik mit Ganzzahlen / Fixed-Point** statt Floats, damit nur Eingaben übers Netz gehen
- Spielstand online beim Host
- Optional: Abstimmung auch für Missionen

## Architektur für Claude Code

- **Warum Bevy:** Rust, Datenorientierung passt zu Schiffen als Daten, und später lassen sich Fixed-Point-Physik und Rollback (ggrs) sauber anbinden
- **Simulation als eigenes Modul** ohne Abhängigkeit von Rendering. Feste Schrittweite (z. B. 60 Hz)
- **Input pro Tick** = Bitmaske der Slots + Zielwinkel für Werkzeuge. Die Simulation weiß nicht, wer gedrückt hat
- Simulation von Anfang an so schreiben, dass sie deterministisch sein kann: kein Zufall ohne Seed, keine Abhängigkeit von Framerate oder Iterationsreihenfolge
- **Physik selbst schreiben:** ein Rigidbody pro Schiff, Kräfte an versetzten Punkten, einfache Kollisionen (Kreise / konvexe Formen). Das erleichtert den späteren Umstieg auf Fixed-Point
- Schiffe, Stationen, Missionen und Shop-Preise als Datendateien (z. B. RON)
- Joy-Con-Unterstützung (einzelne Joy-Cons als eigene Geräte) früh testen
- Keine Namen, Grafiken oder Assets aus Rakete verwenden

## Entscheidungsänderungen

Was sich gegenüber der ersten Fassung geändert hat (Begründungen und Details im WORKLOG):

- **Runde 2, Grafik:** Weniger Leuchtfarbe, gedecktere Lackierungen. Leuchten nur noch bei Triebwerksflammen, Positionslichtern und Slotfarben. Schiffsteile und Stationen mit Schrägen, Fasen und Keilen statt reiner Würfel
- **Runde 2, Schwerkraft:** nur an Anomalien und Schwarzen Löchern, Planeten ohne Anziehung
- **Runde 2, Treibstoff:** neu, mit Notreserve (25 % Schub) statt Stillstand
- **Runde 2, Zerstörung:** Rettungskapsel und Bergungskosten statt einfachem Neustart an der Station
- **Runde 2, Zielen:** Werkzeuge zielen ausschließlich mit dem Gerät des Slot-Besitzers
- **Runde 3, Training und Zeitrennen:** in der offenen Welt mit Toren statt eigener Trainingsszenarien, Bestenliste pro Spielstand *(umgesetzt in Phase 7)*
- **Runde 3, NPC-Schiffe:** volle Physik wie das Crew-Schiff, gesteuert per Autopilot über echte Triebwerke *(entschieden, Umsetzung in Phase 12)*
- **Runde 3, Modulbau und Schiffseditor:** feste Bauplätze pro Rumpf (Daten) statt freiem Raster; die Belegung steht im Spielstand und lässt sich als RON exportieren *(umgesetzt in Phase 11)*
- **Runde 3, Material:** gemeinsames Crew-Lager wie die Kasse *(umgesetzt in Phase 11)*
- **Runde 3, Upgrades:** pro Schiffsteil mit Masse als Nachteil und Materialkosten; sie gelten weiter für die Crew (nicht pro Rumpf), Module dagegen gehören zum Rumpf
