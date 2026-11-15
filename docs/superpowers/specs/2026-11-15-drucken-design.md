# Drucken direkt aus Quillpad – Design

Stand: 2026-11-15 · Status: Entwurf zur Freigabe

## Absicht

Heute schreibt Quillpad beim Drucken eine HTML-Datei in den Temp-Ordner und öffnet sie im
Browser (`print_preview` / `print_html` in `src/app.rs`). Das ist umständlich. Außerdem kommen die
Farben aus dem aktiven Design: Mit Dark oder Monokai landet helle Schrift auf weißem Papier.

Der Nutzer druckt meist **Code mit Syntaxfarben**, daneben **Notizen und andere Texte**.
Er will direkt aus der App drucken, ohne Browser, und vorher in Quillpad sehen, wie die
Seiten aussehen (Umbruch, Farben, Seitenzahl).

## Ziele

- `Strg+P`, Menü „Drucken…“ und das Druck-Symbol öffnen ein **Druckfenster mit Vorschau** in
  Quillpad. Es sieht unter Windows und Linux gleich aus.
- Gedruckt wird **direkt**: unter Windows über GDI, unter Linux als intern erzeugte PDF, die
  an `lp` (CUPS) geht.
- **„Als PDF…“** speichert dasselbe Ergebnis als PDF-Datei (beide Systeme).
- Papier ist immer weiß. Farben kommen aus einem **festen Papier-Farbschema**, unabhängig vom
  Bildschirmdesign.
- Vorschau und Druck stimmen überein: gleiche Umbrüche, gleiche Seiten, gleiche Seitenzahl.
- Die Druckoptionen bleiben gespeichert.

## Nicht-Ziele

- macOS
- Hintergrundfarben, Markierungen, Suchtreffer oder Lesezeichen auf dem Papier
- Skalieren auf Seitenbreite, mehrere Seiten pro Blatt, Wasserzeichen
- mehrere Tabs in einem Druckauftrag
- Bearbeiten in der Vorschau
- eigene Kopf- und Fußzeilen mit Platzhaltern (Text frei einstellbar)

## Anti-Ziele (wäre ein Fehlschlag, auch wenn es technisch läuft)

- Die Vorschau zeigt etwas anderes als das Papier.
- Quillpad friert beim Drucken oder beim Aufbau der Vorschau ein. Nach dem Wayland-Problem
  (Hyprland meldet „nicht reagierend“) darf die Oberfläche nie blockieren.
- Dunkle Flächen oder sehr helle Schrift auf dem Papier
- schwere Abhängigkeiten wie GTK oder Qt unter Linux, oder ein externer PDF-Betrachter als
  Voraussetzung
- Übernahmen von Notepad++ (Werte, Namen, Farben, Layout)

## Rahmenbedingungen

- Rust, eframe/egui 0.31, ein einzelnes Programm ohne Laufzeitabhängigkeiten. Unter Linux
  gibt es keine GTK-Druckbibliotheken. CUPS (`lp`, `lpstat`) ist auf praktisch jedem Desktop
  vorhanden, wird aber nur zur Laufzeit gebraucht.
- Unter WSL werden keine Grafikbibliotheken installiert. Der Linux-Build darf also nichts
  Neues linken.
- Texte in drei Sprachen (`locales/{de,en,fr}`). Ein Test prüft, dass alle Sprachen dieselben
  Schlüssel haben.
- Vorhandene Bausteine:
  - Syntaxfarben je Zeile: `d.hl.ensure(...)`, `d.hl.spans(l)` mit `color` und `bold`
  - Systemschriften: `font_index()` / `system_font()` in `ui_chrome.rs`
  - `ab_glyph`, `chrono`, `rfd`
- Fenster in der App folgen dem Muster der Einstellungen (`egui::Window` mit
  `theme::window_title`, `ui_panels.rs`).

## Ansätze

| | Ansatz | Abwägung |
|---|---|---|
| **A (gewählt)** | Eigener Seitenaufbau. Ausgabe unter Windows nativ über GDI, unter Linux als eigene PDF an `lp`. | Eine Logik für Umbruch, Seiten und Farben, deshalb deckt sich die Vorschau mit dem Ergebnis. Keine schweren Abhängigkeiten. Dafür muss Quillpad zwei kleine Ausgabewege pflegen. |
| B | Überall PDF erzeugen und unter Windows die PDF drucken | Windows kann PDF nicht ohne fremdes Programm drucken. Damit wäre wieder ein Umweg nötig, verworfen. |
| C | Mit egui Bilder der Seiten rendern und diese drucken | Große Spooldateien, unscharfer Text, kein durchsuchbares PDF. Verworfen. |

