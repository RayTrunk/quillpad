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

## Herunterladen und installieren

Fertige Programme gibt es unter [Releases](https://github.com/RayTrunk/quillpad/releases).

- **Windows:** `quillpad-<version>-windows-x64-setup.exe` installiert Quillpad ohne Administratorrechte
  (wahlweise für alle Benutzer) mit Startmenü-Eintrag, „Öffnen mit“ für Textdateien und auf Wunsch dem
  Kontextmenü „Mit Quillpad bearbeiten“. Wer nichts installieren will, nimmt die einzelne `.exe`.
- **Linux:** `quillpad-<version>-linux-x64.tar.gz` entpacken und `./install.sh` ausführen; Quillpad
  erscheint dann im Anwendungsmenü und unter „Öffnen mit“. Details in `LIESMICH-LINUX.txt`.

Die Programme sind nicht signiert; Windows SmartScreen fragt deshalb beim ersten Start nach.

Den Installer selbst bauen: `cargo build --release`, dann mit [Inno Setup](https://jrsoftware.org/isinfo.php)
`ISCC packaging\windows\quillpad.iss`.

## Selbst bauen

Rust von [rustup.rs](https://rustup.rs) installieren, dann:

```
cargo build --release
```

Das Programm liegt danach in `target/release/`. Tests laufen mit `cargo test`.
