use std::io::{self, Stdout};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::skill_installation::SkillDestinationStatus;
use crate::tui_copy::tui_copy;

const APP_NAME: &str = "worklogger";
const APP_CONTEXT: &str = "MCP";
const MAX_PANEL_WIDTH: u16 = 120;
const PANEL_MARGIN: u16 = 2;
const WIDE_LAYOUT_WIDTH: u16 = 70;
const SELECTED_MARKER: &str = " ▸ ";
const BACKGROUND: Color = Color::Rgb(17, 20, 28);
const SURFACE: Color = Color::Rgb(25, 30, 41);
const FOREGROUND: Color = Color::Rgb(224, 230, 240);
const MUTED: Color = Color::Rgb(148, 160, 181);
const ACCENT: Color = Color::Rgb(139, 180, 255);
const SELECTION_BACKGROUND: Color = Color::Rgb(48, 68, 101);
const CHECKED_MARKER: &str = "[x] ";
const UNCHECKED_MARKER: &str = "[ ] ";
const SINGLE_SELECT_HELP: &str = "↑↓ navigate · Enter select · Esc cancel · Click select";
const MULTI_SELECT_HELP: &str = "↑↓ navigate · Space toggle · Enter continue · Esc cancel";
const INPUT_HELP: &str = "Enter continue · Esc cancel";

pub(crate) struct Dashboard {
    title: String,
    overview: Vec<String>,
    clients: Vec<String>,
    actions: Vec<String>,
    navigation_hint: String,
    configured: bool,
    server_installed: bool,
}

pub(crate) struct DetailedChoice {
    label: String,
    description: String,
}

impl DetailedChoice {
    pub(crate) fn new(label: String, description: String) -> Self {
        Self { label, description }
    }
}

impl Dashboard {
    pub(crate) fn new(
        title: String,
        overview: Vec<String>,
        clients: Vec<String>,
        actions: Vec<String>,
        navigation_hint: String,
        configured: bool,
        server_installed: bool,
    ) -> Self {
        Self {
            title,
            overview,
            clients,
            actions,
            navigation_hint,
            configured,
            server_installed,
        }
    }
}

type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

struct ActiveTerminal {
    terminal: AppTerminal,
    notices: Vec<String>,
}

pub(crate) struct TerminalUiSession {
    finished: bool,
}

impl TerminalUiSession {
    pub(crate) fn start() -> io::Result<Self> {
        let mut active = active_terminal()?;
        if active.is_some() {
            return Err(io::Error::other("the TUI session is already active"));
        }
        *active = Some(ActiveTerminal {
            terminal: enter_terminal()?,
            notices: Vec::new(),
        });
        Ok(Self { finished: false })
    }

    pub(crate) fn finish(mut self) -> io::Result<Vec<String>> {
        let mut active_session = active_terminal()?;
        let Some(mut terminal) = active_session.take() else {
            self.finished = true;
            return Ok(Vec::new());
        };
        if let Err(error) = leave_terminal(&mut terminal.terminal) {
            *active_session = Some(terminal);
            return Err(error);
        }
        self.finished = true;
        Ok(terminal.notices)
    }
}

impl Drop for TerminalUiSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let Ok(mut active) = active_terminal() else {
            return;
        };
        let Some(mut terminal) = active.take() else {
            return;
        };
        let _result = leave_terminal(&mut terminal.terminal);
    }
}

pub(crate) fn notice(message: String) {
    let Ok(mut active) = active_terminal() else {
        println!("{message}");
        return;
    };
    let Some(active) = active.as_mut() else {
        println!("{message}");
        return;
    };
    active.notices.push(message);
}

pub(crate) fn present_notices(title: &str) -> io::Result<()> {
    let notices = take_notices()?;
    if notices.is_empty() {
        return Ok(());
    }
    show_message(title, &notices).map(|_| ())
}

pub(crate) fn show_message(title: &str, messages: &[String]) -> io::Result<bool> {
    with_terminal(|terminal| {
        acknowledge_screen(terminal, |frame| {
            draw_message(frame.area(), frame, title, messages);
        })
    })
}

pub(crate) fn show_skill_status(statuses: &[SkillDestinationStatus]) -> io::Result<bool> {
    with_terminal(|terminal| {
        acknowledge_screen(terminal, |frame| draw_skill_status(frame, statuses))
    })
}

