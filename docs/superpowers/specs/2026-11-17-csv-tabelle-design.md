# CSV-Tabelle – Design

Stand: 2026-11-17 · freigegeben im Gespräch („Ja, nimm B“, „Ja so sollte es passen“)

## Absicht

Der Nutzer öffnet in Quillpad CSV-Dateien, um **Exporte anzusehen und von Hand zu pflegen**: vor allem
Telefonbücher aus 3CX und CSV-Exporte aus SQL-Datenbanken. Die Dateien sind **klein** (Hunderte bis wenige
Tausend Zeilen). Gepflegt wird zeilenweise: Nummer korrigieren, Kontakt ergänzen, Zeilen löschen.
Bisher kennt Quillpad CSV gar nicht (reiner Text). Vorbild für den Funktionsumfang ist Modern CSV;
übernommen werden **Ideen, keine Namen, Farben, Layouts oder Abläufe** (wie bei Notepad++).

Erfolg: Ein 3CX-Telefonbuch öffnen, als Tabelle sehen, ein paar Zellen ändern, eine Zeile löschen,
speichern – und die Datei unterscheidet sich vom Original **nur** an den geänderten Feldern.

## Ziele

- Zweite Ansicht **Tabelle** für dasselbe Dokument, umschaltbar mit der Textansicht.
- Zellen bearbeiten, Zeilen und Spalten einfügen, löschen, verschieben; Kopieren und Einfügen von Zellen.
- Filtern (nur Ansicht) und Sortieren (zuerst nur Ansicht, „Reihenfolge übernehmen“ ändert die Datei).
- Trennzeichen und Kopfzeile automatisch erkennen, in der Statusleiste umstellbar.
- Textansicht: Spalten farbig unterscheiden, Statusleiste zeigt Spaltennummer und -name.

## Nicht-Ziele

- Keine Formeln, keine Datentypen, keine Zahlenformatierung, kein Diagramm.
- Keine eigenen Dateiformate (xlsx usw.), kein Import/Export außer CSV/TSV als Text.
- Keine Optimierung für Riesendateien; sie sollen nur bedienbar bleiben (siehe „Grenzen“).
- Kein mehrstufiges Sortieren, keine gespeicherten Filter, kein Tabellendruck.

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es „funktioniert“)

- Die Datei wird beim Speichern **neu geschrieben** und verliert dabei Anführungszeichen, Leerzeichen,
  Zeilenenden, Kodierung oder BOM. Unveränderte Felder müssen **bytegleich** bleiben.
- Filtern oder Ansichtssortierung ändern die Datei oder markieren sie als geändert.
- Rückgängig verhält sich in der Tabelle anders als im Text.

## Ansätze (im Gespräch abgewogen)

- **A – nur farbige Textansicht:** schnell, garantiert verlustfrei, aber Pflegen bleibt fummelig
  (Kommas in Anführungszeichen, verrutschte Ausrichtung).
- **B – Tabellenansicht als zweite Ansicht desselben Textes (gewählt):** der Text bleibt die einzige
  Wahrheit; jede Tabellenänderung wird als gezielte Textersetzung im Dokument ausgeführt. Damit bleiben
  Trennzeichen, Anführungszeichen, Kodierung, Zeilenenden und Rückgängig automatisch erhalten.
- **C – eigenes Tabellenmodell, beim Speichern neu schreiben:** abgelehnt, verletzt das Versprechen
  „deine Datei bleibt deine Datei“.

Die farbigen Spalten aus A kommen als Ergänzung der Textansicht dazu.

## Entscheidungen des Nutzers

- Weg B.
- Filtern blendet nur aus, ändert die Datei nie.
- Klick auf eine Spaltenüberschrift sortiert nur die Ansicht; erst der Befehl **„Reihenfolge übernehmen“**
  stellt die Zeilen in der Datei um (ein Rückgängig-Schritt).
- Der Funktionsumfang unten („so sollte es passen, ein Test wird es zeigen“).

Alles Weitere unten ist **Entwicklerentscheid** (im Gespräch vorgeschlagen, nicht einzeln bestätigt).

