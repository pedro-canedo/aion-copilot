//! aion-copilot: its windows, in one table. Each is a page of its own; the
//! overlays float transparent and always on top of the game, the Painel is an
//! ordinary window. `app.rs` builds, places, reopens and lists them from here,
//! so a new window is one row (and a label that `capabilities/default.json`
//! covers: the test below checks every row).

/// How a window is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An ordinary window: the system's frame and shadow, on the taskbar.
    Panel,
    /// No frame of the system's: the page draws its own header.
    Chromeless,
}

/// Where a window opens the first time, before the player moves it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    /// Centred on the screen, like any window.
    Centre,
    /// Logical px from the screen's right edge to the window's right edge,
    /// and from the top.
    FromRight(f64, f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopilotWindow {
    pub label: &'static str,
    pub page: &'static str,
    pub title: &'static str,
    /// Logical size the first time.
    pub size: (f64, f64),
    pub min_size: (f64, f64),
    /// Transparent, always on top and off the taskbar, over the game.
    pub overlay: bool,
    pub place: Place,
    /// Opened again at start when it was open at exit.
    pub reopen: bool,
    pub kind: Kind,
}

/// The bar: always opened at start, since it is how the others open, and not
/// in its own list of windows.
pub const BAR: &str = "hub";

/// Smallest size of an overlay with no size of its own.
const OVERLAY_MIN: (f64, f64) = (220.0, 140.0);

const fn overlay(label: &'static str, page: &'static str, title: &'static str, size: (f64, f64), place: (f64, f64)) -> CopilotWindow {
    CopilotWindow {
        label,
        page,
        title,
        size,
        min_size: OVERLAY_MIN,
        overlay: true,
        place: Place::FromRight(place.0, place.1),
        reopen: true,
        kind: Kind::Chromeless,
    }
}

/// Every copilot window. The bar near the top-right corner (left of the
/// minimap, as the game lays out its HUD), the overlays in columns under it.
pub const WINDOWS: &[CopilotWindow] = &[
    CopilotWindow { min_size: (200.0, 36.0), ..overlay("hub", "hub.html", "aion-copilot — Barra", (450.0, 48.0), (360.0, 12.0)) },
    overlay("routine", "routine.html", "aion-copilot — Rotina", (300.0, 430.0), (340.0, 300.0)),
    overlay("group", "group.html", "aion-copilot — Grupo", (280.0, 260.0), (24.0, 570.0)),
    overlay("bosses", "bosses.html", "aion-copilot — Chefes", (300.0, 340.0), (340.0, 740.0)),
    overlay("target", "target.html", "aion-copilot — Alvo", (300.0, 250.0), (24.0, 300.0)),
    overlay("timers", "timers.html", "aion-copilot — Timers", (340.0, 380.0), (656.0, 300.0)),
    CopilotWindow { min_size: (360.0, 320.0), ..overlay("map", "map.html", "aion-copilot — Mapa", (560.0, 600.0), (972.0, 300.0)) },
    overlay("pets", "pets.html", "aion-copilot — Mascotes", (320.0, 380.0), (656.0, 696.0)),
    CopilotWindow {
        label: "painel",
        page: "painel.html",
        title: "aion-copilot — Painel",
        size: (1100.0, 700.0),
        min_size: (760.0, 480.0),
        overlay: false,
        place: Place::Centre,
        reopen: false,
        kind: Kind::Panel,
    },
];

pub fn find(label: &str) -> Option<&'static CopilotWindow> {
    WINDOWS.iter().find(|w| w.label == label)
}

pub fn labels() -> impl Iterator<Item = &'static str> {
    WINDOWS.iter().map(|w| w.label)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A capability entry: a label, or a prefix ending in `*`.
    fn covers(entry: &str, label: &str) -> bool {
        match entry.strip_suffix('*') {
            Some(prefix) => label.starts_with(prefix),
            None => entry == label,
        }
    }

    /// A window no capability names opens blank, with no error anywhere.
    #[test]
    fn every_window_is_covered_by_the_capabilities() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
        let text = std::fs::read_to_string(&path).expect("capabilities/default.json");
        let json: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        let entries: Vec<&str> = json["windows"].as_array().expect("a windows list").iter().filter_map(|v| v.as_str()).collect();
        assert!(entries.contains(&"copilot-*"), "new copilot windows are labelled copilot-*");
        for w in WINDOWS {
            assert!(entries.iter().any(|e| covers(e, w.label)), "{} has no capability", w.label);
        }
    }

    #[test]
    fn labels_are_unique_and_the_bar_is_one_of_them() {
        let mut seen = std::collections::HashSet::new();
        assert!(WINDOWS.iter().all(|w| seen.insert(w.label)));
        assert!(find(BAR).is_some_and(|w| w.overlay && w.reopen));
        assert!(find("nope").is_none());
    }

    #[test]
    fn the_painel_is_an_ordinary_window_and_the_rest_float() {
        for w in WINDOWS {
            assert_eq!(w.overlay, w.kind == Kind::Chromeless, "{}", w.label);
            assert!(w.min_size.0 <= w.size.0 && w.min_size.1 <= w.size.1, "{}", w.label);
        }
        assert_eq!(find("painel").map(|w| (w.overlay, w.place, w.reopen)), Some((false, Place::Centre, false)));
    }
}