pub(crate) fn show_progress(title: &str, message: &str) -> io::Result<()> {
    with_terminal(|terminal| {
        terminal
            .draw(|frame| draw_progress(frame.area(), frame, title, message))
            .map(|_| ())
    })
}

pub(crate) fn choose_dashboard(dashboard: &Dashboard) -> io::Result<usize> {
    with_terminal(|terminal| select_dashboard(terminal, dashboard))
}

pub(crate) fn choose(title: &str, items: &[String]) -> io::Result<usize> {
    with_terminal(|terminal| select_one(terminal, title, items, 0))
}

pub(crate) fn choose_detailed(title: &str, items: &[DetailedChoice]) -> io::Result<usize> {
    with_terminal(|terminal| select_detailed(terminal, title, items))
}

pub(crate) fn confirm(title: &str, default_yes: bool) -> io::Result<bool> {
    let choices = vec!["Yes, continue".to_owned(), "No, cancel".to_owned()];
    let default = usize::from(!default_yes);
    with_terminal(|terminal| select_one(terminal, title, &choices, default))
        .map(|selected| selected == 0)
        .or_else(|error| {
            if error.kind() == io::ErrorKind::Interrupted {
                Ok(false)
            } else {
                Err(error)
            }
        })
}

pub(crate) fn choose_many(
    title: &str,
    items: &[String],
    initially_selected: Vec<bool>,
) -> io::Result<Vec<bool>> {
    with_terminal(|terminal| select_many(terminal, title, items, initially_selected))
}

pub(crate) fn read_text(label: &str, default: Option<&str>, secret: bool) -> io::Result<String> {
    with_terminal(|terminal| read_value(terminal, label, default, secret))
}

pub(crate) fn move_selection(selected: usize, offset: isize, item_count: usize) -> usize {
    if offset.is_negative() {
        return selected.checked_sub(1).unwrap_or(item_count - 1);
    }
    (selected + 1) % item_count
}

fn with_terminal<T>(task: impl FnOnce(&mut AppTerminal) -> io::Result<T>) -> io::Result<T> {
    let mut active = active_terminal()?;
    if let Some(active) = active.as_mut() {
        return task(&mut active.terminal);
    }
    drop(active);
    let mut terminal = enter_terminal()?;
    let result = task(&mut terminal);
    leave_terminal(&mut terminal)?;
    result
}

fn take_notices() -> io::Result<Vec<String>> {
    let mut active = active_terminal()?;
    let Some(active) = active.as_mut() else {
        return Ok(Vec::new());
    };
    Ok(std::mem::take(&mut active.notices))
}

fn active_terminal() -> io::Result<MutexGuard<'static, Option<ActiveTerminal>>> {
    terminal_session()
        .lock()
        .map_err(|_| io::Error::other("the TUI session is unavailable"))
}

fn terminal_session() -> &'static Mutex<Option<ActiveTerminal>> {
    static TERMINAL_SESSION: OnceLock<Mutex<Option<ActiveTerminal>>> = OnceLock::new();
    TERMINAL_SESSION.get_or_init(|| Mutex::new(None))
}

fn enter_terminal() -> io::Result<AppTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn leave_terminal(terminal: &mut AppTerminal) -> io::Result<()> {
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    disable_raw_mode()?;
    terminal.show_cursor()
}

fn select_dashboard(terminal: &mut AppTerminal, dashboard: &Dashboard) -> io::Result<usize> {
    let mut selected = 0;
    let mut scroll = 0;
    loop {
        terminal.draw(|frame| draw_dashboard(frame.area(), frame, dashboard, selected, scroll))?;
        let area = dashboard_action_items_area(terminal.size()?.into());
        match selection_input(
            &event::read()?,
            area,
            dashboard.actions.len(),
            selected,
            scroll,
        ) {
            SelectionInput::Select(choice) => return Ok(choice),
            SelectionInput::Cancel => return Ok(dashboard.actions.len() - 1),
            SelectionInput::Move(offset) => {
                selected = move_selection(selected, offset, dashboard.actions.len());
                scroll = visible_scroll(area, selected, scroll);
            }
            SelectionInput::Ignore => {}
        }
    }
}

fn acknowledge_screen(
    terminal: &mut AppTerminal,
    mut draw: impl FnMut(&mut ratatui::Frame),
) -> io::Result<bool> {
    loop {
        terminal.draw(&mut draw)?;
        match event::read()? {
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Enter | KeyCode::Char('q')) =>
            {
                return Ok(true);
            }
            Event::Key(key) if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc => {
                return Ok(false);
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                return Ok(true);
            }
            _ => {}
        }
    }
}

