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
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

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
    let mut terminal = enter_terminal()?;
    let result = select_action(&mut terminal, dashboard);
    leave_terminal(&mut terminal)?;
    result
}

pub(crate) fn move_selection(selected: usize, offset: isize, item_count: usize) -> usize {
    if offset.is_negative() {
        return selected.checked_sub(1).unwrap_or(item_count - 1);
    }
    (selected + 1) % item_count
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

fn select_action(terminal: &mut AppTerminal, dashboard: &Dashboard) -> io::Result<usize> {
    let mut selected = 0;
    loop {
        terminal.draw(|frame| draw_dashboard(frame.area(), frame, dashboard, selected))?;
        let event = event::read()?;
        let input = selection_input(
            &event,
            terminal.size()?.into(),
            dashboard.actions.len(),
            selected,
        );
        match input {
            SelectionInput::Select(choice) => return Ok(choice),
            SelectionInput::Move(offset) => {
                selected = move_selection(selected, offset, dashboard.actions.len());
            }
            SelectionInput::Ignore => {}
        }
    }
}

enum SelectionInput {
    Select(usize),
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

fn key_input(code: KeyCode, selected: usize, item_count: usize) -> SelectionInput {
    match code {
        KeyCode::Enter => SelectionInput::Select(selected),
        KeyCode::Esc | KeyCode::Char('q') => SelectionInput::Select(item_count - 1),
        KeyCode::Up | KeyCode::Char('k') => SelectionInput::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => SelectionInput::Move(1),
        _ => SelectionInput::Ignore,
    }
}

fn mouse_input(area: Rect, kind: MouseEventKind, row: u16, item_count: usize) -> SelectionInput {
    if kind != MouseEventKind::Down(MouseButton::Left) {
        return SelectionInput::Ignore;
    }
    let list = action_area(area);
    let index = usize::from(row.saturating_sub(list.y.saturating_add(1)));
    if row > list.y && index < item_count {
        return SelectionInput::Select(index);
    }
    SelectionInput::Ignore
}

fn draw_dashboard(area: Rect, frame: &mut ratatui::Frame, dashboard: &Dashboard, selected: usize) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            dashboard.title.as_str(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let content = block.inner(area);
    frame.render_widget(block, area);
    draw_overview(frame, overview_area(content), dashboard);
    draw_actions(frame, action_area(area), dashboard, selected);
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

fn draw_overview(frame: &mut ratatui::Frame, area: Rect, dashboard: &Dashboard) {
    let lines = dashboard
        .overview
        .iter()
        .enumerate()
        .map(|(index, text)| Line::styled(text, overview_style(index, dashboard)))
        .collect::<Vec<_>>();
    let overview = Paragraph::new(lines).block(
        Block::default()
            .title(" Estado ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(overview, area);
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
    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" Acciones · {} ", dashboard.navigation_hint))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicking_an_action_selects_its_row() {
        let terminal = Rect::new(0, 0, 80, 24);
        let action = action_area(terminal);
        let input = mouse_input(
            terminal,
            MouseEventKind::Down(MouseButton::Left),
            action.y + 2,
            6,
        );

        assert!(matches!(input, SelectionInput::Select(1)));
    }
}
