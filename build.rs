//! Unter Windows: Programmsymbol und Versionsangaben in die exe einbetten,
//! damit Explorer, Startmenü und Installer sie anzeigen.
fn main() {
    println!("cargo:rerun-if-changed=assets/quillpad.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/quillpad.ico");
    res.set("ProductName", "Quillpad");
    res.set("FileDescription", "Quillpad Texteditor");
    res.set("CompanyName", "R. Trunk");
    res.set("LegalCopyright", "© 2026 R. Trunk – GNU GPL v3 oder neuer");
    if let Err(e) = res.compile() {
        // Ohne Ressourcen-Compiler läuft das Programm trotzdem, nur ohne Symbol in der exe.
        println!("cargo:warning=Programmsymbol nicht eingebettet: {e}");
    }
}
