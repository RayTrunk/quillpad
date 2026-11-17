# CSV-Tabelle – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** CSV/TSV-Dateien in Quillpad als Tabelle ansehen und bearbeiten, wobei die Datei außerhalb der geänderten Felder unverändert bleibt.

**Architecture:** Der Text im `Document` bleibt die einzige Wahrheit. Ein reiner Parser (`src/csv/`) liefert Feldbereiche in Zeichenpositionen; Tabellenaktionen sind reine Funktionen, die eine Liste von Textersetzungen erzeugen, die in **einer** Rückgängig-Gruppe ins Dokument gehen. Die Tabellenansicht (`src/ui_table.rs`) ist eine zweite Darstellung pro Tab; ihre aktive Zelle ist immer die Zelle, in der `view.sels.main().head` liegt – so folgen Suche, Rückgängig und die Textansicht automatisch.

**Tech Stack:** Rust, eframe/egui 0.31, ropey (Zeichenpositionen), bestehendes `Document::begin/splice/commit`.

**Spec:** `docs/superpowers/specs/2026-11-17-csv-tabelle-design.md`

## Global Constraints

- Unveränderte Felder bleiben **bytegleich**; Speichern schreibt die Datei nie aus einem Tabellenmodell neu.
- Filtern und Ansichtssortierung ändern den Text nie und lassen `is_modified()` unverändert.
- Jede Tabellenaktion ist genau **ein** Rückgängig-Schritt (`Document::begin(Kind::Other, …)` … `splice` … `commit`).
- Positionen sind **Zeichen**-Indizes wie in `Document` (ropey), nicht Bytes. Der Dokumenttext ist intern auf `\n` normalisiert (`fileio.rs:185`); Zeilenenden setzt das Speichern.
- Dialekt (Trennzeichen, Kopfzeile) gehört zum **Dokument**; Spaltenbreiten, Sortierung, Filter, Auswahl zur **Ansicht** (Tab).
- Keine neue schwere Abhängigkeit; egui 0.31.
- Nichts von Modern CSV oder Notepad++ übernehmen (Namen, Farben, Layout, Abläufe) – eigene Gestaltung in Quillpads neun Designs inkl. Windows-98-Look.
- Alle Texte über `t!(…)` in `locales/{en,de,fr}/*.toml`, neuer Abschnitt `[table]`; der Schlüsseltest muss grün bleiben.
- Tastenkürzel (bestehende Kürzel haben Vorrang, Spec „sofern frei“): Filter **Strg+Umschalt+L**; Zeile darunter einfügen **Strg+Eingabe**; Zeile löschen **Strg+Umschalt+Entf**. Strg+Plus/Minus bleiben Zoom. In der Tabelle haben F2 (bearbeiten), Pfeiltasten, Enter, Tab, Entf, Esc Vorrang vor den globalen Kürzeln.
- Große Dateien: `doc.large == true` → keine Tabelle; Text ≥ 50 MB → nicht automatisch Tabelle.
- Commits auf Branch `csv`, deutsche Commit-Nachricht im Stil der Historie, Autor-Mail `186974450+RayTrunk@users.noreply.github.com`, Abschluss `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. 3CX-Telefonbuch: alle Felder in `"…"`, Datei mit BOM und CRLF – Zelle ändern und speichern darf nur dieses Feld ändern (BOM/CRLF bleiben, Nachbarfelder behalten Anführungszeichen). → Test in Task 3.
2. Feld mit Zeilenumbruch in Anführungszeichen (SQL-Notizspalte): Datensatz über mehrere Textzeilen; Zeile löschen/verschieben nimmt den ganzen Datensatz mit. → Test in Task 2.
3. Letzte Zeile ohne abschließendes `\n`: Zeile anhängen, letzte Zeile löschen, „Reihenfolge übernehmen“ erzeugen weder Leerzeile noch verklebte Zeilen. → Test in Task 2.
4. Tippen in der Tabelle, während in der geteilten Ansicht dasselbe Dokument als Text offen ist: beide folgen, Rückgängig in der Textansicht setzt die aktive Zelle korrekt. → Test in Task 5.
5. F2 in der Tabelle bearbeitet die Zelle und springt **nicht** zum nächsten Lesezeichen; Strg+L in der Tabelle löscht keine Textzeile. → Test in Task 5.

---

### Task 1: Parser und Erkennung

**Files:**
- Create: `src/csv/mod.rs`, `src/csv/parse.rs`
- Modify: `src/main.rs` (`mod csv;`)
- Test: Unit-Tests in `src/csv/parse.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Dialect { pub delim: char, pub header: bool }
  #[derive(Clone, Debug, PartialEq)]
  pub struct Field { pub start: usize, pub end: usize, pub quoted: bool }   // Zeichenbereich inkl. Anführungszeichen
  #[derive(Clone, Debug, PartialEq)]
  pub struct Record { pub start: usize, pub end: usize, pub fields: Vec<Field> } // end = vor dem '\n' (exkl.)
  #[derive(Clone, Debug, Default)]
  pub struct Parsed { pub records: Vec<Record>, pub ends_with_newline: bool, pub unclosed_line: Option<usize> }
  pub fn parse(text: &str, delim: char) -> Parsed;
  pub fn field_value(text_chars: &[char], f: &Field) -> String; // ohne äußere "", "" → "
  pub fn detect(text: &str, ext: Option<&str>) -> Dialect;
  pub const DELIMS: [char; 4] = [',', ';', '\t', '|'];
  ```
  `Parsed.records` enthält auch die Kopfzeile (Index 0); ob sie Kopfzeile ist, entscheidet `Dialect.header`. Leerer Text → ein Datensatz mit einem leeren Feld. Abschließendes `\n` erzeugt **keinen** leeren Datensatz (`ends_with_newline = true`). Eine Leerzeile mitten im Text ist ein Datensatz mit einem leeren Feld.

- [ ] **Step 1: Tests schreiben**
  - `parses_quoted_fields_with_delims_and_doubled_quotes`: `"a,b","c""d",e\n` → 1 Datensatz, 3 Felder, Werte `a,b`, `c"d`, `e`; `quoted` = true,true,false; `ends_with_newline`.
  - `multiline_field_is_one_record`: `id;note\n1;"x\ny"\n2;z` → 3 Datensätze, Wert `x\ny`; `!ends_with_newline`.
  - `unclosed_quote_runs_to_end`: `a,"b\nc` → `unclosed_line == Some(0)`, kein Panic.
  - `empty_text_is_one_empty_cell`.
  - `field_ranges_cover_record_exactly`: Für mehrere Eingaben gilt: Felder lückenlos, getrennt durch genau ein Trennzeichen, erstes `start == record.start`, letztes `end == record.end`; Positionen sind Zeichen (Test mit `ä€` im Text).
  - `detects_delimiters`: Komma, Semikolon, Tab, `|` je an einem 3-zeiligen Beispiel; `.tsv`-Endung → Tab; Text ohne Trennzeichen → `,`.
  - `detects_header`: `Name;Phone\nAnna;123` → header true; `1;2\n3;4` → false; `Name;\nx;y` → false (leeres Feld).
