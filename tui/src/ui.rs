//! Drawing: the full-screen System Settings layout, or the compact card on small terminals.
//! Layout, colours and icons follow docs/dev/tui-design.md.

use crate::app::{App, Focus, Heart, Info, Kind, Row, NOISE, SECTIONS};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

const WINDOW: Color = Color::Rgb(0x1C, 0x1C, 0x1E);
const PANE: Color = Color::Rgb(0x28, 0x28, 0x2A);
const GROUP: Color = Color::Rgb(0x2C, 0x2C, 0x2E);
const BORDER: Color = Color::Rgb(0x48, 0x48, 0x4A);
const SELECTED: Color = Color::Rgb(0x3A, 0x3A, 0x3C);
const LABEL: Color = Color::Rgb(0xEB, 0xEB, 0xF5);
const SECONDARY: Color = Color::Rgb(0x98, 0x98, 0x9D);
const TERTIARY: Color = Color::Rgb(0x7C, 0x7C, 0x80);
const BLUE: Color = Color::Rgb(0x0A, 0x84, 0xFF);
const CURSOR: Color = Color::Rgb(0x0A, 0x6C, 0xDB);
const GREEN: Color = Color::Rgb(0x30, 0xD1, 0x58);
const RED: Color = Color::Rgb(0xFF, 0x45, 0x3A);
const WHITE: Color = Color::Rgb(0xFF, 0xFF, 0xFF);

const HEADPHONES: char = '\u{F02CB}';
const HEADPHONES_OFF: char = '\u{F07CE}';
const BUD: char = '\u{F184F}';
const CASE: char = '\u{F02CC}';
const DOT: char = '\u{F09DE}';

/// Terminals smaller than this get the compact card.
pub fn is_compact(width: u16, height: u16) -> bool {
    width < 100 || height < 30
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(WINDOW)), area);
    if app.compact {
        compact(f, app, area);
    } else {
        full(f, app, area);
    }
}

fn fg(c: Color) -> Style {
    Style::new().fg(c)
}

fn bold(c: Color) -> Style {
    fg(c).add_modifier(Modifier::BOLD)
}