## Verhalten

### Skizze (eigene Gestaltung in Quillpads Designs)

```
 Datei  Bearbeiten  Ansicht ...            [ Text | Tabelle ]
┌────┬──────────────┬─────────────┬──────────────────┐
│  # │ FirstName    │ LastName    │ Mobile           │  ← Kopfzeile bleibt beim Scrollen stehen
├────┼──────────────┼─────────────┼──────────────────┤
│  1 │ Anna         │ Berger      │ +49 170 1234567  │
│  2 │ Max          │ Huber       │ ▌+43 664 98765   │  ← Zelle in Bearbeitung
│  3 │ Müller, A.   │ …           │                  │
└────┴──────────────┴─────────────┴──────────────────┘
 Filter: [ berg ]  2 von 314 Zeilen · Trennzeichen ; · Kopfzeile ✓ · UTF-8 BOM · CRLF
```

Die Nummernspalte zeigt die Datensatznummer in der Datei (nicht die Bildschirmposition), damit sie
beim Filtern/Sortieren stabil bleibt. Felder werden **ohne** ihre umschließenden Anführungszeichen und
mit aufgelöstem `""` angezeigt.

### Wann die Tabelle angeboten wird

- Umschalter `Text | Tabelle` erscheint für Dokumente mit Dateiendung `.csv`, `.tsv`, `.tab` und für
  Dokumente, deren Sprache auf CSV/TSV gestellt ist. Über das Ansicht-Menü lässt sich die Tabelle für
  jedes Dokument einschalten.
- Beim Öffnen einer `.csv`/`.tsv`/`.tab` startet Quillpad in der **Tabellenansicht**.
- Die Ansicht gilt pro Tab (View). In der geteilten Ansicht kann links Text und rechts Tabelle desselben
  Dokuments stehen; beide zeigen jede Änderung sofort.

### Erkennung (Dialekt)

- Trennzeichen aus den ersten ~50 Datensätzen: Kandidaten `,` `;` Tab `|`; gewinnt der Kandidat mit der
  gleichmäßigsten Feldanzahl > 1 (außerhalb von Anführungszeichen gezählt). Endung `.tsv`/`.tab` → Tab.
  Kein Kandidat > 1 → Komma (eine Spalte).
- Anführungszeichen ist `"`; Verdopplung `""` ist das Escape. Felder dürfen Zeilenumbrüche enthalten,
  wenn sie in Anführungszeichen stehen (ein Datensatz kann also mehrere Textzeilen umfassen).
- Kopfzeile: angenommen, außer die erste Zeile enthält ein leeres Feld oder ein Feld, das eine Zahl ist.
  Im Zweifel **mit Kopfzeile** (3CX und SQL-Exporte haben fast immer eine).
- Trennzeichen und Kopfzeile sind in der Statusleiste umstellbar; die Wahl gilt für das Dokument
  während der Sitzung, nicht als globale Einstellung.

### Bearbeiten in der Tabelle

- Auswahl: eine aktive Zelle plus rechteckiger Bereich (Umschalt+Pfeil, Ziehen). Ganze Zeile über Klick
  auf die Nummer, ganze Spalte über Strg+Klick auf die Überschrift (Klick = sortieren; im Tooltip
  genannt).
- Tippen eines Zeichens ersetzt den Zellinhalt; F2 oder Doppelklick öffnet den vorhandenen Inhalt zum
  Bearbeiten. Enter bestätigt und geht nach unten, Tab nach rechts, Umschalt+Enter/Tab zurück, Esc
  verwirft. Zeilenumbruch in einer Zelle: Alt+Enter. Pfeiltasten bewegen die aktive Zelle (außerhalb
  der Bearbeitung).
- Entf leert die ausgewählten Zellen.
- Zeile einfügen (oberhalb/unterhalb), Zeile löschen, Zeile nach oben/unten verschieben;
  Spalte einfügen (links/rechts), Spalte löschen, Spalte nach links/rechts verschieben.
  Über Kontextmenü auf Zellen/Nummern/Überschriften und ein Menü „Tabelle“ (nur sichtbar, wenn die
  Tabellenansicht aktiv ist). Tastenkürzel: Strg+Plus / Strg+Minus für Zeile einfügen/löschen,
  sofern frei (bestehende Kürzel haben Vorrang).