fn select_one(
    terminal: &mut AppTerminal,
    title: &str,
    items: &[String],
    initial: usize,
) -> io::Result<usize> {
    let mut selected = initial;
    let mut scroll = 0;
    loop {
        terminal
            .draw(|frame| draw_selection(frame.area(), frame, title, items, selected, scroll))?;
        let area = selection_items_area(terminal.size()?.into());
        match selection_input(&event::read()?, area, items.len(), selected, scroll) {
            SelectionInput::Select(choice) => return Ok(choice),
            SelectionInput::Cancel => return Err(cancelled()),
            SelectionInput::Move(offset) => {
                selected = move_selection(selected, offset, items.len());
                scroll = visible_scroll(area, selected, scroll);
            }
            SelectionInput::Ignore => {}
        }
    }
}

fn select_detailed(
    terminal: &mut AppTerminal,
    title: &str,
    items: &[DetailedChoice],
) -> io::Result<usize> {
    let mut selected = 0;
    loop {
        terminal
            .draw(|frame| draw_detailed_selection(frame.area(), frame, title, items, selected))?;
        let area = selection_items_area(terminal.size()?.into());
        match detailed_selection_input(&event::read()?, area, items.len(), selected) {
            SelectionInput::Select(choice) => return Ok(choice),
            SelectionInput::Cancel => return Err(cancelled()),
            SelectionInput::Move(offset) => {
                selected = move_selection(selected, offset, items.len());
            }
            SelectionInput::Ignore => {}
        }
    }
}

fn select_many(
    terminal: &mut AppTerminal,
    title: &str,
    items: &[String],
    mut selected_items: Vec<bool>,
) -> io::Result<Vec<bool>> {
    let mut selected = 0;
    let mut scroll = 0;
    loop {
        terminal.draw(|frame| {
            draw_multi_selection(
                frame.area(),
                frame,
                title,
                items,
                &selected_items,
                selected,
                scroll,
            );
        })?;
        let area = selection_items_area(terminal.size()?.into());
        match multi_selection_input(&event::read()?, area, items.len(), selected, scroll) {
            MultiSelectionInput::Save => return Ok(selected_items),
            MultiSelectionInput::Cancel => return Err(cancelled()),
            MultiSelectionInput::Toggle(index) => selected_items[index] = !selected_items[index],
            MultiSelectionInput::Move(offset) => {
                selected = move_selection(selected, offset, items.len());
                scroll = visible_scroll(area, selected, scroll);
            }
            MultiSelectionInput::Ignore => {}
        }
    }
}

fn read_value(
    terminal: &mut AppTerminal,
    label: &str,
    default: Option<&str>,
    secret: bool,
) -> io::Result<String> {
    let mut value = default.unwrap_or_default().to_owned();
    let mut cursor = value.chars().count();
    loop {
        terminal
            .draw(|frame| draw_text_input(frame.area(), frame, label, &value, cursor, secret))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter if !value.is_empty() => return Ok(value),
                KeyCode::Backspace => cursor = remove_before_cursor(&mut value, cursor),
                KeyCode::Delete => remove_at_cursor(&mut value, cursor),
                KeyCode::Left => cursor = cursor.saturating_sub(1),
                KeyCode::Right => cursor = (cursor + 1).min(value.chars().count()),
                KeyCode::Home => cursor = 0,
                KeyCode::End => cursor = value.chars().count(),
                KeyCode::Char(character) => {
                    cursor = insert_at_cursor(&mut value, cursor, &character.to_string());
                }
                KeyCode::Esc => return Err(cancelled()),
                _ => {}
            },
            Event::Paste(pasted) => cursor = insert_at_cursor(&mut value, cursor, &pasted),
            _ => {}
        }
    }
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "operation cancelled")
}

enum SelectionInput {
    Select(usize),
    Cancel,
    Move(isize),
    Ignore,
}

enum MultiSelectionInput {
    Save,
    Cancel,
    Toggle(usize),
    Move(isize),
    Ignore,
}

fn selection_input(
    event: &Event,
    area: Rect,
    item_count: usize,
    selected: usize,
    scroll: usize,
) -> SelectionInput {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => key_input(key.code, selected),
        Event::Mouse(mouse) => mouse_input(
            area,
            mouse.kind,
            mouse.column,
            mouse.row,
            item_count,
            scroll,
        ),
        _ => SelectionInput::Ignore,
    }
}

