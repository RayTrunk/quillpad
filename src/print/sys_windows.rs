//! Windows: Druckerliste, Treiberdialog (DEVMODE je Drucker) und Papier aus dem
//! Treiber. Die Ausgabe selbst steht in `gdi`.
use super::font::PrintFont;
use super::{default_paper, Geom, Page, Paper, Printer, PAPERS};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER, HANDLE, HWND};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Printing::{
    ClosePrinter, DocumentPropertiesW, EnumPrintersW, GetDefaultPrinterW, OpenPrinterW, PRINTER_ENUM_CONNECTIONS,
    PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
};
use windows::Win32::UI::WindowsAndMessaging::IDOK;

mod gdi;

/// Fehlertext mit API-Name und `GetLastError`.
fn api_err(api: &str) -> String {
    // SAFETY: GetLastError liest nur den Fehlerwert des Threads.
    format!("{api}: Fehler {}", unsafe { GetLastError() }.0)
}

/// Nullterminierte UTF-16-Zeichenkette.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(b: &[u16]) -> String {
    String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())])
}

/// DEVMODE je Drucker aus dem Treiberdialog (Rohbytes samt Treiberdaten).
fn cache() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// DEVMODE-Puffer, 8-Byte-ausgerichtet und mindestens so groß wie `DEVMODEW`.
struct DevMode {
    buf: Vec<u64>,
    len: usize,
}

impl DevMode {
    fn zeroed(len: usize) -> DevMode {
        let words = len.max(std::mem::size_of::<DEVMODEW>()).div_ceil(8);
        DevMode { buf: vec![0; words], len }
    }
    fn from_bytes(b: &[u8]) -> DevMode {
        let mut d = DevMode::zeroed(b.len());
        // SAFETY: Der Puffer fasst mindestens `b.len()` Bytes, die Bereiche überlappen nicht.
        unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), d.buf.as_mut_ptr() as *mut u8, b.len()) };
        d
    }
    fn bytes(&self) -> Vec<u8> {
        // SAFETY: `len` Bytes liegen im Puffer.
        unsafe { std::slice::from_raw_parts(self.buf.as_ptr() as *const u8, self.len) }.to_vec()
    }
    fn ptr(&self) -> *const DEVMODEW {
        self.buf.as_ptr() as *const DEVMODEW
    }
    fn ptr_mut(&mut self) -> *mut DEVMODEW {
        self.buf.as_mut_ptr() as *mut DEVMODEW
    }
    fn dm(&mut self) -> &mut DEVMODEW {
        // SAFETY: Puffer ausgerichtet und groß genug für DEVMODEW, exklusiv geliehen.
        unsafe { &mut *self.ptr_mut() }
    }
}

/// Offener Drucker, wird beim Verlassen geschlossen.
struct PrinterHandle(HANDLE);

impl PrinterHandle {
    fn open(name: &[u16]) -> Result<PrinterHandle, String> {
        let mut h = HANDLE::default();
        // SAFETY: `name` ist nullterminiert, `h` gültiger Ausgabezeiger.
        unsafe { OpenPrinterW(PCWSTR(name.as_ptr()), &mut h, None) }.map_err(|e| format!("OpenPrinterW: {e}"))?;
        Ok(PrinterHandle(h))
    }
}

impl Drop for PrinterHandle {
    fn drop(&mut self) {
        // SAFETY: Handle stammt aus OpenPrinterW und wird nur hier geschlossen.
        let _ = unsafe { ClosePrinter(self.0) };
    }
}

/// Größe der DEVMODE des Treibers in Bytes.
fn devmode_size(h: &PrinterHandle, name: &[u16]) -> Result<usize, String> {
    // SAFETY: Größenabfrage ohne Puffer (fmode 0).
    let size = unsafe { DocumentPropertiesW(HWND::default(), h.0, PCWSTR(name.as_ptr()), None, None, 0) };
    if size <= 0 {
        return Err(api_err("DocumentPropertiesW"));
    }
    Ok(size as usize)
}

/// Gemerkte DEVMODE des Druckers, sonst die Vorgabe des Treibers.
fn devmode(printer: &str) -> Result<DevMode, String> {
    if let Some(b) = cache().lock().ok().and_then(|c| c.get(printer).cloned()) {
        return Ok(DevMode::from_bytes(&b));
    }
    let name = wide(printer);
    let h = PrinterHandle::open(&name)?;
    let mut dm = DevMode::zeroed(devmode_size(&h, &name)?);
    // SAFETY: Ausgabepuffer hat die vom Treiber genannte Größe.
    let r = unsafe { DocumentPropertiesW(HWND::default(), h.0, PCWSTR(name.as_ptr()), Some(dm.ptr_mut()), None, DM_OUT_BUFFER.0) };
    if r != IDOK.0 {
        return Err(api_err("DocumentPropertiesW"));
    }
    Ok(dm)
}