- [ ] **Step 2:** `cargo test csv::` – schlägt fehl (Modul fehlt).
- [ ] **Step 3: Implementieren.** Ein Durchlauf über `chars()` mit Zustand (außerhalb / im Anführungszeichen / nach schließendem `"`). Erkennung über die ersten 50 Datensätze je Kandidat: gewinnt der Kandidat mit Feldanzahl > 1, der die meisten Datensätze mit der häufigsten Feldanzahl hat; Gleichstand → Reihenfolge von `DELIMS`. Kopfzeile = true, außer ein Feld der ersten Zeile ist leer oder eine Zahl (`f64` nach Ersetzen von `,` durch `.`).
- [ ] **Step 4:** `cargo test csv::` – grün.
- [ ] **Step 5:** Commit „CSV: Parser und Erkennung“.

### Task 2: Tabellenaktionen als Textersetzungen

**Files:**
- Create: `src/csv/ops.rs`
- Test: Unit-Tests in `src/csv/ops.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq)]
  pub struct Change { pub start: usize, pub end: usize, pub text: String } // Zeichenbereich im alten Text
  pub fn apply(text: &str, changes: &[Change]) -> String; // für Tests und Task 3; changes nicht überlappend, beliebige Reihenfolge
  pub fn encode(value: &str, delim: char, force_quote: bool) -> String;
  pub fn file_quotes(text: &str, p: &Parsed) -> bool; // ≥ 90 % der nicht-leeren Felder quoted
  pub fn set_cell(text: &str, p: &Parsed, d: Dialect, rec: usize, col: usize, value: &str) -> Vec<Change>;
  pub fn insert_row(text: &str, p: &Parsed, d: Dialect, at: usize) -> Vec<Change>;      // neuer leerer Datensatz wird Index `at`
  pub fn delete_rows(text: &str, p: &Parsed, recs: &[usize]) -> Vec<Change>;
  pub fn move_row(text: &str, p: &Parsed, rec: usize, up: bool) -> Vec<Change>;
  pub fn insert_col(text: &str, p: &Parsed, d: Dialect, at: usize) -> Vec<Change>;
  pub fn delete_col(text: &str, p: &Parsed, col: usize) -> Vec<Change>;
  pub fn move_col(text: &str, p: &Parsed, d: Dialect, col: usize, left: bool) -> Vec<Change>;
  pub fn apply_order(text: &str, p: &Parsed, d: Dialect, order: &[usize]) -> Vec<Change>; // order = Datensatzindizes ohne Kopfzeile, vollständige Permutation
  pub fn paste_block(text: &str, p: &Parsed, d: Dialect, rec: usize, col: usize, block: &[Vec<String>]) -> (Vec<Change>, bool /*abgeschnitten*/);
  pub fn to_tsv(values: &[Vec<String>]) -> String;     // Kopieren
  pub fn from_tsv(s: &str) -> Vec<Vec<String>>;        // Einfügen (Excel-kompatibel, \r\n toleriert)
  ```
  Regeln aus der Spec: `encode` quotet bei Trennzeichen, `"`, `\r`, `\n`, führendem/abschließendem Leerzeichen oder `force_quote`; `"`→`""`. `set_cell` nutzt `force_quote = field.quoted`; gleicher Wert → leere Liste. Zelle jenseits der Feldanzahl → Trennzeichen am Datensatzende ergänzen. Neue Felder: `force_quote = file_quotes(..) && !value.is_empty()`. Neue Zeile: Feldanzahl der Kopfzeile bzw. häufigste. Zeilen/Spalten-Operationen verschieben Datensätze/Felder **als unveränderten Originaltext**.