Der Nutzer hat **A** gewählt. Für das Druckfenster hat er **Variante 1, mit Vorschau**,
gewählt. Die Alternative war ein Druck ohne Vorschau, die Optionen hätten dann in den
Einstellungen gestanden.

## Vom Nutzer entschieden

- Weg A: gemeinsamer Seitenaufbau, Windows über GDI, Linux über PDF und `lp`, „Als PDF“ als
  Nebenprodukt.
- Ein eigenes Druckfenster mit Vorschau, auf beiden Systemen gleich (Skizze unten).
- Ohne Widerspruch übernommen: festes Papier-Farbschema, lange Zeilen werden umbrochen,
  Papierformat vom Drucker, Einstellungen werden gespeichert.

## Das Druckfenster

```
┌ Drucken ─────────────────────────────────────────────┐
│ ┌──────────────┐  Drucker:  [ HP LaserJet       ▾ ] │
│ │ backup.sh  1/3│  Kopien:   [1]   Seiten: (•) alle  │
│ │ 1 #!/usr/bin… │                    ( ) [2] bis [3]│
│ │ 2 # Copies th │                    ( ) Markierung │
│ │ 3 set -euo …  │  [✓] Farben   [✓] Zeilennummern   │
│ │   …           │  [✓] Kopf- und Fußzeile           │
│ │   Seite 1 / 3 │  Schriftgröße: [10] pt             │
│ └──────────────┘  Ränder: [15] mm                    │
│                    Papier: [A4 ▾]  (nur Linux)        │
│                    [Eigenschaften…] (nur Windows)     │
│                    [ ] Beidseitig   (nur Linux)       │
│   ◀  1 / 3  ▶      [Als PDF…] [Abbrechen] [Drucken]  │
└──────────────────────────────────────────────────────┘
```

- **Links** steht die Vorschau einer Seite als weißes Blatt im richtigen Seitenverhältnis.
  Darunter wird geblättert (◀ ▶, Bild-auf/ab, Mausrad). Jede Änderung einer Option baut die
  Vorschau sofort neu auf.
- **Drucker:**
  - Windows: Liste der installierten Drucker, voreingestellt ist der Standarddrucker.
    „Eigenschaften…“ öffnet den Treiberdialog des Druckers (Duplex, Fach, Papier). Das Papier
    kommt dann aus diesen Einstellungen.
  - Linux: Liste aus `lpstat -e`, Standard aus `lpstat -d`. Das Papier ist anfangs das
    Standardformat des Druckers und lässt sich auf A4, Letter, A5 oder Legal ändern.
    *(Entscheidung des Entwicklers:)* Das Standardformat liest Quillpad aus
    `lpoptions -p <drucker> -l`, Zeile `PageSize`, Eintrag mit `*`. Gelingt das nicht, gilt
    A4, bei Systemsprache en_US Letter. Die Ränder bleiben bei den eingestellten 15 mm
    (mindestens 5 mm). Die PDF hat genau das gewählte Format und geht mit
    `-o media=<format>` an `lp`. So skaliert CUPS nichts.
    „Beidseitig“ wird zu `-o sides=two-sided-long-edge`.
  - Findet Quillpad keinen Drucker, sind „Drucken“ und die Liste gesperrt. Ein Hinweis
    erscheint, „Als PDF…“ funktioniert weiterhin.
- **Seiten:** alle, ein Bereich von–bis, oder die **Markierung**. „Markierung“ ist nur wählbar,
  wenn Text markiert ist. Dann wird nur der markierte Text neu umbrochen und paginiert.
  Kopf- und Fußzeile bleiben.
  *(Entscheidung des Entwicklers:)*
  - „Markierung“ ist nur bei **genau einer** zusammenhängenden Auswahl wählbar.
  - Bei mehreren Cursorn oder einer Rechteckauswahl ist der Punkt gesperrt.
  - Beginnt die Auswahl mitten in einer Zeile, beginnt der Druck dort, eingerückt wie in
    der Zeile. Die Zeilennummer bleibt die aus dem Dokument.
- **Farben aus:** Alles wird schwarz gedruckt. Fett bleibt fett, Kommentare werden dunkelgrau.
- **Kopien:** 1 bis 99.
- **Drucken** startet den Auftrag und schließt das Fenster. Ein Fehler erscheint als Meldung,
  ebenso die Bestätigung „An <Drucker> gesendet“.
- `Esc` oder „Abbrechen“ schließen das Fenster.

## Seitenaufbau (gemeinsamer Kern)