/// A rounded pane; returns its inside.
fn pane(f: &mut Frame, area: Rect, bg: Color) -> Rect {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(fg(BORDER))
        .style(Style::new().bg(bg));
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

fn level_color(level: u8, charging: bool) -> Color {
    if charging {
        GREEN
    } else if level <= 20 {
        RED
    } else {
        LABEL
    }
}

/// Battery glyph and percentage, e.g. "󰂂 93%".
fn battery_text(level: Option<u8>, charging: bool) -> (String, Color) {
    const PLAIN: [u32; 11] = [
        0xF008E, 0xF007A, 0xF007B, 0xF007C, 0xF007D, 0xF007E, 0xF007F, 0xF0080, 0xF0081, 0xF0082,
        0xF0079,
    ];
    const CHARGING: [u32; 11] = [
        0xF089C, 0xF089C, 0xF0086, 0xF0087, 0xF0088, 0xF089D, 0xF0089, 0xF089E, 0xF008A, 0xF008B,
        0xF0085,
    ];
    let Some(l) = level else {
        return ("\u{F0091} --".into(), TERTIARY);
    };
    let table = if charging { CHARGING } else { PLAIN };
    let glyph = char::from_u32(table[(l as usize + 5) / 10 % 11]).unwrap_or(' ');
    (format!("{glyph} {l}%"), level_color(l, charging))
}

/// (name, icon, level, charging, state) for left, right and case.
type Part = (&'static str, char, Option<u8>, bool, Option<&'static str>);

fn parts(app: &App) -> [Part; 3] {
    let d = &app.device;
    let b = if d.connected() {
        d.battery.unwrap_or_default()
    } else {
        Default::default()
    };
    let ear = if d.connected() {
        d.bud_states()
    } else {
        [None, None]
    };
    [
        ("Left", BUD, b.left, b.left_charging, ear[0]),
        ("Right", BUD, b.right, b.right_charging, ear[1]),
        (
            "Case",
            CASE,
            b.case,
            b.case_charging,
            b.case_charging.then_some("charging"),
        ),
    ]
}

fn info(app: &App, which: Info) -> String {
    let d = &app.device;
    let value = match which {
        Info::Name => d.name.clone(),
        Info::Model => d.model.clone(),
        Info::Firmware => d.firmware.clone(),
        Info::Address => d.connected().then(|| d.addr.clone()),
        Info::HeartRate => d.connected().then(|| match &app.heart {
            Heart::Off => "⏎ to measure".into(),
            Heart::Starting => "measuring…".into(),
            Heart::Live(h) => format!("{}  {} bpm", sparkline(h), h.last().unwrap_or(&0)),
            Heart::Failed => "no reading, ⏎ to retry".into(),
        }),
    };
    value.unwrap_or_else(|| "--".into())
}

/// Readings as block characters scaled between their own minimum and maximum.
fn sparkline(readings: &[u16]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let lo = readings.iter().copied().min().unwrap_or(0);
    let hi = readings.iter().copied().max().unwrap_or(0);
    readings
        .iter()
        .map(|&r| match hi - lo {
            0 => BARS[3],
            span => BARS[((r - lo) as usize * 7 + span as usize / 2) / span as usize],
        })
        .collect()
}

/// One setting row, padded to `width`: mark, icon and label on the left, control on the right.
fn row_line(app: &App, row: &Row, width: usize, cursor: bool, bg: Color) -> Line<'static> {
    let d = &app.device;
    let on = d.connected();
    let text = if cursor {
        WHITE
    } else if on {
        LABEL
    } else {
        TERTIARY
    };
    let accent = |c: Color| if cursor { WHITE } else { c };
    let mark = |glyphs: [char; 2], set: bool| {
        let color = accent(if set { BLUE } else { TERTIARY });
        Span::styled(format!("{}  ", glyphs[set as usize]), fg(color))
    };
    let mut left = Vec::new();
    match row.kind {
        Kind::Radio(v) => left.push(mark(
            ['\u{F043D}', '\u{F043E}'],
            on && d.control(NOISE) == Some(v),
        )),
        Kind::Check(bit) => left.push(mark(
            ['\u{F0131}', '\u{F0132}'],
            on && d.control(0x1A).is_some_and(|m| m & bit != 0),
        )),
        _ => {}
    }
    left.push(Span::styled(
        format!("{}  {}", row.icon, row.label),
        fg(text),
    ));

    let unknown = || vec![Span::styled("--", fg(accent(TERTIARY)))];
    let right = match row.kind {
        Kind::Switch(id) => match d.control(id).filter(|_| on) {
            Some(1) => vec![Span::styled("\u{F0521}", fg(accent(GREEN)))],
            Some(_) => vec![Span::styled("\u{F0A19}", fg(accent(TERTIARY)))],
            None => unknown(),
        },
        Kind::Choice(id, opts) => opts
            .iter()
            .map(|(label, v)| {
                let style = match (on && d.control(id) == Some(*v), cursor) {
                    (true, true) => Style::new().fg(CURSOR).bg(WHITE),
                    (true, false) => Style::new().fg(WHITE).bg(BORDER),
                    (false, _) => fg(accent(SECONDARY)),
                };
                Span::styled(format!(" {label} "), style)
            })
            .collect(),
        Kind::Slider(id) => match d.control(id).filter(|_| on) {
            Some(v) => {
                let n = (v.min(100) as usize * 20 + 50) / 100;
                vec![
                    Span::styled("━".repeat(n), fg(accent(BLUE))),
                    Span::styled("●", fg(text)),
                    Span::styled("─".repeat(20 - n), fg(accent(BORDER))),
                    Span::styled(format!(" {v:>3}"), fg(text)),
                ]
            }
            None => unknown(),
        },
        Kind::Info(which) => {
            let live = matches!((which, &app.heart), (Info::HeartRate, Heart::Live(_)));
            let color = accent(if live && on { RED } else { SECONDARY });
            vec![Span::styled(info(app, which), fg(color))]
        }
        Kind::Radio(_) | Kind::Check(_) => Vec::new(),
    };

    let used: usize = left.iter().chain(&right).map(Span::width).sum();
    let gap = width.saturating_sub(used + 2);
    let mut spans = vec![Span::raw(" ")];
    spans.extend(left);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    spans.push(Span::raw(" "));
    Line::from(spans).style(Style::new().bg(if cursor { CURSOR } else { bg }))
}