fn multi_selection_input(
    event: &Event,
    area: Rect,
    item_count: usize,
    selected: usize,
    scroll: usize,
) -> MultiSelectionInput {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => multi_key_input(key.code, selected),
        Event::Mouse(mouse) => multi_mouse_input(
            area,
            mouse.kind,
            mouse.column,
            mouse.row,
            item_count,
            scroll,
        ),
        _ => MultiSelectionInput::Ignore,
    }
}

fn detailed_selection_input(
    event: &Event,
    area: Rect,
    item_count: usize,
    selected: usize,
) -> SelectionInput {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => key_input(key.code, selected),
        Event::Mouse(mouse) => {
            detailed_mouse_input(area, mouse.kind, mouse.column, mouse.row, item_count)
        }
        _ => SelectionInput::Ignore,
    }
}

fn key_input(code: KeyCode, selected: usize) -> SelectionInput {
    match code {
        KeyCode::Enter => SelectionInput::Select(selected),
        KeyCode::Esc | KeyCode::Char('q') => SelectionInput::Cancel,
        KeyCode::Up | KeyCode::Char('k') => SelectionInput::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => SelectionInput::Move(1),
        _ => SelectionInput::Ignore,
    }
}

fn multi_key_input(code: KeyCode, selected: usize) -> MultiSelectionInput {
    match code {
        KeyCode::Enter => MultiSelectionInput::Save,
        KeyCode::Esc | KeyCode::Char('q') => MultiSelectionInput::Cancel,
        KeyCode::Char(' ') => MultiSelectionInput::Toggle(selected),
        KeyCode::Up | KeyCode::Char('k') => MultiSelectionInput::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => MultiSelectionInput::Move(1),
        _ => MultiSelectionInput::Ignore,
    }
}

fn mouse_input(
    area: Rect,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    item_count: usize,
    scroll: usize,
) -> SelectionInput {
    selected_from_click(area, kind, column, row, item_count, scroll)
        .map_or(SelectionInput::Ignore, SelectionInput::Select)
}

fn multi_mouse_input(
    area: Rect,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    item_count: usize,
    scroll: usize,
) -> MultiSelectionInput {
    selected_from_click(area, kind, column, row, item_count, scroll)
        .map_or(MultiSelectionInput::Ignore, MultiSelectionInput::Toggle)
}

fn detailed_mouse_input(
    area: Rect,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    item_count: usize,
) -> SelectionInput {
    detailed_selected_from_click(area, kind, column, row, item_count)
        .map_or(SelectionInput::Ignore, SelectionInput::Select)
}

fn selected_from_click(
    area: Rect,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    item_count: usize,
    scroll: usize,
) -> Option<usize> {
    if kind != MouseEventKind::Down(MouseButton::Left) || !contains(area, column, row) {
        return None;
    }
    let index = scroll + usize::from(row.saturating_sub(area.y));
    (index < item_count).then_some(index)
}

fn detailed_selected_from_click(
    area: Rect,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    item_count: usize,
) -> Option<usize> {
    if kind != MouseEventKind::Down(MouseButton::Left) || !contains(area, column, row) {
        return None;
    }
    let index = usize::from(row.saturating_sub(area.y)) / 2;
    (index < item_count).then_some(index)
}

fn draw_dashboard(
    area: Rect,
    frame: &mut ratatui::Frame,
    dashboard: &Dashboard,
    selected: usize,
    scroll: usize,
) {
    let layout = selection_layout(area);
    draw_screen_shell(frame, layout, &dashboard.title, &dashboard.navigation_hint);
    frame.render_widget(
        Paragraph::new(tui_copy().menu_subtitle.as_str()).style(Style::default().fg(MUTED)),
        Rect::new(layout.title.x, layout.title.y + 1, layout.title.width, 1),
    );
    let sections = dashboard_sections(layout.content);
    frame.render_widget(Block::default().style(base_style()), layout.content);
    draw_actions(frame, sections[0], dashboard, selected, scroll);
    draw_overview(frame, sections[1], dashboard);
}

fn draw_selection(
    area: Rect,
    frame: &mut ratatui::Frame,
    title: &str,
    items: &[String],
    selected: usize,
    scroll: usize,
) {
    let layout = selection_layout(area);
    draw_screen_shell(frame, layout, title, SINGLE_SELECT_HELP);
    draw_compact_surface(frame, layout.content, items.len());
    draw_list(frame, selection_items_area(area), items, selected, scroll);
}

