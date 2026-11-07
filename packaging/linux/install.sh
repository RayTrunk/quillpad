#!/bin/sh
# Richtet Quillpad für den aktuellen Benutzer ein, ohne Root-Rechte:
# Programm nach ~/.local/bin, Eintrag im Anwendungsmenü, Symbol und „Öffnen mit“ für Textdateien.
# Entfernen:  ./install.sh --uninstall
set -e

here=$(cd "$(dirname "$0")" && pwd)
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin"
apps="$data/applications"
icons="$data/icons/hicolor/256x256/apps"

refresh() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$data/icons/hicolor" || true
}

if [ "$1" = "--uninstall" ]; then
    rm -f "$bin/quillpad" "$apps/quillpad.desktop" "$icons/quillpad.png"
    refresh
    echo "Quillpad wurde entfernt. Die Einstellungen liegen weiter in ${XDG_CONFIG_HOME:-$HOME/.config}/Quillpad."
    exit 0
fi

mkdir -p "$bin" "$apps" "$icons"
install -m 755 "$here/quillpad-linux" "$bin/quillpad"
install -m 644 "$here/quillpad.png" "$icons/quillpad.png"
# Voller Pfad, damit der Menüeintrag auch ohne ~/.local/bin im PATH funktioniert.
sed "s|^Exec=.*|Exec=$bin/quillpad %F|" "$here/quillpad.desktop" > "$apps/quillpad.desktop"
chmod 644 "$apps/quillpad.desktop"
refresh

echo "Quillpad ist installiert: im Anwendungsmenü und unter „Öffnen mit“ für Textdateien."
case ":$PATH:" in
    *":$bin:"*) echo "Start im Terminal:  quillpad datei.txt" ;;
    *) echo "Hinweis: $bin ist nicht im PATH. Start im Terminal dann mit:  $bin/quillpad" ;;
esac