- Kopieren: Auswahl als Tab-getrennter Text (Zeilen mit `\n`), Felder mit Tab/Zeilenumbruch/`"` in
  Anführungszeichen – damit Einfügen in Excel/LibreOffice funktioniert. Einfügen: Tab-getrennter Text
  wird ab der aktiven Zelle verteilt; fehlende Zeilen werden angehängt, überzählige Spalten
  abgeschnitten (Hinweis in der Statusleiste). Ein einzelner Wert in eine Mehrfachauswahl füllt alle
  ausgewählten Zellen.
- **Jede** dieser Aktionen ist genau **ein** Rückgängig-Schritt (`Document::begin` … `splice` … `commit`).
  Rückgängig/Wiederholen in der Tabelle macht die betroffene Zelle wieder zur aktiven Zelle.

### Wie Änderungen in den Text kommen (Kern von B)

- Ein Parser liefert für den aktuellen Text pro Datensatz die **Zeichenbereiche** jedes Feldes
  (Positionen in Zeichen, wie `Document` sie nutzt) samt „war in Anführungszeichen“. Er wird nach jeder
  Textänderung neu berechnet (Revision des Dokuments als Cache-Schlüssel).
- Zelle ändern = `splice` genau des Feldbereichs (inkl. vorhandener Anführungszeichen) mit dem neu
  kodierten Feld. Andere Zeichen werden nie angefasst. Unveränderter Wert → keine Änderung, kein
  „geändert“.
- Kodierung eines neuen Feldwerts: Anführungszeichen, wenn der Wert Trennzeichen, `"`, `\r` oder `\n`
  enthält oder mit Leerzeichen beginnt/endet, **oder** wenn das bisherige Feld in Anführungszeichen
  stand (Stil des Feldes bleibt); `"` wird zu `""`. Neue Felder in neuen Zeilen/Spalten übernehmen den
  Stil der Datei: Stehen in der Datei mindestens 90 % der nicht-leeren Felder in Anführungszeichen,
  bekommen neue nicht-leere Felder auch welche.
- Zeilenumbruch innerhalb eines Feldes wird als `\n` eingefügt (Dokument-intern); beim Speichern gilt das
  normale Zeilenende-Verhalten von Quillpad (wie heute für jeden Text).
- Neue Zeilen erhalten so viele Felder wie die Kopfzeile bzw. die häufigste Feldanzahl. Zeilen mit
  abweichender Feldanzahl werden so angezeigt, wie sie sind; fehlende Zellen erscheinen leer und
  abgesetzt (schraffiert/gedämpft); wer hineinschreibt, ergänzt die nötigen Trennzeichen am Zeilenende.
- Zeile löschen entfernt den Datensatz samt seinem Zeilenende; ist es der letzte Datensatz ohne
  Zeilenende, wird das Zeilenende davor entfernt.
- Spalte löschen/einfügen/verschieben ist eine Textänderung pro Datensatz, alle in **einer** Gruppe.
- „Reihenfolge übernehmen“ ersetzt den Bereich der Datensätze (ohne Kopfzeile) durch die Datensätze in
  Ansichtsreihenfolge, jeder Datensatz **bytegleich** übernommen; ob die Datei mit Zeilenende endet,
  bleibt erhalten. Danach ist die Ansichtssortierung aus.

### Filtern und Sortieren

- Filterfeld in einer Leiste über der Tabelle (Strg+Umschalt+L oder Menü „Tabelle“; Strg+F bleibt die
  normale Suche). Filter = Teiltext, ohne Groß/Klein, über alle Spalten; optional auf eine Spalte
  beschränkbar (Auswahl neben dem Feld). Anzeige „n von m Zeilen“. Die Kopfzeile ist nie gefiltert.