fn draw_detailed_selection(
    area: Rect,
    frame: &mut ratatui::Frame,
    title: &str,
    items: &[DetailedChoice],
    selected: usize,
) {
    let layout = selection_layout(area);
    draw_screen_shell(frame, layout, title, SINGLE_SELECT_HELP);
    draw_compact_surface(frame, layout.content, items.len() * 2);
    let items = items.iter().map(|item| {
        ListItem::new(vec![
            Line::styled(
                item.label.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::styled(item.description.as_str(), Style::default().fg(MUTED)),
        ])
    });
    let list = List::new(items)
        .highlight_style(highlight_style())
        .highlight_symbol(SELECTED_MARKER);
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, selection_items_area(area), &mut state);
}

fn draw_multi_selection(
    area: Rect,
    frame: &mut ratatui::Frame,
    title: &str,
    items: &[String],
    selected_items: &[bool],
    selected: usize,
    scroll: usize,
) {
    let items = items
        .iter()
        .zip(selected_items)
        .map(|(item, checked)| {
            format!(
                "{}{}",
                if *checked {
                    CHECKED_MARKER
                } else {
                    UNCHECKED_MARKER
                },
                item
            )
        })
        .collect::<Vec<_>>();
    let layout = selection_layout(area);
    draw_screen_shell(frame, layout, title, MULTI_SELECT_HELP);
    draw_compact_surface(frame, layout.content, items.len());
    draw_list(frame, selection_items_area(area), &items, selected, scroll);
}

fn draw_text_input(
    area: Rect,
    frame: &mut ratatui::Frame,
    label: &str,
    value: &str,
    cursor: usize,
    secret: bool,
) {
    let layout = selection_layout(area);
    draw_screen_shell(frame, layout, label, INPUT_HELP);
    draw_compact_surface(frame, layout.content, 5);
    let content = selection_items_area(area);
    frame.render_widget(
        Paragraph::new(tui_copy().input_description.as_str()).style(Style::default().fg(MUTED)),
        content,
    );
    frame.render_widget(
        Paragraph::new(input_display(value, cursor, secret))
            .block(framed_block(&tui_copy().input_value_label, ACCENT)),
        input_area(content),
    );
}

fn draw_message(area: Rect, frame: &mut ratatui::Frame, title: &str, messages: &[String]) {
    let layout = selection_layout(area);
    draw_screen_shell(frame, layout, title, "Enter continuar · Esc volver");
    let content = messages.join("\n\n");
    let rows = message_rows(&content, selection_items_area(area).width);
    let paragraph = Paragraph::new(content)
        .style(Style::default().fg(FOREGROUND))
        .wrap(Wrap { trim: false });
    draw_compact_surface(frame, layout.content, rows);
    frame.render_widget(paragraph, selection_items_area(area));
}

fn message_rows(content: &str, width: u16) -> usize {
    let width = usize::from(width).max(1);
    content
        .lines()
        .map(|line| {
            let mut rows = 1;
            let mut occupied = 0;
            for word in line.split_inclusive(' ') {
                let length = Line::from(word).width();
                if occupied > 0 && occupied + length > width {
                    rows += 1;
                    occupied = 0;
                }
                rows += length.saturating_sub(1) / width;
                occupied += length.saturating_sub(1) % width + usize::from(length > 0);
            }
            rows
        })
        .sum()
}

fn draw_progress(area: Rect, frame: &mut ratatui::Frame, title: &str, message: &str) {
    let layout = selection_layout(area);
    draw_screen_shell(frame, layout, title, "");
    draw_compact_surface(frame, layout.content, 3);
    frame.render_widget(
        Paragraph::new(vec![
            section_heading(message),
            Line::default(),
            Line::from(tui_copy().progress_description.as_str()),
        ]),
        selection_items_area(area),
    );
}

fn draw_compact_surface(frame: &mut ratatui::Frame, area: Rect, rows: usize) {
    let height = u16::try_from(rows)
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(area.height);
    frame.render_widget(
        Block::default().style(surface_style()),
        Rect::new(area.x, area.y, area.width, height),
    );
}