Ein neues Modul `src/print/` mit einem plattformunabhängigen Kern:

- **Eingabe:**
  - die Zeilen des Dokuments (oder der Markierung) mit den Farbabschnitten aus `d.hl.spans`
  - Tabulatorbreite und Dateiname
  - die Druckoptionen
  - Papiergröße und bedruckbarer Bereich in Punkt (1/72 Zoll)
  - die Zeichenbreiten der Druckschrift
- **Ausgabe:** eine Liste von Seiten. Jede Seite enthält positionierte Textstücke (x, y in
  Punkt, Text, Farbe, fett) sowie Kopf- und Fußzeile. Vorschau, GDI und PDF zeichnen nur diese
  Liste und rechnen selbst nichts.
- **Umbruch:**
  - Lange Zeilen brechen an der rechten Kante um, bevorzugt nach Leerzeichen, sonst hart.
  - Fortsetzungszeilen beginnen ohne Zeilennummer und eingerückt auf Höhe des Textes.
  - Tabs werden mit der eingestellten Tabbreite aufgelöst.
  - Zeichen, die breiter als eine Zelle sind (z. B. CJK), zählen mit ihrer echten Breite aus
    der Schrift.
- **Zeilennummern:** rechtsbündig, so breit wie die größte Nummer, grau. Es gilt die Nummer
  im Dokument, auch beim Druck einer Markierung.
- **Kopfzeile:** links der Dateiname (oder der Tab-Titel bei unbenannten Dokumenten), rechts
  das Datum im Format der Oberflächensprache. Darunter eine dünne graue Linie.
- **Fußzeile:** mittig „Seite x von n“, übersetzt.
- **Voreinstellungen:**
  - Schrift 10 pt, Zeilenabstand 1,2 × Schriftgröße
  - Ränder 15 mm auf allen Seiten, mindestens aber der vom Drucker gemeldete nicht
    bedruckbare Rand
  - Zeilennummern, Kopf- und Fußzeile und Farben eingeschaltet
- **Eingeklappte Bereiche** werden vollständig gedruckt.
- **Sehr große Dateien:** Ab 200 000 Zeilen wird ohne Farben gedruckt, wie heute. Aufbau und
  Ausgabe laufen in einem Hintergrund-Thread. Das Fenster zeigt solange „Seiten werden
  berechnet…“ und bleibt bedienbar.

## Papier-Farbschema

*(Entscheidung des Entwicklers.)* `d.hl.spans` enthält bereits fertige Farben des aktiven
Designs, aber nicht die Textart. Deshalb hebt der Druck den Text **ein zweites Mal** hervor:

- Dafür gibt es eine eigene `Highlighter`-Instanz mit zwei zusätzlichen syntect-Themes:
  „Papier farbig“ und „Papier schwarzweiß“.
- Gearbeitet wird auf einer Kopie des Rope (billig) im Hintergrund-Thread.
- `Span`, der Cache und der Zeichenpfad des Editors bleiben unverändert.
- Im Schwarzweiß-Theme ist alles schwarz, Kommentare sind #666666, Fett bleibt.

Ein fester Farbsatz, nur für den Druck, mit der Funktion `theme::paper_syntax()`:

- Grundlage sind die Syntaxfarben des Designs „Hell“ (`ThemeId::Light`).
- Normaler Text ist reines Schwarz.
- Jede Farbe hat auf Weiß einen Kontrast von mindestens 4,5 : 1. Zu helle Farben werden
  abgedunkelt, ein Test prüft das.
- Zeilennummern, die Kopflinie und Kommentare im Schwarzweiß-Modus: Grau #666666.

## Schrift

- Gedruckt wird in der Editorschrift (`settings.font`), wenn Quillpad sie als TrueType-Datei
  findet (`font_index()`). Sonst gilt unter Windows Consolas, unter Linux das Ergebnis von
  `fc-match -f '%{file}' monospace`, zuletzt DejaVu Sans Mono.
- Fett nimmt die Fett-Variante derselben Familie, falls vorhanden. Andernfalls erzeugt
  Quillpad künstliches Fett (in der PDF Füllen und Umranden, in GDI eine Schrift mit
  `FW_BOLD`).
- Zeichen, die der Druckschrift fehlen, kommen aus den Ersatzschriften des Editors (`sym`,
  `cjk`).
- Die Breiten für den Umbruch kommen über `ab_glyph` aus derselben Datei, die auch in die PDF
  eingebettet bzw. in GDI ausgewählt wird.

## Ausgabe Windows (GDI)