- Klick auf Überschrift: aufsteigend → absteigend → aus. Vergleich: wenn alle nicht-leeren Werte der
  Spalte Zahlen sind (auch mit `,` als Dezimalzeichen), numerisch, sonst natürlich („a2 < a10“), ohne
  Groß/Klein; leere Werte ans Ende. Stabil.
- Ein Pfeil in der Überschrift zeigt die Sortierung; daneben in der Leiste der Hinweis „nur Ansicht“
  und der Knopf „Reihenfolge übernehmen“. Bei aktivem Filter ist „Reihenfolge übernehmen“
  deaktiviert (ausgeblendete Zeilen hätten keine definierte Position).
- Beim Bearbeiten in sortierter/gefilterter Ansicht bleibt die Zeile an ihrem Platz, bis der Nutzer
  erneut sortiert/filtert (kein Wegspringen beim Tippen). Eingefügte Zeilen erscheinen an der
  Einfügestelle der Ansicht.

### Suche

- Die normale Suchleiste (Strg+F) funktioniert in der Tabelle: Treffer werden im Text gesucht wie
  bisher, die Tabelle springt zur Zelle mit dem Treffer und hebt die Zelle hervor. Ersetzen arbeitet
  ebenfalls auf dem Text (wie in der Textansicht).

### Textansicht

- Für CSV/TSV-Dokumente: jede Spalte bekommt eine eigene Textfarbe aus einer kleinen Palette pro Design
  (eigene Farben, Kontrast ≥ 4,5 : 1 gegen den Hintergrund), zyklisch. Trennzeichen gedämpft.
- Statusleiste zeigt zusätzlich „Spalte 3: Mobile“ (Name aus der Kopfzeile, wenn vorhanden).

### Statusleiste in der Tabellenansicht

- Statt „Zeile, Spalte“: „Zeile 12 von 314 · Spalte 3: Mobile“, außerdem klickbare Felder
  **Trennzeichen** (Komma, Semikolon, Tab, Senkrechter Strich) und **Kopfzeile** (an/aus).
  Kodierung, Zeilenende und Sprache bleiben wie gehabt.

### Aussehen

- Gezeichnet in den Farben des aktuellen Designs (alle neun, inkl. Windows-98-Look); Kopfzeile und
  Nummernspalte bleiben beim Scrollen stehen. Spaltenbreite automatisch aus dem Inhalt der ersten ~200
  Datensätze, gedeckelt bei etwa 40 Zeichen, per Ziehen am Spaltenrand änderbar, Doppelklick auf den
  Rand = passend. Lange Inhalte werden mit „…“ abgeschnitten, Zellen mit Zeilenumbruch zeigen die
  erste Zeile plus Hinweiszeichen; die Bearbeitung zeigt alles.
- Spaltenbreiten gelten pro Tab und Sitzung (nicht in den Einstellungen).

### Drucken

- Gedruckt wird wie bisher der Text. Die Spaltenfarben der Textansicht müssen im Druck nicht erscheinen.

## Grenzen und Fehlerfälle

- Datei ohne erkennbares Trennzeichen: Tabelle zeigt eine Spalte, nichts geht kaputt.
- Nicht geschlossenes Anführungszeichen: Das Feld reicht bis zum Dateiende; die Statusleiste zeigt
  „Anführungszeichen ab Zeile N nicht geschlossen“.
- Leere Datei: Tabelle mit einer leeren Zelle; Tippen legt den ersten Datensatz an.
- Große Dateien: mit 100 000 Zeilen bedienbar (nur sichtbare Zeilen zeichnen; Neuberechnung so schnell
  oder im Hintergrund, dass Tippen nicht hakt). Ab 50 MB startet Quillpad nicht automatisch in der
  Tabelle (Umschalten bleibt möglich). Dokumente im Modus „große Datei“: keine Tabelle.

## Festlegungen nach der Durchsicht (Entwicklerentscheid)