- [ ] **Step 1: Tests schreiben** (jeweils `apply(text, &changes)` gegen erwarteten String):
  - `set_cell_touches_only_that_field`: `"a","b","c"\n"d","e","f"\n` Zelle (1,1)=`x` → `"a","b","c"\n"d","x","f"\n`.
  - `set_cell_quotes_when_needed`: `a;b\n` Zelle (0,1)=`x;y` → `a;"x;y"\n`; Wert `say "hi"` → `a;"say ""hi"""\n`.
  - `set_cell_same_value_is_noop` → `changes.is_empty()`.
  - `set_cell_beyond_ragged_row_appends_delims`: `a,b,c\nd\n` Zelle (1,2)=`z` → `a,b,c\nd,,z\n`.
  - `insert_row_at_end_without_trailing_newline`: `h1,h2\na,b` insert at 2 → `h1,h2\na,b\n,`; mit `file_quotes` true bleiben neue leere Felder ohne `""`.
  - `delete_last_row_without_trailing_newline`: `h\na\nb` delete [2] → `h\na`; delete [1] bei `h\na\nb\n` → `h\nb\n`.
  - `move_multiline_record`: `id;n\n1;"x\ny"\n2;z\n` move_row(1, down) → `id;n\n2;z\n1;"x\ny"\n`.
  - `insert_delete_move_col`: `a,b\nc,d\n` insert_col 1 → `a,,b\nc,,d\n`; delete_col 0 → `b\nd\n`; move_col(1,left) → `b,a\nd,c\n`.
  - `apply_order_keeps_records_bytewise_and_trailing_state`: `h\n"b"\na\nc` mit order `[2,1,3]` → `h\na\n"b"\nc`; ohne abschließendes `\n` bleibt es ohne.
  - `paste_block_appends_rows_and_clips_cols`: 2×3-Block in eine 2-spaltige Datei ab (1,1) → angehängte Zeile, `truncated == true`.
  - `tsv_roundtrip`: `from_tsv(&to_tsv(v)) == v` für Werte mit Tab, `"` und `\n`.
