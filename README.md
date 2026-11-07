# Quillpad

Ein schneller Texteditor für Windows und Linux, geschrieben in Rust mit [egui](https://github.com/emilk/egui).

## Funktionen

- Tabs, geteilte Ansicht mit gemeinsamem Scrollen, Sitzungen mit ungespeichertem Text
- Syntaxfarben für über 100 Sprachen, Gliederung, Übersicht und Projektordner
- Suchleiste mit Live-Zähler, regulären Ausdrücken und Steuerzeichen; Suchen und Ersetzen in Dateien
- Mehrere Cursor, Blockauswahl, Lesezeichen, Einklappen, Zeilenwerkzeuge, Zahlenfolgen
- Kodierungen (UTF-8, UTF-16, ANSI und weitere Zeichensätze) und Zeilenenden bleiben beim Speichern erhalten
- Neun Farbdesigns, darunter Solarized, Nord, Monokai, Sepia, Hoher Kontrast und ein Windows-98-Look
- Hinweis auf neue Versionen (abschaltbar)

## Herunterladen

Fertige Programme für Windows und Linux gibt es unter [Releases](https://github.com/RayTrunk/quillpad/releases).
Unter Linux bitte `LIESMICH-LINUX.txt` beachten.

## Selbst bauen

Rust von [rustup.rs](https://rustup.rs) installieren, dann:

```
cargo build --release
```

Das Programm liegt danach in `target/release/`. Tests laufen mit `cargo test`.
