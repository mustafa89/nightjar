//! 16-point transport and playlist icons, drawn in the text colour (`currentColor`).

use rune::Icon;

macro_rules! icon {
    ($name:ident, $id:literal, $body:literal) => {
        pub static $name: Icon = Icon {
            name: $id,
            view: 16.0,
            body: $body,
        };
    };
}

icon!(
    PLAY,
    "nj-play",
    r#"<path d="M5 3.2L13 8L5 12.8Z" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linejoin="round"/>"#
);
icon!(
    PAUSE,
    "nj-pause",
    r#"<rect x="4" y="3.25" width="2.75" height="9.5" rx="0.8" fill="currentColor"/><rect x="9.25" y="3.25" width="2.75" height="9.5" rx="0.8" fill="currentColor"/>"#
);
icon!(
    STOP,
    "nj-stop",
    r#"<rect x="4" y="4" width="8" height="8" rx="1.2" fill="currentColor"/>"#
);
icon!(
    PREV,
    "nj-prev",
    r#"<path d="M12.5 3.5L6 8L12.5 12.5Z" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linejoin="round"/><rect x="3.25" y="3.5" width="1.75" height="9" rx="0.6" fill="currentColor"/>"#
);
icon!(
    NEXT,
    "nj-next",
    r#"<path d="M3.5 3.5L10 8L3.5 12.5Z" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linejoin="round"/><rect x="11" y="3.5" width="1.75" height="9" rx="0.6" fill="currentColor"/>"#
);
icon!(
    SHUFFLE,
    "nj-shuffle",
    r#"<path d="M2.5 4.5H5C8 4.5 8 11.5 11 11.5H13.5M2.5 11.5H5C6.2 11.5 6.9 10.4 7.5 9.2M8.5 6.8C9.1 5.6 9.8 4.5 11 4.5H13.5M11.75 2.75L13.5 4.5L11.75 6.25M11.75 9.75L13.5 11.5L11.75 13.25" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    REPEAT,
    "nj-repeat",
    r#"<path d="M3 7.5V6.5A2 2 0 0 1 5 4.5H12.5M10.75 2.75L12.5 4.5L10.75 6.25M13 8.5V9.5A2 2 0 0 1 11 11.5H3.5M5.25 13.25L3.5 11.5L5.25 9.75" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    REPEAT_ONE,
    "nj-repeat-one",
    r#"<path d="M3 7.5V6.5A2 2 0 0 1 5 4.5H12.5M10.75 2.75L12.5 4.5L10.75 6.25M13 8.5V9.5A2 2 0 0 1 11 11.5H3.5M5.25 13.25L3.5 11.5L5.25 9.75" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/><path d="M7.4 7.2L8.4 6.6V9.6" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    VOLUME,
    "nj-volume",
    r#"<path d="M2.75 6.25H5L8.25 3.5V12.5L5 9.75H2.75Z" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linejoin="round"/><path d="M10.5 5.75A3 3 0 0 1 10.5 10.25M12.25 4A5.5 5.5 0 0 1 12.25 12" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" fill="none"/>"#
);
icon!(
    MUTED,
    "nj-muted",
    r#"<path d="M2.75 6.25H5L8.25 3.5V12.5L5 9.75H2.75Z" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linejoin="round"/><path d="M10.5 6.25L13.5 9.25M13.5 6.25L10.5 9.25" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#
);
icon!(
    PLUS,
    "nj-plus",
    r#"<path d="M8 3.5V12.5M3.5 8H12.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#
);
icon!(
    FOLDER,
    "nj-folder",
    r#"<path d="M2.5 4.5A1 1 0 0 1 3.5 3.5H6.25L7.75 5H12.5A1 1 0 0 1 13.5 6V11.5A1 1 0 0 1 12.5 12.5H3.5A1 1 0 0 1 2.5 11.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    TRASH,
    "nj-trash",
    r#"<path d="M3 4.5H13M6.5 4.5V3H9.5V4.5M4.5 4.5L5.25 13H10.75L11.5 4.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    LEFT,
    "nj-left",
    r#"<path d="M10 3.5L5.5 8L10 12.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    RIGHT,
    "nj-right",
    r#"<path d="M6 3.5L10.5 8L6 12.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
icon!(
    SKIN,
    "nj-skin",
    r#"<path d="M8 2.5A5.5 5.5 0 1 0 8 13.5C9 13.5 9.25 12.75 8.75 12.1C8.25 11.45 8.6 10.5 9.5 10.5H11A2.5 2.5 0 0 0 13.5 8A5.5 5.5 0 0 0 8 2.5Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/><circle cx="5.25" cy="7.5" r="0.9" fill="currentColor"/><circle cx="7.5" cy="5" r="0.9" fill="currentColor"/><circle cx="10.5" cy="5.75" r="0.9" fill="currentColor"/>"#
);
icon!(
    LAYOUT,
    "nj-layout",
    r#"<rect x="2.75" y="3.25" width="10.5" height="9.5" rx="1.5" stroke="currentColor" stroke-width="1.2" fill="none"/><path d="M7.25 3.25V12.75M7.25 7.5H13.25" stroke="currentColor" stroke-width="1.2"/>"#
);
icon!(
    NOTE,
    "nj-note",
    r#"<path d="M6.25 11.5V3.75L12.5 2.5V10.25" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/><circle cx="4.75" cy="11.5" r="1.75" fill="currentColor"/><circle cx="11" cy="10.25" r="1.75" fill="currentColor"/>"#
);
icon!(
    CYCLE,
    "nj-cycle",
    r#"<path d="M12.75 6.5A5 5 0 0 0 3.6 5.25M3.25 9.5A5 5 0 0 0 12.4 10.75" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/><path d="M3.25 2.75V5.5H6M12.75 13.25V10.5H10" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
);