- [ ] **Step 2:** `cargo test csv::ops` – rot.
- [ ] **Step 3: Implementieren** in `src/csv/ops.rs`; `apply` sortiert absteigend nach `start` und ersetzt auf einem `Vec<char>`.
- [ ] **Step 4:** grün.
- [ ] **Step 5:** Commit „CSV: Tabellenaktionen als Textersetzungen“.

### Task 3: Anbindung ans Dokument

**Files:**
- Create: `src/csv/doc.rs`
- Modify: `src/doc.rs` (Feld `pub csv: Option<crate::csv::DocCsv>` in `Document`, `None` in `new`), `src/app.rs` (beim Öffnen/Neuladen setzen)
- Test: `src/tests.rs`

**Interfaces:**
- Consumes: Tasks 1–2, `Document::{begin, splice, commit, full_text, rev, undo}`.
- Produces:
  ```rust
  pub struct DocCsv { pub dialect: Dialect, pub manual: bool, cache: Option<(u64 /*rev*/, Dialect, Arc<Parsed>, Arc<Vec<char>>)> }
  impl DocCsv {
      pub fn new(dialect: Dialect) -> DocCsv;
      pub fn parsed(&mut self, doc_text: &ropey::Rope, rev: u64) -> (Arc<Parsed>, Arc<Vec<char>>); // neu bei geänderter rev oder geändertem Dialekt
      pub fn set_dialect(&mut self, d: Dialect); // manual = true
  }
  pub fn apply_changes(doc: &mut Document, changes: Vec<Change>, before: &SelSet, caret_after: usize) -> bool; // false = nichts geändert; eine Gruppe Kind::Other; Ersetzungen absteigend nach start
  pub fn is_table_file(path: Option<&Path>, language: Option<&str>) -> bool; // Endung csv/tsv/tab (ohne Groß/Klein) oder Sprache "CSV"/"TSV"
  pub fn cell_at(p: &Parsed, pos: usize) -> (usize /*rec*/, usize /*col*/); // Feld, das pos enthält; hinter letztem Datensatz → letzter
  pub fn cell_pos(p: &Parsed, rec: usize, col: usize) -> usize; // Inhalt-Anfang des Feldes (hinter öffnendem ")
  ```
  `Document.csv` wird beim Öffnen gesetzt, wenn `is_table_file` und `!doc.large`: `DocCsv::new(detect(text, ext))`. Neu erkannt wird nur, solange `!manual`, und nur beim Öffnen/Neuladen (nicht bei jedem Tippen).

- [ ] **Step 1: Tests schreiben** (`Rig` aus `src/tests.rs`, Datei in `temp_dir`):
  - `csv_file_gets_dialect_on_open`: `a;b\n1;2\n` als `x.csv` öffnen → `doc.csv.dialect == Dialect{delim:';',header:true}`; `x.txt` → `None`.
  - `table_edit_is_one_undo_step`: `set_cell` + `apply_changes` → Text geändert; `doc.undo()` → Text wie vorher; `is_modified()` false.
  - `three_cx_cell_edit_saves_bytewise`: Datei-Bytes = BOM + `"FirstName","Mobile"\r\n"Anna","+49 1"\r\n"Max","+43 2"\r\n`; öffnen, Zelle (2,1) auf `+43 9` setzen, `Action::Save` → Bytes auf der Platte = Original mit nur `+43 2`→`+43 9`.
  - `noop_change_does_not_mark_modified`: `apply_changes(doc, vec![], …) == false`, `!is_modified()`.
- [ ] **Step 2:** rot. **Step 3:** implementieren. **Step 4:** `cargo test` komplett grün.
- [ ] **Step 5:** Commit „CSV: Dialekt am Dokument, Änderungen als eine Gruppe“.

### Task 4: Ansichtszustand – Sortieren und Filtern