fn draw_skill_status(frame: &mut ratatui::Frame, statuses: &[SkillDestinationStatus]) {
    let layout = selection_layout(frame.area());
    let copy = tui_copy();
    draw_screen_shell(
        frame,
        layout,
        &copy.skills_status_title,
        "Enter continuar · Esc volver",
    );
    frame.render_widget(
        Paragraph::new(copy.skills_status_subtitle.as_str()).style(Style::default().fg(MUTED)),
        Rect::new(layout.title.x, layout.title.y + 1, layout.title.width, 1),
    );
    if layout.content.width < WIDE_LAYOUT_WIDTH {
        draw_compact_skill_status(frame, layout.content, statuses);
        return;
    }
    for (index, status) in statuses.iter().enumerate() {
        draw_skill_card(frame, skill_card_area(layout.content, index), status);
    }
}

fn skill_card_area(area: Rect, index: usize) -> Rect {
    let width = area.width.saturating_sub(2) / 2;
    let height = if area.height >= 14 { 6 } else { 5 };
    let column = u16::try_from(index % 2).unwrap_or_default();
    let row = u16::try_from(index / 2).unwrap_or_default();
    Rect::new(
        area.x + column * (width + 2),
        area.y + row * (height + 1),
        width,
        height,
    )
}

fn draw_skill_card(frame: &mut ratatui::Frame, area: Rect, status: &SkillDestinationStatus) {
    frame.render_widget(Block::default().style(surface_style()), area);
    let inner = panel_inner(area);
    frame.render_widget(Paragraph::new(skill_heading(status)), inner);
    let metrics = skill_metrics(status);
    let offset = inner.height.saturating_sub(2);
    for (index, metric) in metrics.into_iter().enumerate() {
        let column = u16::try_from(index % 2).unwrap_or_default();
        let row = u16::try_from(index / 2).unwrap_or_default();
        let target = Rect::new(
            inner.x + column * (inner.width / 2),
            inner.y + offset + row,
            inner.width / 2,
            1,
        );
        frame.render_widget(Paragraph::new(metric), target);
    }
}

fn skill_heading(status: &SkillDestinationStatus) -> Line<'_> {
    Line::from(vec![
        Span::styled("●  ", state_style(status.is_ready())),
        Span::styled(
            status.name.as_str(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ])
}

fn skill_metrics(status: &SkillDestinationStatus) -> [Line<'static>; 4] {
    let labels = &tui_copy().skills_metric_labels;
    let counts = [
        status.current,
        status.updates,
        status.missing,
        status.conflicts,
    ];
    std::array::from_fn(|index| {
        Line::from(vec![
            Span::styled(
                format!("{}  ", counts[index]),
                Style::default().fg(FOREGROUND).add_modifier(Modifier::BOLD),
            ),
            Span::styled(labels[index].as_str(), Style::default().fg(MUTED)),
        ])
    })
}

fn draw_compact_skill_status(
    frame: &mut ratatui::Frame,
    area: Rect,
    statuses: &[SkillDestinationStatus],
) {
    draw_compact_surface(frame, area, statuses.len() * 2);
    let mut lines = Vec::new();
    for status in statuses {
        lines.push(skill_heading(status));
        let metrics = skill_metrics(status)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join(" · ");
        lines.push(Line::from(metrics));
    }
    frame.render_widget(Paragraph::new(lines), panel_inner(area));
}

fn draw_screen_shell(frame: &mut ratatui::Frame, layout: ScreenLayout, title: &str, help: &str) {
    frame.render_widget(Block::default().style(base_style()), frame.area());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◆  ", Style::default().fg(ACCENT)),
            Span::styled(APP_NAME, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("   {APP_CONTEXT}  /  v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(MUTED),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(SURFACE)),
        ),
        layout.header,
    );
    frame.render_widget(
        Paragraph::new(title)
            .style(Style::default().fg(FOREGROUND).add_modifier(Modifier::BOLD))
            .wrap(Wrap { trim: false }),
        layout.title,
    );
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(MUTED)),
        layout.footer,
    );
}

fn draw_list(
    frame: &mut ratatui::Frame,
    area: Rect,
    items: &[String],
    selected: usize,
    scroll: usize,
) {
    let items = items.iter().map(|label| ListItem::new(label.as_str()));
    let list = List::new(items)
        .highlight_style(highlight_style())
        .highlight_symbol(SELECTED_MARKER);
    let mut state = ListState::default()
        .with_selected(Some(selected))
        .with_offset(scroll);
    frame.render_stateful_widget(list, area, &mut state);
}

