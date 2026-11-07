Quillpad on Linux (64-bit)

Install (for your own user, no root rights needed):
1. Unpack the archive:  tar xzf quillpad-<version>-linux-x64.tar.gz
2. In the unpacked folder:  ./install.sh
   Quillpad then appears in the application menu and under "Open with" for text files.
   The program is installed to ~/.local/bin/quillpad.
Remove:  ./install.sh --uninstall  (your settings in ~/.config/Quillpad are kept)

Without installing:  ./quillpad-linux  (or  ./quillpad-linux my-file.txt)

Quillpad needs the graphics libraries every Linux desktop ships with
(libGL/libEGL, libxkbcommon, libwayland under Wayland). If they are missing, on Debian/Ubuntu:
  sudo apt install libgl1 libegl1 libxkbcommon0 libxkbcommon-x11-0 libxcursor1 libxi6 libwayland-client0 libwayland-egl1

The "Open" and "Save" dialogs need xdg-desktop-portal or zenity.

The ready-made program needs glibc 2.39 or newer, for example
Ubuntu 24.04, Debian 13, Fedora 40 or newer. On older systems, build it yourself
in the app folder (install Rust from rustup.rs first):  cargo build --release