**Files:**
- Create: `src/csv/view.rs`
- Test: Unit-Tests in `src/csv/view.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SortDir { Asc, Desc }
  #[derive(Clone, Debug, Default)]
  pub struct RowView {
      pub sort: Option<(usize /*col*/, SortDir)>,
      pub filter: String, pub filter_col: Option<usize>,
      pub rows: Vec<usize>,   // sichtbare Datensatzindizes (ohne Kopfzeile) in Anzeigereihenfolge
      built_rev: u64,
  }
  impl RowView {
      pub fn rebuild(&mut self, text: &[char], p: &Parsed, d: Dialect, rev: u64); // sortiert + filtert neu
      pub fn after_own_insert(&mut self, at_view: usize, new_rec: usize, rev: u64); // Indizes ≥ new_rec +1, neue Zeile bei at_view
      pub fn after_own_delete(&mut self, recs: &[usize], rev: u64);
      pub fn mark_own(&mut self, rev: u64);          // eigene Änderung ohne Strukturänderung (Zelle)
      pub fn needs_rebuild(&self, rev: u64) -> bool; // fremde Änderung → true
      pub fn cycle_sort(&mut self, col: usize);      // Asc → Desc → aus
      pub fn is_identity(&self) -> bool;             // keine Sortierung, kein Filter
  }
  pub fn compare_col(a: &str, b: &str, numeric: bool) -> std::cmp::Ordering;
  pub fn column_is_numeric(values: &[&str]) -> bool; // alle nicht-leeren Werte f64 (mit , oder . als Dezimalzeichen)
  ```
  Leere Werte stehen bei Asc **und** Desc am Ende. Nicht-numerisch: natürlicher Vergleich ohne Groß/Klein (`a2 < a10`). Stabil (`sort_by`). Filter: Teiltext, ohne Groß/Klein (`to_lowercase`), über alle Spalten oder `filter_col`.

- [ ] **Step 1: Tests:** `natural_order` (`a2<a10`, `B` vs `a`), `numeric_with_comma` (`2,5 < 10`), `empties_last_both_directions`, `stable_sort`, `filter_case_insensitive_and_column`, `own_insert_keeps_position_external_change_rebuilds` (nach `after_own_insert` steht die neue Zeile an `at_view` trotz Sortierung; `needs_rebuild(rev+1)` true).
- [ ] **Step 2–4:** rot → implementieren → grün.
- [ ] **Step 5:** Commit „CSV: Sortieren und Filtern als Ansicht“.

### Task 5: Tabellenansicht zeichnen und bedienen

**Files:**
- Create: `src/ui_table.rs`
- Modify: `src/editor.rs` (`View`: Feld `pub table: Option<Box<crate::ui_table::TableState>>`, `None` in `View::new`), `src/ui_chrome.rs` (an der Stelle von `editor::show` bei `view.table.is_some()` stattdessen `ui_table::show`; Umschalter Text|Tabelle; Kürzel-Vorrang), `src/app.rs` (beim Öffnen einer Tabellendatei `view.table = Some(..)` außer Text ≥ 50 MB; `Action::ToggleTable`; Ansicht-Menü), `src/main.rs`
- Test: `src/tests.rs`

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  ```rust
  pub struct TableState { pub rows: RowView, pub widths: Vec<f32>, pub scroll: egui::Vec2,
                          pub anchor: Option<(usize, usize)>, /*Rechteckauswahl: anderes Ende = aktive Zelle*/
                          pub editing: Option<String>, pub focused: bool }
  pub struct TableOut { pub focused: bool, pub clipboard: Option<String>, pub status: Option<String> }
  pub fn show(ui: &mut egui::Ui, doc: &mut Document, view: &mut View, pal: &Palette, m: &mut Metrics, retro: bool) -> TableOut;
  // Action::ToggleTable in app.rs
  ```
  `Action::ToggleTable` (Ansicht-Menü und Umschalter) funktioniert für **jedes** Dokument außer `doc.large`: hat es noch kein `doc.csv`, wird `DocCsv::new(detect(text, ext))` angelegt. Der Umschalter `Text | Tabelle` steht sichtbar über der Ansicht, wenn `doc.csv.is_some()`.
  Aktive Zelle = `cell_at(parsed, view.sels.main().head)`; Bewegen setzt `view.set_sels(SelSet::single(cell_pos(..)))`. Damit stimmen Suche (Strg+F/F3), Rückgängig/Wiederholen (gibt `SelSet` zurück) und geteilte Ansicht ohne Extra-Code. Rückgängig in der Tabelle = bestehendes `Cmd::Undo`/`Cmd::Redo` auf dem Dokument.
  Kürzel: Wenn eine Tabelle den Fokus hat, behandelt `ui_table::show` F2, Pfeile (±Umschalt), Enter, Tab, Entf, Esc, Pos1/Ende, Bild auf/ab und Zeicheneingabe selbst und konsumiert sie (`ctx.input_mut(|i| i.consume_key(..))`) **vor** der globalen Kürzelverarbeitung; `editor_only`-Kürzel (`ui_chrome.rs:374`) gelten bei Tabellenfokus nicht. Strg+Z/Strg+Y/Strg+C/Strg+V/Strg+X/Strg+A wirken auf die Tabelle.
  Zeichnen: eigener Painter, nur sichtbare Zeilen; Kopfzeile und Nummernspalte fest; Spaltenbreite aus den ersten 200 Datensätzen, max. 40 Zeichen, Rand ziehbar, Doppelklick = passend; „…“ bei Überlänge; Zelle mit `\n` zeigt erste Zeile + `⏎`; fehlende Zellen einer kurzen Zeile gedämpft. Ohne Kopfzeile Überschriften `A, B, …, Z, AA…`. Bearbeitung: `egui::TextEdit` über der Zelle (Alt+Enter fügt `\n` ein). Farben aus `Palette` bzw. Chrome des Designs.

