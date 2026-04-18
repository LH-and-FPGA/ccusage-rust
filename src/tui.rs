//! Interactive TUI dashboard.
//!
//! Four views switchable with `1/2/3/4` (or Tab): Daily, Monthly, Session,
//! Blocks. Each view has a colored bar chart of cost-per-bucket on top, a
//! summary panel, and a detail panel for the currently selected bucket.
//!
//! Animation: when the user switches views, all bar heights ease from 0 to
//! their target height over ~500ms (cubic ease-out). The active 5-hour block
//! pulses with a sine envelope.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Bar, BarChart, BarGroup, Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};
use ratatui::{Frame, Terminal};
use std::io::{stdout, Stdout};
use std::time::{Duration, Instant};

use crate::aggregate::{Bucket, Totals};
use crate::blocks::SessionBlock;

const TICK_MS: u64 = 33; // ~30fps
const ANIM_MS: f64 = 500.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Daily,
    Monthly,
    Session,
    Blocks,
}

impl View {
    fn next(self) -> Self {
        match self {
            View::Daily => View::Monthly,
            View::Monthly => View::Session,
            View::Session => View::Blocks,
            View::Blocks => View::Daily,
        }
    }
    fn prev(self) -> Self {
        match self {
            View::Daily => View::Blocks,
            View::Monthly => View::Daily,
            View::Session => View::Monthly,
            View::Blocks => View::Session,
        }
    }
    fn idx(self) -> usize {
        match self {
            View::Daily => 0,
            View::Monthly => 1,
            View::Session => 2,
            View::Blocks => 3,
        }
    }
    fn title(self) -> &'static str {
        match self {
            View::Daily => "Daily",
            View::Monthly => "Monthly",
            View::Session => "Session",
            View::Blocks => "Blocks",
        }
    }
}

struct App {
    daily: Vec<Bucket>,
    monthly: Vec<Bucket>,
    session: Vec<Bucket>,
    blocks: Vec<SessionBlock>,
    view: View,
    view_changed_at: Instant,
    started_at: Instant,
    daily_sel: ListState,
    monthly_sel: ListState,
    session_sel: ListState,
    blocks_sel: ListState,
}

impl App {
    fn new(
        daily: Vec<Bucket>,
        monthly: Vec<Bucket>,
        session: Vec<Bucket>,
        blocks: Vec<SessionBlock>,
    ) -> Self {
        let sel_default = |len: usize| {
            let mut s = ListState::default();
            if len > 0 {
                s.select(Some(len - 1));
            }
            s
        };
        Self {
            daily_sel: sel_default(daily.len()),
            monthly_sel: sel_default(monthly.len()),
            session_sel: sel_default(session.len()),
            blocks_sel: sel_default(blocks.len()),
            daily,
            monthly,
            session,
            blocks,
            view: View::Daily,
            view_changed_at: Instant::now(),
            started_at: Instant::now(),
        }
    }

    fn switch(&mut self, v: View) {
        if self.view != v {
            self.view = v;
            self.view_changed_at = Instant::now();
        }
    }

    /// 0.0 → 1.0 over ANIM_MS with cubic ease-out.
    fn anim(&self) -> f64 {
        let elapsed = self.view_changed_at.elapsed().as_millis() as f64;
        let t = (elapsed / ANIM_MS).min(1.0);
        1.0 - (1.0 - t).powi(3)
    }

    /// Sine-pulse envelope (0.5 .. 1.0) for the active block.
    fn pulse(&self) -> f64 {
        let t = self.started_at.elapsed().as_secs_f64();
        0.75 + 0.25 * (t * 3.0).sin()
    }

    fn current_len(&self) -> usize {
        match self.view {
            View::Daily => self.daily.len(),
            View::Monthly => self.monthly.len(),
            View::Session => self.session.len(),
            View::Blocks => self.blocks.len(),
        }
    }

    fn current_state(&mut self) -> &mut ListState {
        match self.view {
            View::Daily => &mut self.daily_sel,
            View::Monthly => &mut self.monthly_sel,
            View::Session => &mut self.session_sel,
            View::Blocks => &mut self.blocks_sel,
        }
    }

    fn move_sel(&mut self, delta: isize) {
        let len = self.current_len();
        if len == 0 {
            return;
        }
        let st = self.current_state();
        let cur = st.selected().unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        st.select(Some(next));
    }
}

pub fn run(
    daily: Vec<Bucket>,
    monthly: Vec<Bucket>,
    session: Vec<Bucket>,
    blocks: Vec<SessionBlock>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut term = Terminal::new(backend)?;

    let res = run_loop(&mut term, App::new(daily, monthly, session, blocks));

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    term.show_cursor()?;
    res
}

