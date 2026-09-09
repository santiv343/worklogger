use std::io::{self, Stdout};

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
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

const APP_NAME: &str = "WORKLOGGER";
const APP_CONTEXT: &str = "MCP";
const MAX_PANEL_WIDTH: u16 = 88;
const PANEL_MARGIN: u16 = 2;
const SELECTED_MARKER: &str = "› ";
const CHECKED_MARKER: &str = "[x] ";
const UNCHECKED_MARKER: &str = "[ ] ";
const SINGLE_SELECT_HELP: &str = "↑↓ navegar · Enter elegir · Esc cancelar · Click seleccionar";
const MULTI_SELECT_HELP: &str = "↑↓ navegar · Espacio marcar · Enter continuar · Esc cancelar";
const INPUT_HELP: &str = "Enter continuar · Esc cancelar";

pub(crate) struct Dashboard {
    title: String,
    overview: Vec<String>,
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
        actions: Vec<String>,
        navigation_hint: String,
        configured: bool,
        server_installed: bool,
    ) -> Self {
        Self {
            title,
            overview,
            actions,
            navigation_hint,
            configured,
            server_installed,
        }
    }
}

type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

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
    let choices = vec!["Sí, continuar".to_owned(), "No, cancelar".to_owned()];
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
    let mut terminal = enter_terminal()?;
    let result = task(&mut terminal);
    leave_terminal(&mut terminal)?;
    result
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
    io::Error::new(io::ErrorKind::Interrupted, "operación cancelada")
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
    frame.render_widget(Clear, area);
    let panel = panel_area(area);
    let block = framed_block(&dashboard.title, Color::Cyan);
    let content = block.inner(panel);
    frame.render_widget(block, panel);
    draw_overview(frame, dashboard_overview_area(content), dashboard);
    draw_actions(
        frame,
        dashboard_actions_area(panel),
        dashboard,
        selected,
        scroll,
    );
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
    draw_list(frame, layout.content, items, selected, scroll);
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
    let items = items.iter().map(|item| {
        ListItem::new(vec![
            Line::styled(
                item.label.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                item.description.as_str(),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    });
    let list = List::new(items)
        .block(framed_block("", Color::DarkGray))
        .highlight_style(highlight_style())
        .highlight_symbol(SELECTED_MARKER);
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, layout.content, &mut state);
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
    draw_list(frame, layout.content, &items, selected, scroll);
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
    frame.render_widget(
        Paragraph::new("Ingresá el valor. Podés editar el valor predeterminado."),
        layout.content,
    );
    frame.render_widget(
        Paragraph::new(input_display(value, cursor, secret))
            .block(framed_block(" Valor ", Color::Cyan)),
        input_area(layout.content),
    );
}

fn draw_screen_shell(frame: &mut ratatui::Frame, layout: ScreenLayout, title: &str, help: &str) {
    frame.render_widget(Clear, layout.panel);
    frame.render_widget(framed_block("", Color::Cyan), layout.panel);
    frame.render_widget(
        Paragraph::new(Line::from(format!("{APP_NAME}  ·  {APP_CONTEXT}"))).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        layout.header,
    );
    frame.render_widget(
        Paragraph::new(title).style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        layout.title,
    );
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
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
        .block(framed_block("", Color::DarkGray))
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
    panel: Rect,
    header: Rect,
    title: Rect,
    content: Rect,
    footer: Rect,
}

fn selection_layout(area: Rect) -> ScreenLayout {
    let panel = panel_area(area);
    let content = Block::default().borders(Borders::ALL).inner(panel);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(content);
    ScreenLayout {
        panel,
        header: rows[0],
        title: rows[1],
        content: rows[2],
        footer: rows[3],
    }
}

fn selection_items_area(area: Rect) -> Rect {
    framed_block("", Color::DarkGray).inner(selection_layout(area).content)
}

fn dashboard_overview_area(area: Rect) -> Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(7), Constraint::Length(7)])
        .split(area)[0]
}

fn dashboard_actions_area(area: Rect) -> Rect {
    let content = Block::default().borders(Borders::ALL).inner(area);
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(7), Constraint::Length(7)])
        .split(content)[1]
}

fn dashboard_action_items_area(area: Rect) -> Rect {
    framed_block("", Color::DarkGray).inner(dashboard_actions_area(panel_area(area)))
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
    let lines = dashboard
        .overview
        .iter()
        .enumerate()
        .map(|(index, text)| Line::styled(text, overview_style(index, dashboard)))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(framed_block(" Estado actual ", Color::DarkGray)),
        area,
    );
}

fn overview_style(index: usize, dashboard: &Dashboard) -> Style {
    match index {
        0 => state_style(dashboard.configured),
        3 => state_style(dashboard.server_installed),
        4 => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
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
    let items = dashboard
        .actions
        .iter()
        .map(|label| ListItem::new(label.as_str()));
    let title = format!(" Acciones · {} ", dashboard.navigation_hint);
    let list = List::new(items)
        .block(framed_block(&title, Color::DarkGray))
        .highlight_style(highlight_style())
        .highlight_symbol(SELECTED_MARKER);
    let mut state = ListState::default()
        .with_selected(Some(selected))
        .with_offset(scroll);
    frame.render_stateful_widget(list, area, &mut state);
}

fn framed_block(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
}

fn highlight_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
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