- [ ] **Step 1: Tests (Rig):**
  - `csv_opens_in_table_and_typing_edits_cell`: `a,b\n1,2\n` als `t.csv` öffnen → `view.table.is_some()`; Pfeil runter, `typed("9")`, Enter → Text `a,b\n9,2\n`; ein Undo-Schritt.
  - `f2_edits_cell_not_bookmark` und `ctrl_l_in_table_keeps_lines` (Review Focus 5).
  - `split_view_follows_table_edit_and_undo` (Review Focus 4): `Action::CloneToOther`, rechts Text, links Tabelle; Zelle ändern → beide Views sehen Text; Undo in der Textansicht → aktive Tabellenzelle = geänderte Zelle.
  - `toggle_table_switches_mode`; `large_doc_has_no_table` (`settings.large_file_mb = 1`, 1-MB-CSV).
  - `find_in_table_selects_cell`: Strg+F nach `2` → aktive Zelle (1,1).
- [ ] **Step 2–4:** rot → implementieren → `cargo test` grün.
- [ ] **Step 5:** Sichtprüfung: Testzustand `"table"` in `apply_test_state` (öffnet ein eingebautes Beispiel als Tabelle), `--screenshot` in Hell, Monokai und Windows 98; Bilder ansehen.
- [ ] **Step 6:** Commit „Tabellenansicht für CSV“.

### Task 6: Zeilen, Spalten, Zwischenablage, Filter und „Reihenfolge übernehmen“

**Files:**
- Modify: `src/ui_table.rs`, `src/ui_chrome.rs` (Menü „Tabelle“ nur bei aktiver Tabelle, Kürzel aus Global Constraints), `src/app.rs` (Actions)
- Test: `src/tests.rs`

**Interfaces:**
- Consumes: Task 2 (`insert_row`, `delete_rows`, `move_row`, `insert_col`, `delete_col`, `move_col`, `apply_order`, `paste_block`, `to_tsv`, `from_tsv`), Task 4 (`RowView`).
- Produces: `Action::Table(TableCmd)` mit
  `enum TableCmd { RowAbove, RowBelow, DeleteRows, RowUp, RowDown, ColLeft, ColRight, DeleteCol, ColMoveLeft, ColMoveRight, ShowFilter, ApplyOrder, ClearSort }`.
  Kontextmenüs auf Zelle, Nummer und Überschrift mit denselben Befehlen. Bei `!rows.is_identity()` sind `RowUp/RowDown` deaktiviert; `ApplyOrder` nur bei aktiver Sortierung **ohne** Filter. Filterleiste über der Tabelle: Textfeld, Spaltenauswahl (alle / eine), „n von m Zeilen“, bei Sortierung „nur Ansicht“ + Knopf „Reihenfolge übernehmen“. Klick auf Überschrift = `cycle_sort`, Strg+Klick = Spalte auswählen (Tooltip nennt beides). Kopieren der Rechteckauswahl via `to_tsv`; Einfügen via `from_tsv` → `paste_block`; ein einzelner Wert in eine Mehrfachauswahl füllt alle Zellen (eine Gruppe); abgeschnitten → `TableOut.status`. Bei aktiver Sortierung/Filter fügt `RowAbove/RowBelow` im Text direkt vor/nach dem Datensatz der aktiven Zelle ein.