pub fn printers() -> Result<Vec<Printer>, String> {
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let (mut needed, mut count) = (0u32, 0u32);
    // SAFETY: Größenabfrage ohne Puffer.
    match unsafe { EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut count) } {
        Ok(()) => return Ok(Vec::new()),
        Err(e) if e.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult() => {}
        Err(e) => return Err(format!("EnumPrintersW: {e}")),
    }
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: Der u64-Puffer fasst `needed` Bytes und ist für PRINTER_INFO_4W ausgerichtet.
    let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, needed as usize) };
    // SAFETY: Puffer in der verlangten Größe.
    unsafe { EnumPrintersW(flags, PCWSTR::null(), 4, Some(bytes), &mut needed, &mut count) }
        .map_err(|e| format!("EnumPrintersW: {e}"))?;
    // SAFETY: Die API hat `count` Einträge an den Pufferanfang geschrieben.
    let infos = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_4W, count as usize) };
    let default = default_printer();
    Ok(infos
        .iter()
        // SAFETY: pPrinterName zeigt auf eine nullterminierte Zeichenkette im Puffer.
        .filter_map(|i| unsafe { i.pPrinterName.to_string() }.ok())
        .map(|name| Printer { is_default: default.as_deref() == Some(name.as_str()), name })
        .collect())
}

fn default_printer() -> Option<String> {
    let mut n = 0u32;
    // SAFETY: Größenabfrage mit Nullpuffer.
    let _ = unsafe { GetDefaultPrinterW(PWSTR::null(), &mut n) };
    if n == 0 {
        return None;
    }
    let mut b = vec![0u16; n as usize];
    // SAFETY: Puffer fasst `n` Zeichen.
    unsafe { GetDefaultPrinterW(PWSTR(b.as_mut_ptr()), &mut n) }.as_bool().then(|| from_wide(&b))
}

/// Treiberdialog; bei OK merkt sich das Programm die Einstellungen für diesen Drucker.
pub fn properties(printer: &str, parent: isize) -> bool {
    let name = wide(printer);
    let (Ok(h), Ok(input)) = (PrinterHandle::open(&name), devmode(printer)) else {
        return false;
    };
    let Ok(size) = devmode_size(&h, &name) else {
        return false;
    };
    let mut out = DevMode::zeroed(size);
    let mode = DM_IN_BUFFER.0 | DM_OUT_BUFFER.0 | DM_IN_PROMPT.0;
    // SAFETY: Eingabe ist eine DEVMODE dieses Treibers, Ausgabe hat die verlangte Größe; `parent` ist ein Fenster oder 0.
    let r = unsafe {
        DocumentPropertiesW(HWND(parent as _), h.0, PCWSTR(name.as_ptr()), Some(out.ptr_mut()), Some(input.ptr()), mode)
    };
    if r != IDOK.0 {
        return false;
    }
    if let Ok(mut c) = cache().lock() {
        c.insert(printer.to_string(), out.bytes());
    }
    true
}

/// Werte des Geräts in Punkt: Papiergröße und nicht bedruckbare Ränder.
struct Caps {
    dpi_x: f32,
    dpi_y: f32,
    off_x: f32,
    off_y: f32,
}

fn caps(dc: HDC) -> Caps {
    // SAFETY: `dc` ist ein gültiger Geräte- oder Informationskontext.
    let c = |i| unsafe { GetDeviceCaps(dc, i) } as f32;
    Caps { dpi_x: c(LOGPIXELSX).max(1.0), dpi_y: c(LOGPIXELSY).max(1.0), off_x: c(PHYSICALOFFSETX), off_y: c(PHYSICALOFFSETY) }
}

/// Papier aus dem Treiber; `choice` gilt unter Windows nicht.
pub fn paper_for(printer: &str, _choice: Option<&str>) -> Paper {
    query_paper(printer).unwrap_or_else(|_| default_paper())
}

fn query_paper(printer: &str) -> Result<Paper, String> {
    let mut dm = devmode(printer)?;
    let name = wide(printer);
    // SAFETY: Name nullterminiert, DEVMODE vom Treiber.
    let ic = unsafe { CreateICW(PCWSTR::null(), PCWSTR(name.as_ptr()), PCWSTR::null(), Some(dm.ptr())) };
    if ic.is_invalid() {
        return Err(api_err("CreateICW"));
    }
    let k = caps(ic);
    // SAFETY: `ic` ist gültig.
    let c = |i| unsafe { GetDeviceCaps(ic, i) } as f32;
    let (pw, ph, hr, vr) = (c(PHYSICALWIDTH), c(PHYSICALHEIGHT), c(HORZRES), c(VERTRES));
    // SAFETY: Kontext aus CreateICW, danach nicht mehr benutzt.
    unsafe {
        let _ = DeleteDC(ic);
    }
    let (sx, sy) = (72.0 / k.dpi_x, 72.0 / k.dpi_y);
    let g = Geom {
        w: pw * sx,
        h: ph * sy,
        left: k.off_x * sx,
        top: k.off_y * sy,
        right: (pw - k.off_x - hr).max(0.0) * sx,
        bottom: (ph - k.off_y - vr).max(0.0) * sy,
    };
    let form = if dm.dm().dmFields.0 & DM_FORMNAME.0 != 0 { from_wide(&dm.dm().dmFormName) } else { String::new() };
    let name = if !form.is_empty() {
        form
    } else if let Some(p) = PAPERS.iter().find(|p| (p.1 - g.w).abs() < 3.0 && (p.2 - g.h).abs() < 3.0) {
        p.0.to_string()
    } else {
        format!("{:.0} × {:.0} mm", g.w / super::MM, g.h / super::MM)
    };
    Ok(Paper { name, geom_full: g })
}

#[allow(clippy::too_many_arguments)]
pub fn print_pages(
    printer: &str,
    pages: &[Page],
    g: &Geom,
    font: &PrintFont,
    title: &str,
    copies: u32,
    duplex: bool,
    _paper: &Paper,
) -> Result<(), String> {
    gdi::print_to(printer, pages, g, font, title, copies, duplex, None)
}