fn run_loop(
    term: &mut Terminal<CrosstermBackend<Stdout>>,
    mut app: App,
) -> Result<()> {
    let tick = Duration::from_millis(TICK_MS);
    let mut last = Instant::now();
    loop {
        term.draw(|f| draw(f, &mut app))?;

        let timeout = tick.checked_sub(last.elapsed()).unwrap_or_default();
        if event::poll(timeout)? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('1') => app.switch(View::Daily),
                    KeyCode::Char('2') => app.switch(View::Monthly),
                    KeyCode::Char('3') => app.switch(View::Session),
                    KeyCode::Char('4') => app.switch(View::Blocks),
                    KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                        let n = app.view.next();
                        app.switch(n);
                    }
                    KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                        let n = app.view.prev();
                        app.switch(n);
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.move_sel(1),
                    KeyCode::Up | KeyCode::Char('k') => app.move_sel(-1),
                    KeyCode::Home | KeyCode::Char('g') => {
                        if app.current_len() > 0 {
                            app.current_state().select(Some(0));
                        }
                    }
                    KeyCode::End | KeyCode::Char('G') => {
                        let last = app.current_len().saturating_sub(1);
                        if app.current_len() > 0 {
                            app.current_state().select(Some(last));
                        }
                    }
                    _ => {}
                }
            }
        }
        if last.elapsed() >= tick {
            last = Instant::now();
        }
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Min(0),    // body
            Constraint::Length(1), // hint bar
        ])
        .split(f.area());

    draw_tabs(f, chunks[0], app);
    match app.view {
        View::Daily => draw_buckets_view(f, chunks[1], app, ViewKind::Daily),
        View::Monthly => draw_buckets_view(f, chunks[1], app, ViewKind::Monthly),
        View::Session => draw_buckets_view(f, chunks[1], app, ViewKind::Session),
        View::Blocks => draw_blocks_view(f, chunks[1], app),
    }
    draw_hint(f, chunks[2]);
}

#[derive(Clone, Copy)]
enum ViewKind {
    Daily,
    Monthly,
    Session,
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = [View::Daily, View::Monthly, View::Session, View::Blocks]
        .iter()
        .map(|v| Line::from(v.title()))
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" rcusage "))
        .select(app.view.idx())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, area);
}

fn draw_hint(f: &mut Frame, area: Rect) {
    let hint = Line::from(vec![
        Span::styled(" 1-4 ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::raw(" view  "),
        Span::styled(" Tab ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::raw(" next  "),
        Span::styled(" ↑↓/jk ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::raw(" select  "),
        Span::styled(" g/G ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::raw(" first/last  "),
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::raw(" quit"),
    ]);
    f.render_widget(Paragraph::new(hint), area);
}

/// Disjoint split of `App` into the slice + state for the chosen view. This
/// lets one closure borrow the data immutably while another borrows the
/// selection mutably without tripping the borrow checker.
fn split_buckets<'a>(app: &'a mut App, kind: ViewKind) -> (&'a [Bucket], &'a mut ListState) {
    match kind {
        ViewKind::Daily => (&app.daily, &mut app.daily_sel),
        ViewKind::Monthly => (&app.monthly, &mut app.monthly_sel),
        ViewKind::Session => (&app.session, &mut app.session_sel),
    }
}

fn draw_buckets_view(f: &mut Frame, area: Rect, app: &mut App, kind: ViewKind) {
    let anim = app.anim();
    let pulse = app.pulse();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let (buckets, state) = split_buckets(app, kind);
    let selected = state.selected().unwrap_or(0);
    draw_bar_panel(f, chunks[0], buckets, selected, anim, pulse, kind);
    draw_list_panel(f, bottom[0], buckets, state);
    draw_detail_panel(f, bottom[1], buckets, selected);
}

fn make_bar<'a>(label: String, value: u64, color: Color, selected: bool) -> Bar<'a> {
    let style = if selected {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color)
    };
    Bar::default()
        .label(Line::from(label))
        .value(value)
        .style(style)
        .value_style(
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        )
}

fn draw_bar_panel(
    f: &mut Frame,
    area: Rect,
    buckets: &[Bucket],
    selected: usize,
    anim: f64,
    pulse: f64,
    kind: ViewKind,
) {
    let title = format!(" {} cost (USD) ", match kind {
        ViewKind::Daily => "Daily",
        ViewKind::Monthly => "Monthly",
        ViewKind::Session => "Session",
    });
    if buckets.is_empty() {
        let block = Block::default().borders(Borders::ALL).title(title);
        f.render_widget(
            Paragraph::new("No data").block(block).alignment(ratatui::layout::Alignment::Center),
            area,
        );
        return;
    }

    // Limit visible bars to fit the inner width. BarChart needs ~5 cells per bar.
    let inner_w = area.width.saturating_sub(2) as usize;
    let bar_w: u16 = 6;
    let gap: u16 = 1;
    let max_bars = (inner_w / (bar_w as usize + gap as usize)).max(1);

    // Window around the selected bucket (right-anchored when possible).
    let total = buckets.len();
    let end = (selected + 1).max(max_bars).min(total);
    let start = end.saturating_sub(max_bars);
    let window = &buckets[start..end];

    // Animate cost values: scale by `anim` and apply pulse to the selected bar.
    let bars: Vec<Bar> = window
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let abs_idx = start + i;
            let is_sel = abs_idx == selected;
            let mut scaled = (b.totals.total_cost_usd * 100.0 * anim) as u64;
            if is_sel {
                scaled = ((scaled as f64) * pulse) as u64;
            }
            let label = short_label(&b.key, kind);
            let color = if is_sel { Color::Yellow } else { color_for_cost(b.totals.total_cost_usd) };
            make_bar(label, scaled, color, is_sel)
        })
        .collect();

    let chart = BarChart::default()
        .block(Block::default().borders(Borders::ALL).title(title))
        .data(BarGroup::default().bars(&bars))
        .bar_width(bar_w)
        .bar_gap(gap)
        .label_style(Style::default().fg(Color::Gray));
    f.render_widget(chart, area);
}