- Neue Abhängigkeit, nur unter Windows: das `windows`-Crate mit den Features
  `Win32_Graphics_Gdi`, `Win32_Graphics_Printing`, `Win32_Storage_Xps` und
  `Win32_UI_WindowsAndMessaging`.
- Drucker auflisten mit `EnumPrintersW`, Standarddrucker mit `GetDefaultPrinterW`.
- „Eigenschaften…“ ruft `DocumentPropertiesW` mit Dialog auf. Die geänderte `DEVMODE`
  behält Quillpad für diesen Auftrag.
- Papiergröße und bedruckbarer Bereich kommen aus `GetDeviceCaps` (`PHYSICALWIDTH`,
  `PHYSICALOFFSETX` …) eines Informationskontexts. Ändert sich das Papier, wird neu
  paginiert.
- Ablauf: `CreateDCW` → `StartDocW` → je Seite `StartPage`, Textstücke mit `ExtTextOutW`,
  feste Zeichenabstände über `lpDx` (dadurch deckt sich der Umbruch exakt), `EndPage` →
  `EndDoc`. Kopien über `DEVMODE.dmCopies`.
- Der Auftrag läuft in einem eigenen Thread. Der Dialog von „Eigenschaften…“ ist modal zum
  Hauptfenster.

## Ausgabe PDF (Linux-Druck und „Als PDF…“)

- Neue Abhängigkeiten: `pdf-writer` (schreibt PDF) und `subsetter` (bettet nur die benutzten
  Zeichen ein).
- Schrift als CIDFont Type 2 mit Identity-H und ToUnicode, damit sich Text aus der PDF
  kopieren und durchsuchen lässt. Titel der PDF ist der Dateiname.
- Linux-Druck: PDF in den Temp-Ordner schreiben, dann
  `lp -d <drucker> -n <kopien> [-o sides=…] [-o media=…] -t <dateiname> <datei.pdf>`.
  Die Temp-Datei wird danach gelöscht. Der Exit-Code und stderr von `lp` werden als Meldung
  gezeigt, wenn es schiefgeht. Fehlt `lp` ganz, kommt eine eigene Meldung mit dem Hinweis auf
  CUPS und „Als PDF…“.
- „Als PDF…“ öffnet den Speichern-Dialog (`rfd`), vorgeschlagen wird `<dateiname>.pdf`.
  Beim PDF gilt das Papierformat aus dem Fenster bzw. vom gewählten Drucker.

## Einstellungen

Neues Feld `print: PrintSettings` in `Settings`, mit `#[serde(default)]`:

- `font_size_pt` (10)
- `margin_mm` (15)
- `line_numbers`, `header_footer`, `color` (alle `true`)
- `printer: Option<String>` (zuletzt gewählter Drucker, sonst Standard)
- `paper: Option<String>` (nur Linux; leer heißt „vom Drucker“)
- `duplex` (`false`)

Kopien und Seitenbereich werden nicht gespeichert.

## Was wegfällt

`print_preview`, `print_html`, der Test `print_page_contains_escaped_colored_text` und der
Schlüssel `msg.print_failed` mit seinem Browser-Text werden ersetzt.

## Prüfung

- **Unit-Tests für den Kern:**
  - Umbruch an Leerzeichen und hart
  - Tabs
  - Fortsetzungszeilen ohne Nummer
  - Seitenumbruch und Seitenzahl bei bekannter Papiergröße
  - Seitenbereich und Markierung
  - Schwarzweiß-Modus
  - Kontrast des Papier-Farbschemas
- **PDF-Test:** Die erzeugte Datei beginnt mit `%PDF` und hat die erwartete Seitenzahl. Der
  Text ist über die ToUnicode-Tabelle wiederherstellbar (z. B. mit `lopdf` als
  Dev-Abhängigkeit).
- **Screenshot-Zustand `print`** für `--state`, der das Druckfenster mit Vorschau zeigt
  (Design hell und dunkel).
- **Von Hand:**
  - Windows: eine Seite auf einen echten Drucker und auf „Microsoft Print to PDF“
  - Linux (Omarchy): `lp` auf einen CUPS-Drucker oder einen PDF-Drucker

## Dem Umsetzer überlassen

- die genaue Aufteilung in Dateien unter `src/print/`
- ob die Vorschau gecachte Seiten als Texturen zeichnet oder jedes Bild neu malt
- die Lösung, wenn eine Ersatzschrift mehrere Glyphen auf eine Zelle abbildet
- wie das Hintergrund-Ergebnis zurück in den Frame kommt (Kanal, wie bei `poll_find_job`)
- die Wortlaute der neuen Übersetzungsschlüssel, in allen drei Sprachen