fn panel_area(area: Rect) -> Rect {
    let horizontal_margin = PANEL_MARGIN.min(area.width / 4);
    let width = area
        .width
        .saturating_sub(horizontal_margin * 2)
        .min(MAX_PANEL_WIDTH);
    let height = area
        .height
        .saturating_sub(PANEL_MARGIN * 2)
        .max(8)
        .min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[derive(Clone, Copy)]
struct ScreenLayout {
    header: Rect,
    title: Rect,
    content: Rect,
    footer: Rect,
}

fn selection_layout(area: Rect) -> ScreenLayout {
    let panel = panel_area(area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(panel);
    ScreenLayout {
        header: rows[0],
        title: rows[1],
        content: rows[2],
        footer: rows[3],
    }
}

fn selection_items_area(area: Rect) -> Rect {
    panel_inner(selection_layout(area).content)
}

fn dashboard_sections(content: Rect) -> [Rect; 2] {
    let wide = content.width >= WIDE_LAYOUT_WIDTH;
    let sections = Layout::default()
        .direction(if wide {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([
            Constraint::Length(if wide { 42.min(content.width / 2) } else { 10 }),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(content);
    [sections[0], sections[2]]
}

fn dashboard_action_items_area(area: Rect) -> Rect {
    action_items_area(dashboard_sections(selection_layout(area).content)[0])
}

fn panel_inner(area: Rect) -> Rect {
    area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    })
}

fn action_items_area(area: Rect) -> Rect {
    let inner = panel_inner(area);
    Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2).min(6),
    )
}

fn input_area(content: Rect) -> Rect {
    Rect::new(
        content.x,
        content.y.saturating_add(2),
        content.width,
        3.min(content.height.saturating_sub(2)),
    )
}

fn visible_scroll(area: Rect, selected: usize, current: usize) -> usize {
    let visible_items = usize::from(area.height).max(1);
    if selected < current {
        return selected;
    }
    if selected >= current + visible_items {
        return selected + 1 - visible_items;
    }
    current
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn input_display(value: &str, cursor: usize, secret: bool) -> String {
    let visible = if secret {
        "•".repeat(value.chars().count())
    } else {
        value.to_owned()
    };
    let cursor_position = character_byte_index(&visible, cursor);
    let (before, after) = visible.split_at(cursor_position);
    format!("{before}▏{after}")
}

fn character_byte_index(value: &str, character_index: usize) -> usize {
    value
        .char_indices()
        .nth(character_index)
        .map_or(value.len(), |(byte_index, _)| byte_index)
}

fn insert_at_cursor(value: &mut String, cursor: usize, inserted: &str) -> usize {
    let byte_index = character_byte_index(value, cursor);
    value.insert_str(byte_index, inserted);
    cursor + inserted.chars().count()
}

fn remove_before_cursor(value: &mut String, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let start = character_byte_index(value, cursor - 1);
    let end = character_byte_index(value, cursor);
    value.replace_range(start..end, "");
    cursor - 1
}

fn remove_at_cursor(value: &mut String, cursor: usize) {
    let start = character_byte_index(value, cursor);
    let end = character_byte_index(value, cursor + 1);
    if start != end {
        value.replace_range(start..end, "");
    }
}

fn draw_overview(frame: &mut ratatui::Frame, area: Rect, dashboard: &Dashboard) {
    frame.render_widget(Block::default().style(surface_style()), area);
    let roomy = area.height >= 18;
    let mut lines = vec![section_heading(&tui_copy().menu_overview_title)];
    if roomy {
        lines.push(Line::default());
    }
    for (index, text) in dashboard.overview.iter().enumerate() {
        lines.push(Line::styled(
            text.as_str(),
            overview_style(index, dashboard),
        ));
        if roomy {
            lines.push(Line::default());
        }
    }
    lines.push(section_heading(&tui_copy().clients_label));
    if roomy {
        lines.push(Line::default());
    }
    lines.extend(
        dashboard
            .clients
            .iter()
            .map(|text| Line::from(text.as_str())),
    );
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        panel_inner(area),
    );
}

fn overview_style(index: usize, dashboard: &Dashboard) -> Style {
    match index {
        0 => state_style(dashboard.configured),
        2 => state_style(dashboard.server_installed),
        _ => Style::default().fg(MUTED),
    }
}

fn state_style(available: bool) -> Style {
    let color = if available {
        Color::Green
    } else {
        Color::Yellow
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn draw_actions(
    frame: &mut ratatui::Frame,
    area: Rect,
    dashboard: &Dashboard,
    selected: usize,
    scroll: usize,
) {
    frame.render_widget(Block::default().style(surface_style()), area);
    frame.render_widget(
        Paragraph::new(section_heading(&tui_copy().menu_navigation_title)),
        panel_inner(area),
    );
    let items_area = action_items_area(area);
    draw_list(frame, items_area, &dashboard.actions, selected, scroll);
    draw_action_description(frame, area, selected);
}

fn draw_action_description(frame: &mut ratatui::Frame, area: Rect, selected: usize) {
    let Some(description) = tui_copy().menu_action_descriptions.get(selected) else {
        return;
    };
    let inner = panel_inner(area);
    let start = action_items_area(area).bottom().saturating_add(2);
    let details = Rect::new(
        inner.x,
        start,
        inner.width,
        inner.bottom().saturating_sub(start),
    );
    frame.render_widget(
        Paragraph::new(description.as_str())
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: false }),
        details,
    );
}

fn section_heading(text: &str) -> Line<'_> {
    Line::styled(
        text,
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )
}

fn base_style() -> Style {
    Style::default().bg(BACKGROUND).fg(FOREGROUND)
}

fn surface_style() -> Style {
    Style::default().bg(SURFACE).fg(FOREGROUND)
}

fn framed_block(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
}

fn highlight_style() -> Style {
    Style::default()
        .fg(FOREGROUND)
        .bg(SELECTION_BACKGROUND)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_outside_the_list_has_no_effect() {
        let area = Rect::new(10, 4, 20, 5);
        let input = mouse_input(area, MouseEventKind::Down(MouseButton::Left), 1, 4, 3, 0);
        assert!(matches!(input, SelectionInput::Ignore));
    }

    #[test]
    fn click_uses_the_visible_list_offset() {
        let area = Rect::new(10, 4, 20, 5);
        let input = mouse_input(area, MouseEventKind::Down(MouseButton::Left), 11, 5, 8, 3);
        assert!(matches!(input, SelectionInput::Select(4)));
    }

    #[test]
    fn dashboard_click_targets_follow_the_responsive_layout() {
        for width in [60, 80, 120] {
            let area = dashboard_action_items_area(Rect::new(0, 0, width, 24));
            assert!(area.height >= 6);
            let input = mouse_input(
                area,
                MouseEventKind::Down(MouseButton::Left),
                area.x,
                area.y + 3,
                6,
                0,
            );
            assert!(matches!(input, SelectionInput::Select(3)));
        }
    }

    #[test]
    fn dashboard_renders_navigation_and_clients_in_a_standard_terminal() {
        let dashboard = Dashboard::new(
            "Inicio".into(),
            vec!["Configuración".into(); 3],
            vec!["Cliente visible".into(); 5],
            vec!["Acción".into(); 6],
            "Ayuda".into(),
            false,
            false,
        );
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| draw_dashboard(frame.area(), frame, &dashboard, 3, 0))
            .expect("render");
        let symbols: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert_eq!(symbols.matches("Cliente visible").count(), 5);
        let selection = dashboard_action_items_area(Rect::new(0, 0, 80, 24));
        assert_eq!(
            terminal.backend().buffer()[(selection.x, selection.y + 3)].bg,
            SELECTION_BACKGROUND
        );
    }

    #[test]
    fn skill_cards_keep_every_destination_and_metric_visible_at_eighty_columns() {
        let statuses = ["Agent Skills", "Codex", "Claude Code", "Windsurf"].map(|name| {
            SkillDestinationStatus {
                name: name.into(),
                current: 1,
                updates: 2,
                missing: 3,
                conflicts: 4,
            }
        });
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("test terminal");
        terminal
            .draw(|frame| draw_skill_status(frame, &statuses))
            .expect("render");
        let symbols: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        for status in &statuses {
            assert!(symbols.contains(&status.name));
        }
        for label in &tui_copy().skills_metric_labels {
            assert_eq!(symbols.matches(label).count(), statuses.len());
        }
    }

    #[test]
    fn escape_cancels_instead_of_selecting_an_option() {
        assert!(matches!(key_input(KeyCode::Esc, 1), SelectionInput::Cancel));
    }

    #[test]
    fn input_masks_secret_values_and_keeps_the_cursor() {
        assert_eq!(input_display("secret", 3, true), "•••▏•••");
    }

    #[test]
    fn input_edits_at_the_cursor() {
        let mut value = "ac".to_owned();
        let cursor = insert_at_cursor(&mut value, 1, "b");
        assert_eq!((value, cursor), ("abc".to_owned(), 2));
    }
}