fn full(f: &mut Frame, app: &App, area: Rect) {
    let [top, middle, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let [side, _, main] = Layout::horizontal([
        Constraint::Length(26),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(middle);

    let d = &app.device;
    let title = Line::styled(format!(" {HEADPHONES}  podatui"), bold(LABEL));
    f.render_widget(Paragraph::new(title), top);
    let status = match &d.name {
        Some(name) => Line::from(vec![
            Span::styled(DOT.to_string(), fg(GREEN)),
            Span::styled(format!(" {name}  connected "), fg(SECONDARY)),
        ]),
        None => Line::from(vec![
            Span::styled(DOT.to_string(), fg(TERTIARY)),
            Span::styled(format!(" {} ", app.status), fg(SECONDARY)),
        ]),
    };
    f.render_widget(Paragraph::new(status).alignment(Alignment::Right), top);
    let hints = " tab sidebar/settings   ↑↓ move   ←→ change   ⏎ toggle   1-4 noise mode   q quit";
    f.render_widget(Paragraph::new(Line::styled(hints, fg(SECONDARY))), bottom);

    sidebar(f, app, side);
    content(f, app, main);
}

fn sidebar(f: &mut Frame, app: &App, area: Rect) {
    let inner = pane(f, area, PANE);
    let w = inner.width as usize;
    let d = &app.device;
    let pct = |l: Option<u8>| l.map_or("--".into(), |l| format!("{l}%"));
    let (head, sub) = match &d.name {
        Some(name) => {
            let b = d.battery.unwrap_or_default();
            let sub = format!(" L {}  R {}  C {}", pct(b.left), pct(b.right), pct(b.case));
            (
                Line::styled(format!(" {HEADPHONES}  {name}"), bold(LABEL)),
                sub,
            )
        }
        None => (
            Line::styled(format!(" {HEADPHONES_OFF}  Not connected"), bold(TERTIARY)),
            " connect in Bluetooth".into(),
        ),
    };
    let mut lines = vec![
        head,
        Line::styled(sub, fg(SECONDARY)),
        Line::styled("─".repeat(w), fg(BORDER)),
    ];
    for (i, s) in SECTIONS.iter().enumerate() {
        let style = match (i == app.section, app.focus) {
            (true, Focus::Sidebar) => Style::new().fg(WHITE).bg(CURSOR),
            (true, Focus::Content) => Style::new().fg(LABEL).bg(SELECTED),
            (false, _) => fg(LABEL),
        };
        let text = format!(" {}  {}", s.icon, s.name);
        lines.push(Line::styled(format!("{text:<w$}"), style));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn content(f: &mut Frame, app: &App, area: Rect) {
    let inner = pane(f, area, PANE);
    let x = inner.x + 2;
    let width = inner.width.saturating_sub(4).min(76);
    let at = |y: u16, h: u16| Rect::new(x, y, width, h).intersection(inner);
    let mut y = inner.y;
    let title = Line::styled(SECTIONS[app.section].name, bold(LABEL));
    f.render_widget(Paragraph::new(title), at(y, 1));
    y += 2;
    if app.section == 0 {
        hero(f, app, at(y, 4));
        y += 5;
    }
    let mut index = 0;
    for g in app.groups() {
        if !g.title.is_empty() {
            f.render_widget(
                Paragraph::new(Line::styled(format!(" {}", g.title), fg(SECONDARY))),
                at(y, 1),
            );
            y += 1;
        }
        let h = g.rows.len() as u16 + 2;
        let rows = pane(f, at(y, h), GROUP);
        let lines: Vec<Line> = g
            .rows
            .iter()
            .map(|r| {
                let cursor = app.focus == Focus::Content && app.cursor == index;
                index += 1;
                row_line(app, r, rows.width as usize, cursor, GROUP)
            })
            .collect();
        f.render_widget(Paragraph::new(lines), rows);
        y += h + 1;
    }
}

/// Large battery bars for left, right and case.
fn hero(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::horizontal([Constraint::Length(26); 3]).split(area);
    for ((name, icon, level, charging, state), col) in parts(app).into_iter().zip(cols.iter()) {
        let width = 22;
        let n = level.map_or(0, |l| (l.min(100) as usize * width + 50) / 100);
        let color = level.map_or(TERTIARY, |l| level_color(l, charging));
        let bar = Line::from(vec![
            Span::styled("█".repeat(n), fg(color)),
            Span::styled("█".repeat(width - n), fg(SELECTED)),
        ]);
        let lines = vec![
            Line::styled(format!("{icon}  {name}"), fg(SECONDARY)),
            bar.clone(),
            bar,
            Line::from(vec![
                Span::styled(level.map_or("--".into(), |l| format!("{l}%")), bold(color)),
                Span::styled(
                    state.map(|s| format!("  {s}")).unwrap_or_default(),
                    fg(SECONDARY),
                ),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), *col);
    }
}

fn compact(f: &mut Frame, app: &App, area: Rect) {
    let (w, h) = (40.min(area.width), 16.min(area.height));
    let card = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + area.height.saturating_sub(h + 1) / 2,
        w,
        h,
    );
    let inner = pane(f, card, PANE);
    let width = inner.width as usize;
    let d = &app.device;
    let head = match &d.name {
        Some(name) => Line::styled(format!(" {HEADPHONES}  {name}"), bold(LABEL)),
        None => Line::styled(format!(" {HEADPHONES_OFF}  Not connected"), bold(TERTIARY)),
    };
    let parts = parts(app);
    let column = |cells: Vec<Span<'static>>| {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(cells);
        Line::from(spans)
    };
    let pad = |s: String| format!("{s:<11}");
    let separator = || Line::styled("─".repeat(width), fg(BORDER));
    let mut lines = vec![
        head,
        Line::raw(""),
        column(
            parts
                .iter()
                .map(|p| Span::styled(pad(format!("{} {}", p.1, p.0)), fg(SECONDARY)))
                .collect(),
        ),
        column(
            parts
                .iter()
                .map(|p| {
                    let (text, color) = battery_text(p.2, p.3);
                    Span::styled(pad(text), fg(color))
                })
                .collect(),
        ),
        column(
            parts
                .iter()
                .map(|p| Span::styled(pad(p.4.unwrap_or("").into()), fg(SECONDARY)))
                .collect(),
        ),
    ];
    let mut index = 0;
    for g in app.groups() {
        lines.push(separator());
        if !g.title.is_empty() {
            lines.push(Line::styled(format!(" {}", g.title), bold(SECONDARY)));
        }
        for r in g.rows {
            lines.push(row_line(app, r, width, app.cursor == index, PANE));
            index += 1;
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
    let hints = Rect::new(card.x, card.y + card.height, card.width, 1).intersection(area);
    f.render_widget(
        Paragraph::new(Line::styled(
            "  ↑↓ select   ⏎ apply   q quit",
            fg(SECONDARY),
        )),
        hints,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::LinkEvent;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn screen(app: &App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        let buf = t.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n")
            .collect()
    }

    fn connected() -> App {
        let mut app = App::default();
        app.on_link(LinkEvent::Connected {
            name: "AirPods Pro van Jay".into(),
            addr: "EC:E9".into(),
        });
        app.on_link(LinkEvent::Packet(vec![
            0x04, 0x00, 0x04, 0x00, 0x04, 0x00, 0x03, 0x02, 0x01, 0x55, 0x02, 0x01, 0x04, 0x01,
            0x50, 0x01, 0x01, 0x08, 0x01, 0x20, 0x02, 0x01,
        ]));
        app.on_link(LinkEvent::Packet(vec![
            0x04, 0x00, 0x04, 0x00, 0x09, 0x00, 0x0D, 0x04, 0, 0, 0,
        ]));
        app
    }

    #[test]
    fn full_screen_shows_sidebar_batteries_and_settings() {
        let s = screen(&connected(), 120, 34);
        for want in [
            "podatui",
            "AirPods Pro van Jay  connected",
            "L 80%  R 85%  C 32%",
            "About",
            "Conversation Awareness",
            " Adaptive ",
        ] {
            assert!(s.contains(want), "{want:?} missing from:\n{s}");
        }
    }

    #[test]
    fn small_terminal_gets_the_compact_card() {
        let mut app = connected();
        app.compact = true;
        let s = screen(&app, 60, 24);
        assert!(s.contains("Noise Cancellation"), "{s}");
        assert!(s.contains("85%"), "{s}");
        assert!(!s.contains("About"), "{s}");
    }

    #[test]
    fn disconnected_says_why() {
        let mut app = App::default();
        app.on_link(LinkEvent::Down("no connected AirPods found".into()));
        let s = screen(&app, 120, 34);
        assert!(s.contains("Not connected"), "{s}");
        assert!(s.contains("no connected AirPods found"), "{s}");
    }

    #[test]
    fn health_shows_live_heart_rate_with_a_sparkline() {
        let mut app = connected();
        app.section = 3;
        for bpm in [64, 70, 72] {
            app.on_link(LinkEvent::Bpm(bpm));
        }
        let s = screen(&app, 120, 34);
        assert!(s.contains("▁▆█  72 bpm"), "{s}");
    }
}