- **Ansicht = Liste von Datensatzindizes.** Sortierung und Filter sind eine Permutation/Teilmenge der
  aktuellen Datensätze. Bei Änderungen aus der Tabelle selbst wird die Liste mitgeführt (Zelle ändern:
  unverändert; Zeile einfügen/löschen: Indizes verschoben, neue Zeile an der Einfügestelle der Ansicht).
  Bei **jeder anderen** Textänderung (Textansicht, Rückgängig/Wiederholen, Ersetzen, Neuladen) werden
  Sortierung und Filter auf den neuen Stand **neu angewandt**; die aktive Zelle wird über ihre
  Textposition wiedergefunden. Keine dauerhaften Zeilen-IDs.
- **Bei aktiver Sortierung oder aktivem Filter** sind „Zeile nach oben/unten verschieben“ gesperrt.
  „Zeile einfügen oberhalb/unterhalb“ fügt im Text direkt vor/nach dem Datensatz der aktiven Zelle ein;
  „Zeile löschen“ löscht die ausgewählten Datensätze im Text.
- **Aktive Zelle nach Rückgängig/Wiederholen:** die Zelle, in der die erste geänderte Textposition der
  Gruppe liegt (aus der Auswahl, die `undo`/`redo` zurückgibt). Gibt es sie nicht mehr, die nächstliegende.
- **Dialekt (Trennzeichen, Kopfzeile) gehört zum Dokument**, nicht zur Ansicht: Tabelle und farbige
  Textansicht desselben Dokuments nutzen denselben. Spaltenbreiten, Sortierung, Filter und Auswahl
  gehören zur Ansicht (Tab).
- **Große Dateien:** Die bestehende Schwelle „große Datei“ (Einstellung, Vorgabe 256 MB) schaltet die
  Tabelle ganz ab. Darunter gilt: ab 50 MB nicht automatisch Tabelle, Umschalten möglich.
- **Zählung:** Mit Kopfzeile ist die Kopfzeile keine Nummer; Datensätze darunter zählen ab 1. Ohne
  Kopfzeile zählt die erste Zeile als 1, und die Überschriften zeigen A, B, C … „n von m Zeilen“ und
  „Zeile 12 von 314“ zählen nur Datensätze ohne Kopfzeile.

## Übersetzungen

Alle neuen Texte in `locales/{en,de,fr}/*.toml` (neuer Abschnitt, z. B. `[table]`); der bestehende
Test prüft gleiche Schlüssel.

## Dem Umsetzer überlassen

- Modulaufteilung (Vorschlag: `src/csv/` für Parser, Dialekt, Kodierung und Tabellenoperationen als
  reine Funktionen auf `&str` → Liste von `(start, ende, ersatz)`-Änderungen; `src/ui_table.rs` für
  Zeichnen und Eingabe).
- Zeichnen mit eigenem Painter (wie der Editor) oder mit egui-Bordmitteln – ohne neue schwere
  Abhängigkeit, egui 0.31.
- Ob die zuletzt gewählte Ansicht pro Datei in der Sitzung gemerkt wird.
- Genaue Belegung im Detail, solange sie in Tooltip/Menü sichtbar ist und nichts Bestehendes
  überschreibt.

## Tests (mindestens)

- Rundlauf: Parsen ohne Änderung erzeugt keine Textänderung – für 3CX-artige Dateien (Komma, alles in
  Anführungszeichen, CRLF, UTF-8 mit BOM) und SQL-Exporte (Semikolon, teilweise Anführungszeichen,
  `NULL`, leere Felder, Felder mit Zeilenumbruch).
- Eine Zelle ändern verändert nur dieses Feld; Feld mit Trennzeichen bekommt Anführungszeichen; `"`
  wird verdoppelt; vorher gequotetes Feld bleibt gequotet.
- Zeile/Spalte einfügen, löschen, verschieben; letzte Zeile ohne Zeilenende.
- „Reihenfolge übernehmen“ bytegleich pro Datensatz; Rückgängig stellt das Original wieder her.
- Erkennung: Komma, Semikolon, Tab, Senkrechter Strich; Kopfzeile ja/nein.
- Filtern und Ansichtssortierung lassen `is_modified()` unverändert.
- Abweichende Feldanzahl, nicht geschlossenes Anführungszeichen und leere Datei stürzen nicht ab.