fn draw_list_panel(f: &mut Frame, area: Rect, buckets: &[Bucket], state: &mut ListState) {
    let items: Vec<ListItem> = buckets
        .iter()
        .map(|b| {
            let cost = format!("${:.2}", b.totals.total_cost_usd);
            let line = Line::from(vec![
                Span::styled(
                    format!("{:<22}", b.key),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:>9}", cost),
                    Style::default()
                        .fg(color_for_cost(b.totals.total_cost_usd))
                        .add_modifier(Modifier::BOLD),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Buckets "))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, state);
}

fn draw_detail_panel(f: &mut Frame, area: Rect, buckets: &[Bucket], sel: usize) {
    let block = Block::default().borders(Borders::ALL).title(" Detail ");
    if buckets.is_empty() {
        f.render_widget(Paragraph::new("No data").block(block), area);
        return;
    }
    let sel = sel.min(buckets.len() - 1);
    let b = &buckets[sel];
    let lines = totals_lines(&b.key, &b.totals, &b.models);
    f.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: false }),
        area,
    );
}

fn totals_lines<'a>(key: &str, t: &Totals, models: &'a [String]) -> Vec<Line<'a>> {
    let total = t.input_tokens + t.output_tokens + t.cache_creation_tokens + t.cache_read_tokens;
    vec![
        Line::from(vec![Span::styled(key.to_string(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        kv("Input tokens", fmt_int(t.input_tokens), Color::Yellow),
        kv("Output tokens", fmt_int(t.output_tokens), Color::Yellow),
        kv("Cache creation", fmt_int(t.cache_creation_tokens), Color::Yellow),
        kv("Cache read", fmt_int(t.cache_read_tokens), Color::Yellow),
        kv("Total tokens", fmt_int(total), Color::White),
        Line::from(""),
        kv_styled(
            "Cost",
            format!("${:.2}", t.total_cost_usd),
            Style::default().fg(color_for_cost(t.total_cost_usd)).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from(Span::styled("Models:", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(if models.is_empty() {
            Span::raw("(none)")
        } else {
            Span::styled(models.join(", "), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
        }),
    ]
}

fn draw_blocks_view(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // Top: bar chart of cost per block (gaps as zero-height bars).
    if app.blocks.is_empty() {
        let block = Block::default().borders(Borders::ALL).title(" 5-hour blocks ");
        f.render_widget(
            Paragraph::new("No data").block(block).alignment(ratatui::layout::Alignment::Center),
            chunks[0],
        );
    } else {
        let anim = app.anim();
        let pulse = app.pulse();
        let selected = app.blocks_sel.selected().unwrap_or(0);

        let inner_w = chunks[0].width.saturating_sub(2) as usize;
        let bar_w: u16 = 6;
        let gap: u16 = 1;
        let max_bars = (inner_w / (bar_w as usize + gap as usize)).max(1);
        let total = app.blocks.len();
        let end = (selected + 1).max(max_bars).min(total);
        let start = end.saturating_sub(max_bars);
        let window = &app.blocks[start..end];

        let bars: Vec<Bar> = window
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let abs_idx = start + i;
                let is_sel = abs_idx == selected;
                let mut v = (b.totals.total_cost_usd * 100.0 * anim) as u64;
                if b.is_active {
                    v = ((v as f64) * pulse) as u64;
                }
                let label = b.start.to_string()[5..16].to_string(); // "MM-DDTHH:MM"
                let color = if b.is_gap {
                    Color::DarkGray
                } else if b.is_active {
                    Color::Green
                } else if is_sel {
                    Color::Yellow
                } else {
                    color_for_cost(b.totals.total_cost_usd)
                };
                make_bar(label, v, color, is_sel)
            })
            .collect();

        let chart = BarChart::default()
            .block(Block::default().borders(Borders::ALL).title(" 5-hour blocks (cost USD × 100) "))
            .data(BarGroup::default().bars(&bars))
            .bar_width(bar_w)
            .bar_gap(gap)
            .label_style(Style::default().fg(Color::Gray));
        f.render_widget(chart, chunks[0]);
    }

    // Bottom: list + detail.
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let items: Vec<ListItem> = app
        .blocks
        .iter()
        .map(|b| {
            let status = if b.is_gap {
                Span::styled(" gap   ", Style::default().fg(Color::DarkGray))
            } else if b.is_active {
                Span::styled(" active", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" closed", Style::default().fg(Color::Gray))
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", &b.start.to_string()[..16]),
                    Style::default().fg(Color::Cyan),
                ),
                status,
                Span::raw(" "),
                Span::styled(
                    format!("${:.2}", b.totals.total_cost_usd),
                    Style::default()
                        .fg(color_for_cost(b.totals.total_cost_usd))
                        .add_modifier(Modifier::BOLD),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Blocks "))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, bottom[0], &mut app.blocks_sel);

    // Detail
    let detail_block = Block::default().borders(Borders::ALL).title(" Detail ");
    if app.blocks.is_empty() {
        f.render_widget(Paragraph::new("No data").block(detail_block), bottom[1]);
        return;
    }
    let sel = app.blocks_sel.selected().unwrap_or(0).min(app.blocks.len() - 1);
    let b = &app.blocks[sel];
    let status_line = if b.is_gap {
        Line::from(Span::styled("GAP", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)))
    } else if b.is_active {
        Line::from(Span::styled("ACTIVE", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)))
    } else {
        Line::from(Span::styled("CLOSED", Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)))
    };
    let mut lines = vec![
        status_line,
        Line::from(""),
        kv("Start", b.start.to_string(), Color::Cyan),
        kv("End", b.end.to_string(), Color::Cyan),
    ];
    if let Some(ae) = b.actual_end {
        lines.push(kv("Last activity", ae.to_string(), Color::Cyan));
    }
    lines.push(Line::from(""));
    let total = b.totals.input_tokens + b.totals.output_tokens + b.totals.cache_creation_tokens + b.totals.cache_read_tokens;
    lines.push(kv("Input", fmt_int(b.totals.input_tokens), Color::Yellow));
    lines.push(kv("Output", fmt_int(b.totals.output_tokens), Color::Yellow));
    lines.push(kv("Cache create", fmt_int(b.totals.cache_creation_tokens), Color::Yellow));
    lines.push(kv("Cache read", fmt_int(b.totals.cache_read_tokens), Color::Yellow));
    lines.push(kv("Total tokens", fmt_int(total), Color::White));
    lines.push(Line::from(""));
    lines.push(kv_styled(
        "Cost",
        format!("${:.2}", b.totals.total_cost_usd),
        Style::default().fg(color_for_cost(b.totals.total_cost_usd)).add_modifier(Modifier::BOLD),
    ));
    if !b.models.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Models:", Style::default().add_modifier(Modifier::BOLD))));
        lines.push(Line::from(Span::styled(
            b.models.join(", "),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )));
    }
    f.render_widget(Paragraph::new(lines).block(detail_block).wrap(Wrap { trim: false }), bottom[1]);
}

fn kv(k: &str, v: String, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<16}", k), Style::default().fg(Color::Gray)),
        Span::styled(v, Style::default().fg(color).add_modifier(Modifier::BOLD)),
    ])
}

fn kv_styled(k: &str, v: String, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<16}", k), Style::default().fg(Color::Gray)),
        Span::styled(v, style),
    ])
}

fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        let from_end = len - i;
        if i != 0 && from_end % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn color_for_cost(v: f64) -> Color {
    if v < 1.0 {
        Color::DarkGray
    } else if v < 10.0 {
        Color::Green
    } else if v < 100.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn short_label(key: &str, kind: ViewKind) -> String {
    match kind {
        ViewKind::Daily => key.get(5..).unwrap_or(key).to_string(), // MM-DD
        ViewKind::Monthly => key.to_string(),
        ViewKind::Session => {
            // project might be very long; take last segment after `/`
            let last = key.rsplit('/').next().unwrap_or(key);
            last.chars().take(6).collect()
        }
    }
}
