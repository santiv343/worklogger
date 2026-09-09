use dioxus::prelude::*;

use crate::connection_model::AccessibleBoard;
use crate::copy::text;

#[component]
pub(crate) fn BoardSelector(
    id: &'static str,
    boards: Vec<AccessibleBoard>,
    selected: String,
    disabled: bool,
    on_select: EventHandler<String>,
) -> Element {
    let mut query = use_signal(String::new);
    let filtered_boards = filter_boards(&boards, &query());
    let selected_board = selected_board_outside_results(&boards, &filtered_boards, &selected);
    let has_results = !filtered_boards.is_empty();
    let result_summary = result_summary(filtered_boards.len(), boards.len());
    rsx! { div { class: "board-picker",
        input { id: "{id}-search", name: "{id}-search", r#type: "search", autocomplete: "off", spellcheck: "false", placeholder: text("setup.boardSearchPlaceholder"), value: query(), disabled, oninput: move |event| query.set(event.value()) }
        select { id, name: id, aria_describedby: "{id}-help {id}-results", required: true, disabled: disabled || !has_results, value: selected, oninput: move |event| {
            on_select.call(event.value());
            query.set(String::new());
        },
            if selected.is_empty() { option { value: "", disabled: true, {text("setup.boardPlaceholder")} } }
            if !has_results { option { value: "", disabled: true, {text("setup.boardNoMatches")} } }
            if let Some(board) = selected_board { option { value: board.id.to_string(), hidden: true, "{board_label(&board)}" } }
            for board in filtered_boards { option { value: board.id.to_string(), "{board_label(&board)}" } }
        }
        small { id: "{id}-results", class: "board-results", aria_live: "polite", "{result_summary}" }
    } }
}

fn result_summary(visible: usize, total: usize) -> String {
    format!("{}: {visible}/{total}", text("setup.boardResults"))
}

fn selected_board_outside_results(
    boards: &[AccessibleBoard],
    filtered_boards: &[AccessibleBoard],
    selected: &str,
) -> Option<AccessibleBoard> {
    let selected_is_visible = filtered_boards
        .iter()
        .any(|board| board.id.to_string() == selected);
    if selected_is_visible {
        return None;
    }
    boards
        .iter()
        .find(|board| board.id.to_string() == selected)
        .cloned()
}

fn filter_boards(boards: &[AccessibleBoard], query: &str) -> Vec<AccessibleBoard> {
    let normalized_query = query.trim().to_lowercase();
    boards
        .iter()
        .filter(|board| board_matches_query(board, &normalized_query))
        .cloned()
        .collect()
}

fn board_matches_query(board: &AccessibleBoard, query: &str) -> bool {
    query.is_empty()
        || board.name.to_lowercase().contains(query)
        || board.board_type.to_lowercase().contains(query)
        || board.id.to_string().contains(query)
        || board
            .project_key
            .as_ref()
            .is_some_and(|project_key| project_key.to_lowercase().contains(query))
}

fn board_label(board: &AccessibleBoard) -> String {
    board.project_key.as_ref().map_or_else(
        || format!("{} · {}", board.name, board.board_type),
        |project_key| format!("{project_key} · {} · {}", board.name, board.board_type),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_and_labels_boards_by_project_name_and_identifier() {
        let boards = vec![board(42, "Operaciones", "DEMO")];

        assert_eq!(filter_boards(&boards, "demo"), boards);
        assert_eq!(filter_boards(&boards, "operaciones"), boards);
        assert_eq!(filter_boards(&boards, "42"), boards);
        assert!(filter_boards(&boards, "otro").is_empty());
        assert_eq!(board_label(&boards[0]), "DEMO · Operaciones · scrum");
    }

    fn board(id: u64, name: &str, project_key: &str) -> AccessibleBoard {
        AccessibleBoard {
            id,
            name: name.to_owned(),
            board_type: "scrum".to_owned(),
            project_key: Some(project_key.to_owned()),
        }
    }
}
