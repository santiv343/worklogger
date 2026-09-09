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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

const TUI_HELP: &str =
    "↑↓ mover · Enter elegir · Espacio marcar · Esc cancelar · Click seleccionar";
const SELECTED_MARKER: &str = "› ";
const CHECKED_MARKER: &str = "[x] ";
const UNCHECKED_MARKER: &str = "[ ] ";

pub(crate) struct Dashboard {
    title: String,
    overview: Vec<String>,
    actions: Vec<String>,
    navigation_hint: String,
    configured: bool,
    server_installed: bool,
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

pub(crate) fn confirm(title: &str, default_yes: bool) -> io::Result<bool> {
    let options = vec!["Sí".to_owned(), "No".to_owned()];
    let default = usize::from(!default_yes);
    with_terminal(|terminal| select_one(terminal, title, &options, default))
        .map(|selected| selected == 0)
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
    loop {
        terminal.draw(|frame| draw_dashboard(frame.area(), frame, dashboard, selected))?;
        let area = action_area(terminal.size()?.into());
        let input = selection_input(&event::read()?, area, dashboard.actions.len(), selected);
        match input {
            SelectionInput::Select(choice) => return Ok(choice),
            SelectionInput::Move(offset) => {
                selected = move_selection(selected, offset, dashboard.actions.len());
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
    loop {
        terminal.draw(|frame| draw_selection(frame.area(), frame, title, items, selected))?;
        let area = selection_area(terminal.size()?.into());
        let input = selection_input(&event::read()?, area, items.len(), selected);
        match input {
            SelectionInput::Select(choice) => return Ok(choice),
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
    loop {
        terminal.draw(|frame| {
            draw_multi_selection(frame.area(), frame, title, items, &selected_items, selected);
        })?;
        let area = selection_area(terminal.size()?.into());
        let input = multi_selection_input(&event::read()?, area, items.len(), selected);
        match input {
            MultiSelectionInput::Save => return Ok(selected_items),
            MultiSelectionInput::Toggle(index) => selected_items[index] = !selected_items[index],
            MultiSelectionInput::Move(offset) => {
                selected = move_selection(selected, offset, items.len());
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
    let mut value = String::new();
    loop {
        terminal
            .draw(|frame| draw_text_input(frame.area(), frame, label, default, &value, secret))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter if !value.is_empty() => return Ok(value),
                KeyCode::Enter if default.is_some() => {
                    return Ok(default.unwrap_or_default().to_owned());
                }
                KeyCode::Backspace => drop(value.pop()),
                KeyCode::Char(character) => value.push(character),
                KeyCode::Esc => return Err(cancelled()),
                _ => {}
            },
            Event::Paste(pasted) => value.push_str(&pasted),
            _ => {}
        }
    }
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "operación cancelada")
}

enum SelectionInput {
    Select(usize),
    Move(isize),
    Ignore,
}

enum MultiSelectionInput {
    Save,
    Toggle(usize),
    Move(isize),
    Ignore,
}

fn selection_input(
    event: &Event,
    area: Rect,
    item_count: usize,
    selected: usize,
) -> SelectionInput {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            key_input(key.code, selected, item_count)
        }
        Event::Mouse(mouse) => mouse_input(area, mouse.kind, mouse.row, item_count),
        _ => SelectionInput::Ignore,
    }
}

fn multi_selection_input(
    event: &Event,
    area: Rect,
    item_count: usize,
    selected: usize,
) -> MultiSelectionInput {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            multi_key_input(key.code, selected, item_count)
        }
        Event::Mouse(mouse) => multi_mouse_input(area, mouse.kind, mouse.row, item_count),
        _ => MultiSelectionInput::Ignore,
    }
}

fn key_input(code: KeyCode, selected: usize, item_count: usize) -> SelectionInput {
    match code {
        KeyCode::Enter => SelectionInput::Select(selected),
        KeyCode::Esc | KeyCode::Char('q') => SelectionInput::Select(item_count - 1),
        KeyCode::Up | KeyCode::Char('k') => SelectionInput::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => SelectionInput::Move(1),
        _ => SelectionInput::Ignore,
    }
}

fn multi_key_input(code: KeyCode, selected: usize, _item_count: usize) -> MultiSelectionInput {
    match code {
        KeyCode::Enter => MultiSelectionInput::Save,
        KeyCode::Char(' ') => MultiSelectionInput::Toggle(selected),
        KeyCode::Up | KeyCode::Char('k') => MultiSelectionInput::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => MultiSelectionInput::Move(1),
        _ => MultiSelectionInput::Ignore,
    }
}

fn mouse_input(area: Rect, kind: MouseEventKind, row: u16, item_count: usize) -> SelectionInput {
    selected_from_click(area, kind, row, item_count)
        .map_or(SelectionInput::Ignore, SelectionInput::Select)
}

fn multi_mouse_input(
    area: Rect,
    kind: MouseEventKind,
    row: u16,
    item_count: usize,
) -> MultiSelectionInput {
    selected_from_click(area, kind, row, item_count)
        .map_or(MultiSelectionInput::Ignore, MultiSelectionInput::Toggle)
}

fn selected_from_click(
    area: Rect,
    kind: MouseEventKind,
    row: u16,
    item_count: usize,
) -> Option<usize> {
    if kind != MouseEventKind::Down(MouseButton::Left) || row <= area.y {
        return None;
    }
    let index = usize::from(row.saturating_sub(area.y.saturating_add(1)));
    (index < item_count).then_some(index)
}

fn draw_dashboard(area: Rect, frame: &mut ratatui::Frame, dashboard: &Dashboard, selected: usize) {
    let block = framed_block(&dashboard.title, Color::Cyan);
    let content = block.inner(area);
    frame.render_widget(block, area);
    draw_overview(frame, overview_area(content), dashboard);
    draw_actions(frame, action_area(area), dashboard, selected);
}

fn draw_selection(
    area: Rect,
    frame: &mut ratatui::Frame,
    title: &str,
    items: &[String],
    selected: usize,
) {
    draw_list(frame, selection_area(area), title, items, selected, None);
}

fn draw_multi_selection(
    area: Rect,
    frame: &mut ratatui::Frame,
    title: &str,
    items: &[String],
    selected_items: &[bool],
    selected: usize,
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
    draw_list(
        frame,
        selection_area(area),
        title,
        &items,
        selected,
        Some("Enter guardar"),
    );
}

fn draw_text_input(
    area: Rect,
    frame: &mut ratatui::Frame,
    label: &str,
    default: Option<&str>,
    value: &str,
    secret: bool,
) {
    let block = framed_block(label, Color::Cyan);
    let content = block.inner(area);
    frame.render_widget(block, area);
    let display = input_display(value, default, secret);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(content);
    frame.render_widget(
        Paragraph::new("Escribí el valor y presioná Enter."),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(display).block(framed_block(" Valor ", Color::DarkGray)),
        rows[1],
    );
    frame.render_widget(Paragraph::new("Esc cancelar"), rows[2]);
}

fn input_display(value: &str, default: Option<&str>, secret: bool) -> String {
    if value.is_empty() {
        return default.map_or_else(String::new, |item| format!("{item} (predeterminado)"));
    }
    if secret {
        return "•".repeat(value.chars().count());
    }
    value.to_owned()
}

fn draw_list(
    frame: &mut ratatui::Frame,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    footer: Option<&str>,
) {
    let items = items.iter().map(|label| ListItem::new(label.as_str()));
    let title = footer.map_or_else(
        || format!(" {title} · {TUI_HELP} "),
        |item| format!(" {title} · {item} · {TUI_HELP} "),
    );
    let list = List::new(items)
        .block(framed_block(&title, Color::Cyan))
        .highlight_style(highlight_style())
        .highlight_symbol(SELECTED_MARKER);
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn overview_area(area: Rect) -> Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area)[0]
}

fn action_area(area: Rect) -> Rect {
    let content = Block::default().borders(Borders::ALL).inner(area);
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(content)[1]
}

fn selection_area(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

fn draw_overview(frame: &mut ratatui::Frame, area: Rect, dashboard: &Dashboard) {
    let lines = dashboard
        .overview
        .iter()
        .enumerate()
        .map(|(index, text)| Line::styled(text, overview_style(index, dashboard)))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(framed_block(" Estado ", Color::DarkGray)),
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

fn draw_actions(frame: &mut ratatui::Frame, area: Rect, dashboard: &Dashboard, selected: usize) {
    let items = dashboard
        .actions
        .iter()
        .map(|label| ListItem::new(label.as_str()));
    let title = format!(" Acciones · {} ", dashboard.navigation_hint);
    let list = List::new(items)
        .block(framed_block(&title, Color::DarkGray))
        .highlight_style(highlight_style())
        .highlight_symbol(SELECTED_MARKER);
    let mut state = ListState::default();
    state.select(Some(selected));
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
    fn clicking_an_action_selects_its_row() {
        let list = Rect::new(0, 0, 80, 24);
        let input = mouse_input(list, MouseEventKind::Down(MouseButton::Left), 2, 6);
        assert!(matches!(input, SelectionInput::Select(1)));
    }

    #[test]
    fn input_masks_secret_values() {
        assert_eq!(input_display("secret", None, true), "••••••");
    }

    #[test]
    fn multi_selection_toggles_the_highlighted_item() {
        assert!(matches!(
            multi_key_input(KeyCode::Char(' '), 2, 4),
            MultiSelectionInput::Toggle(2)
        ));
    }
}