- [ ] **Step 1: Tests (Rig):** `row_insert_delete_via_shortcuts` (Strg+Eingabe, Strg+Umschalt+Entf), `filter_and_sort_do_not_modify` (`is_modified()` false, Text gleich), `apply_order_is_one_undo_step`, `apply_order_disabled_while_filtered`, `copy_paste_block_roundtrip` (Kopieren 2×2 → Einfügen woanders → Werte gleich), `single_value_fills_selection`.
- [ ] **Step 2–4:** rot → implementieren → grün.
- [ ] **Step 5:** Commit „Tabelle: Zeilen, Spalten, Zwischenablage, Filter“.

### Task 7: Statusleiste und farbige Spalten in der Textansicht

**Files:**
- Modify: `src/ui_chrome.rs` (`status_bar`), `src/highlight.rs` (`DocHl`: CSV-Spans statt syntect, wenn das Dokument einen CSV-Dialekt hat), `src/theme.rs` (Spaltenpalette je Design)
- Test: `src/tests.rs`, Unit-Test in `src/theme.rs`

**Interfaces:**
- Consumes: Task 1, Task 3 (`DocCsv`, `cell_at`).
- Produces:
  ```rust
  pub fn csv_columns(theme: ThemeId) -> &'static [egui::Color32]; // 6 eigene Farben je Design, jede Kontrast ≥ 4.5 zu dessen Editor-Hintergrund
  pub fn csv_line_spans(line: &[char], start_col: usize, in_quote: bool, delim: char, cols: &[Color32], dim: Color32) -> (Vec<Span>, usize /*col am Zeilenende*/, bool /*in_quote am Zeilenende*/);
  ```
  `DocHl` färbt CSV-Dokumente zeilenweise mit diesem Zustand über die vorhandenen Checkpoints (Neuberechnung ab der geänderten Zeile wie heute). Trennzeichen in `dim`.
  Statusleiste: Textansicht zusätzlich `t!("table.status_col", n, name)` („Spalte 3: Mobile“; ohne Kopfzeile nur „Spalte 3“). Tabellenansicht statt Zeile/Spalte: „Zeile 12 von 314 · Spalte 3: Mobile“; klickbare Felder Trennzeichen (Menü: Komma, Semikolon, Tab, Senkrechter Strich → `DocCsv::set_dialect`) und Kopfzeile (an/aus); Hinweis `table.unclosed` bei `unclosed_line`; `TableOut.status` als vorübergehender Hinweis.

- [ ] **Step 1: Tests:** `csv_palette_contrast` (alle Designs, alle Farben ≥ 4.5 mit `theme::contrast`), `csv_spans_carry_quote_state` (Zeile 1 endet in offenem Feld → Zeile 2 beginnt `in_quote` in derselben Spalte), `switching_delimiter_reparses` (Statusleisten-Aktion setzt `;`→`,`, Tabelle zeigt andere Spaltenzahl, Text unverändert).
- [ ] **Step 2–4:** rot → implementieren → grün.
- [ ] **Step 5:** Commit „CSV: Statusleiste und Spaltenfarben“.

### Task 8: Übersetzungen, Doku, Abschlussprüfung

**Files:**
- Modify: `locales/{en,de,fr}/*.toml` (alle `table.*`-Schlüssel vollständig in drei Sprachen), `README.md`, `README.de.md`, `README.fr.md` (ein Funktionspunkt „CSV als Tabelle“), `site/index.html` (eine Karte „CSV tables“)
- Test: bestehender Locale-Schlüsseltest; `cargo test`; Release-Build Windows; WSL-Build (`set -eo pipefail; . ~/.cargo/env; cargo build --release`) ohne neue Warnungen in den neuen Dateien.

- [ ] **Step 1:** Übersetzungen prüfen/ergänzen; `cargo test` grün.
- [ ] **Step 2:** READMEs und Webseite ergänzen (englisch/deutsch/französisch, eigene Formulierungen).
- [ ] **Step 3:** Screenshots der Tabelle (Hell, Dunkel, Windows 98) über `--state table` prüfen.
- [ ] **Step 4:** Commit „CSV: Übersetzungen und Doku“.
