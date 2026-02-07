use chrono::{DateTime, Utc};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::task::{TaskDetails, TaskRecord};

use super::app::{AppState, DeleteConfirmState, HelpContext, ListMode, StatusKind};
use super::editor::{
    EditorFieldId, EditorMode, EditorState, MultiTaskPicker, PriorityPicker, StatusPicker,
    TaskPicker,
};

const STATUS_WIDTH: usize = 7;
const READY_WIDTH: usize = 1;
const ID_WIDTH: usize = 12;
const PRIORITY_WIDTH: usize = 3;
const HELP_KEY_WIDTH: usize = 14;
const COLOR_TEXT: Color = Color::Rgb(234, 236, 239);
const COLOR_MUTED: Color = Color::Rgb(160, 165, 172);
const COLOR_MUTED_DARK: Color = Color::Rgb(118, 124, 130);
const COLOR_BG_MUTED: Color = Color::Rgb(52, 56, 60);
const COLOR_INFO: Color = Color::Rgb(116, 198, 219);
const COLOR_WARNING: Color = Color::Rgb(244, 200, 98);
const COLOR_ERROR: Color = Color::Rgb(255, 107, 107);
const COLOR_SUCCESS: Color = Color::Rgb(126, 210, 146);
const COLOR_ACCENT: Color = Color::Rgb(122, 170, 255);
const COLOR_BORDER_LIST: Color = Color::Rgb(92, 126, 166);
const COLOR_BORDER_DETAIL: Color = Color::Rgb(180, 156, 92);
const COLOR_MAGENTA: Color = Color::Rgb(214, 140, 230);

pub fn render(frame: &mut Frame, app: &mut AppState) {
    let area = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(area);
    let tabs = chunks[0];
    let main = chunks[1];
    let footer = chunks[2];

    render_tabs(frame, app, tabs);

    if app.is_narrow() && !app.show_detail {
        render_list(frame, app, main);
    } else if app.is_narrow() {
        render_detail(frame, app, main);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
            .split(main);
        render_list(frame, app, chunks[0]);
        render_detail(frame, app, chunks[1]);
    }

    render_footer(frame, app, footer);

    if let Some(picker) = app
        .editor_priority_picker
        .as_ref()
        .or(app.priority_picker.as_ref())
    {
        render_priority_modal(frame, area, picker);
    }
    if let Some(picker) = app.parent_picker.as_ref() {
        render_task_picker_modal(frame, area, picker, "Parent");
    }
    if let Some(picker) = app.epic_picker.as_ref() {
        render_task_picker_modal(frame, area, picker, "Epic Filter");
    }
    if let Some(picker) = app.project_picker.as_ref() {
        render_task_picker_modal(frame, area, picker, "Project Filter");
    }
    if let Some(picker) = app.children_picker.as_ref() {
        render_multi_picker_modal(frame, area, picker, "Children");
    }
    if let Some(picker) = app.blocks_picker.as_ref() {
        render_multi_picker_modal(frame, area, picker, "Blocking");
    }
    if let Some(picker) = app.blocked_by_picker.as_ref() {
        render_multi_picker_modal(frame, area, picker, "Blocked by");
    }
    if let Some(state) = app.status_picker.as_ref() {
        let title = match state.mode {
            super::app::StatusPickerMode::Filter => "Status Filter",
            super::app::StatusPickerMode::Change => "Status",
        };
        render_status_modal(frame, area, &state.picker, title);
    }
    if let Some(state) = app.delete_confirm.as_ref() {
        render_delete_confirm_modal(frame, area, state);
    }
}

fn render_tabs(frame: &mut Frame, app: &AppState, area: Rect) {
    let tabs = vec![
        (
            "1 Tasks",
            app.list_mode == ListMode::Tasks,
            app.tasks.len(),
            COLOR_INFO,
        ),
        (
            "2 Epics",
            app.list_mode == ListMode::Epics,
            app.epic_ids.len(),
            COLOR_ACCENT,
        ),
        (
            "3 Projects",
            app.list_mode == ListMode::Projects,
            app.project_ids.len(),
            COLOR_SUCCESS,
        ),
    ];

    let mut spans = Vec::new();
    for (idx, (label, selected, count, color)) in tabs.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled("  ", Style::default().fg(COLOR_MUTED_DARK)));
        }
        let text = format!("{label} ({count})");
        let style = if selected {
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(COLOR_MUTED)
        };
        spans.push(Span::styled(text, style));
    }

    let widget = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(COLOR_BG_MUTED)),
    );
    frame.render_widget(widget, area);
}

fn render_list(frame: &mut Frame, app: &mut AppState, area: Rect) {
    let mut lines = Vec::new();
    let content_width = area.width.saturating_sub(2) as usize;
    let help_lines = if app.help_context == HelpContext::List {
        build_list_help_lines(content_width)
    } else {
        Vec::new()
    };
    let help_reserved = if help_lines.is_empty() {
        0
    } else {
        help_lines.len() + 1
    };

    if app.filter_active
        || !app.filter.is_empty()
        || app.status_filter.is_some()
        || app.epic_filter.is_some()
        || app.project_filter.is_some()
    {
        let filter_label = if app.filter_active && app.filter.is_empty() {
            "filter: _".to_string()
        } else if app.filter.is_empty() {
            "filter:".to_string()
        } else {
            format!("filter: {}", app.filter)
        };
        let status_label = match app.status_filter.as_deref() {
            Some(value) => format!("status: {value}"),
            None => "status: all".to_string(),
        };
        let epic_label = match app.epic_filter.as_deref() {
            Some(value) => format!("epic: {value}"),
            None => "epic: all".to_string(),
        };
        let project_label = match app.project_filter.as_deref() {
            Some(value) => format!("project: {value}"),
            None => "project: all".to_string(),
        };
        lines.push(Line::from(vec![
            Span::styled(filter_label, Style::default().fg(COLOR_INFO)),
            Span::raw("  "),
            Span::styled(status_label, Style::default().fg(COLOR_WARNING)),
            Span::raw("  "),
            Span::styled(epic_label, Style::default().fg(COLOR_ACCENT)),
            Span::raw("  "),
            Span::styled(project_label, Style::default().fg(COLOR_SUCCESS)),
        ]));
        lines.push(Line::from(""));
    }

    if app.filtered.is_empty() {
        if !app.filter.is_empty()
            || app.status_filter.is_some()
            || app.epic_filter.is_some()
            || app.project_filter.is_some()
        {
            lines.push(Line::from("No matches"));
        } else if app.is_epics_mode() {
            lines.push(Line::from("No epics"));
        } else if app.is_projects_mode() {
            lines.push(Line::from("No projects"));
        } else {
            lines.push(Line::from("No tasks"));
        }
    } else {
        let list_height = area
            .height
            .saturating_sub(2)
            .saturating_sub(lines.len() as u16)
            .saturating_sub(help_reserved as u16) as usize;
        let selected_pos = app
            .selected
            .and_then(|idx| app.filtered.iter().position(|candidate| *candidate == idx));
        let (start, end) = list_window(app.filtered.len(), selected_pos, list_height);
        let mut previous_project = if app.list_mode == ListMode::Tasks && start > 0 {
            app.task_project_id(app.filtered[start - 1])
                .map(|value| value.to_string())
        } else {
            None
        };
        for pos in start..end {
            let idx = app.filtered[pos];
            if let Some(task) = app.tasks.get(idx) {
                if app.list_mode == ListMode::Tasks {
                    let current_project = app.task_project_id(idx).map(|value| value.to_string());
                    if current_project != previous_project {
                        let title = app.project_title_for_id(current_project.as_deref());
                        lines.push(render_project_group_row(&title, content_width));
                    }
                    previous_project = current_project;
                }
                let selected = app.selected == Some(idx);
                let ready = app.is_task_ready(task);
                let depth = app.task_depths.get(idx).copied().unwrap_or(0);
                let is_epic = app.is_epic_task(&task.id);
                lines.push(render_list_row(
                    task,
                    selected,
                    ready,
                    depth,
                    content_width,
                    is_epic,
                ));
            }
        }
    }

    if !help_lines.is_empty() {
        lines.push(Line::from(""));
        lines.extend(help_lines);
    }

    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.list_title())
                .border_style(Style::default().fg(COLOR_BORDER_LIST)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

fn render_detail(frame: &mut Frame, app: &mut AppState, area: Rect) {
    let content_width = area.width.saturating_sub(2) as usize;
    let (title, content) = if let Some(editor) = app.editor.as_ref() {
        let title = match editor.kind() {
            super::editor::EditorKind::NewTask => "New Task",
            super::editor::EditorKind::EditTask => "Edit Task",
        };
        (
            title,
            build_editor_lines(
                editor,
                content_width,
                app.help_context == HelpContext::Editor,
            ),
        )
    } else {
        ("Details", build_detail_lines(app, content_width))
    };
    let widget = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(COLOR_BORDER_DETAIL)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_footer(frame: &mut Frame, app: &AppState, area: Rect) {
    let hint = app.footer_hint();
    let hint_span = Span::styled(hint, Style::default().fg(COLOR_INFO));
    let line = if let Some((status, kind)) = app.status_line() {
        let status_style = match kind {
            StatusKind::Error => Style::default()
                .fg(COLOR_ERROR)
                .add_modifier(Modifier::BOLD),
            StatusKind::Info => Style::default().fg(COLOR_WARNING),
        };
        Line::from(vec![
            hint_span,
            Span::raw("  |  "),
            Span::styled(status, status_style),
        ])
    } else {
        Line::from(hint_span)
    };
    let counts_line = Line::from(Span::styled(
        app.task_count_summary(),
        Style::default().fg(COLOR_ACCENT),
    ));
    let widget = Paragraph::new(vec![line, counts_line])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(COLOR_BORDER_LIST)),
        );
    frame.render_widget(widget, area);
}

fn render_priority_modal(frame: &mut Frame, area: Rect, picker: &PriorityPicker) {
    let content_width = 22u16.min(area.width.saturating_sub(6));
    let height = (picker.options().len() as u16 + 4).min(area.height.saturating_sub(4));
    let modal = centered_rect(content_width, height, area);
    frame.render_widget(Clear, modal);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, option) in picker.options().iter().enumerate() {
        let mut span = Span::styled(
            option.clone(),
            Style::default()
                .fg(priority_color(option))
                .add_modifier(Modifier::BOLD),
        );
        if idx == picker.selected_index() {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
        lines.push(Line::from(span));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter apply  esc cancel",
        Style::default().fg(COLOR_MUTED_DARK),
    )));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Priority"))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn render_status_modal(frame: &mut Frame, area: Rect, picker: &StatusPicker, title: &str) {
    let content_width = 26u16.min(area.width.saturating_sub(6));
    let height = (picker.options().len() as u16 + 4).min(area.height.saturating_sub(4));
    let modal = centered_rect(content_width, height, area);
    frame.render_widget(Clear, modal);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, option) in picker.options().iter().enumerate() {
        let base_style = if option.eq_ignore_ascii_case("all") {
            Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)
        } else {
            status_style(option).add_modifier(Modifier::BOLD)
        };
        let mut span = Span::styled(option.clone(), base_style);
        if idx == picker.selected_index() {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
        lines.push(Line::from(span));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter apply  esc cancel",
        Style::default().fg(COLOR_MUTED_DARK),
    )));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn render_task_picker_modal(frame: &mut Frame, area: Rect, picker: &TaskPicker, title: &str) {
    let content_width = area.width.saturating_sub(6).min(72);
    let max_height = area.height.saturating_sub(6).max(8);
    let list_height = max_height.saturating_sub(6) as usize;
    let modal = centered_rect(content_width, max_height, area);
    frame.render_widget(Clear, modal);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let query = if picker.query().is_empty() {
        "_".to_string()
    } else {
        picker.query().to_string()
    };
    lines.push(Line::from(vec![
        Span::styled("search: ", Style::default().fg(COLOR_MUTED_DARK)),
        Span::styled(query, Style::default().fg(COLOR_INFO)),
    ]));
    lines.push(Line::from(""));

    let filtered = picker.filtered_indices();
    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matches",
            Style::default().fg(COLOR_MUTED_DARK),
        )));
    } else {
        let selected = Some(picker.selected_index());
        let (start, end) = list_window(filtered.len(), selected, list_height.max(1));
        let id_width = ID_WIDTH.min((content_width as usize).saturating_sub(6));
        let title_width = (content_width as usize)
            .saturating_sub(id_width)
            .saturating_sub(3);
        for (pos, idx) in filtered.iter().enumerate().take(end).skip(start) {
            let idx = *idx;
            if let Some(option) = picker.options().get(idx) {
                let id_text = pad_text(&option.id, id_width);
                let title_text = truncate_text(&option.title, title_width);
                let mut spans = vec![
                    Span::styled(id_text, id_style()),
                    Span::raw(" "),
                    Span::styled(title_text, Style::default().fg(COLOR_TEXT)),
                ];
                if selected == Some(pos) {
                    for span in &mut spans {
                        span.style = span.style.add_modifier(Modifier::REVERSED);
                    }
                }
                lines.push(Line::from(spans));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "type to filter  enter apply  esc cancel",
        Style::default().fg(COLOR_MUTED_DARK),
    )));
    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn render_multi_picker_modal(frame: &mut Frame, area: Rect, picker: &MultiTaskPicker, title: &str) {
    let content_width = area.width.saturating_sub(6).min(72);
    let max_height = area.height.saturating_sub(6).max(8);
    let list_height = max_height.saturating_sub(6) as usize;
    let modal = centered_rect(content_width, max_height, area);
    frame.render_widget(Clear, modal);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let query = if picker.query().is_empty() {
        "_".to_string()
    } else {
        picker.query().to_string()
    };
    lines.push(Line::from(vec![
        Span::styled("search: ", Style::default().fg(COLOR_MUTED_DARK)),
        Span::styled(query, Style::default().fg(COLOR_INFO)),
    ]));
    lines.push(Line::from(""));

    let filtered = picker.filtered_indices();
    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matches",
            Style::default().fg(COLOR_MUTED_DARK),
        )));
    } else {
        let selected = Some(picker.selected_index());
        let (start, end) = list_window(filtered.len(), selected, list_height.max(1));
        let marker_width = 3usize;
        let id_width = ID_WIDTH.min((content_width as usize).saturating_sub(marker_width + 6));
        let title_width = (content_width as usize).saturating_sub(marker_width + id_width + 4);
        for (pos, idx) in filtered.iter().enumerate().take(end).skip(start) {
            let idx = *idx;
            if let Some(option) = picker.options().get(idx) {
                let marker = if picker.is_selected(idx) {
                    "[x]"
                } else {
                    "[ ]"
                };
                let marker_style = if picker.is_selected(idx) {
                    Style::default().fg(COLOR_SUCCESS)
                } else {
                    Style::default().fg(COLOR_MUTED_DARK)
                };
                let mut spans = vec![
                    Span::styled(marker, marker_style),
                    Span::raw(" "),
                    Span::styled(pad_text(&option.id, id_width), id_style()),
                    Span::raw(" "),
                    Span::styled(
                        truncate_text(&option.title, title_width),
                        Style::default().fg(COLOR_TEXT),
                    ),
                ];
                if selected == Some(pos) {
                    for span in &mut spans {
                        span.style = span.style.add_modifier(Modifier::REVERSED);
                    }
                }
                lines.push(Line::from(spans));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "type to filter  space toggle  enter apply  esc cancel",
        Style::default().fg(COLOR_MUTED_DARK),
    )));
    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn render_delete_confirm_modal(frame: &mut Frame, area: Rect, state: &DeleteConfirmState) {
    let content_width = area.width.saturating_sub(8).min(64);
    let height = 9u16.min(area.height.saturating_sub(6).max(8));
    let modal = centered_rect(content_width, height, area);
    frame.render_widget(Clear, modal);

    let title_width = (content_width as usize).saturating_sub(8);
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Delete task?",
        Style::default()
            .fg(COLOR_ERROR)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("ID: ", Style::default().fg(COLOR_MUTED_DARK)),
        Span::styled(state.task_id.clone(), id_style()),
    ]));
    if !state.title.trim().is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Title: ", Style::default().fg(COLOR_MUTED_DARK)),
            Span::styled(
                truncate_text(&state.title, title_width),
                Style::default().fg(COLOR_TEXT),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "This will remove all relations.",
        Style::default().fg(COLOR_WARNING),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter/c confirm  esc/q cancel",
        Style::default().fg(COLOR_MUTED_DARK),
    )));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Delete Task"))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn build_editor_lines(editor: &EditorState, width: usize, show_help: bool) -> Vec<Line<'static>> {
    if editor.confirming() {
        return build_confirm_lines(editor, width, show_help);
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, field) in editor.fields().iter().enumerate() {
        let is_body = field.id == EditorFieldId::Body;
        let is_active = idx == editor.active_index();
        let in_insert = is_active && editor.mode() == EditorMode::Insert;
        let label = format!("{:<12}", field.label);
        let mut value = field.value.clone();
        let placeholder = if !in_insert && value.trim().is_empty() {
            if field.required {
                Some("<required>".to_string())
            } else if field.id == EditorFieldId::Priority {
                editor
                    .default_priority()
                    .map(|priority| format!("(default {priority})"))
            } else {
                Some("(optional)".to_string())
            }
        } else {
            None
        };
        let value_style = if placeholder.is_some() {
            Style::default().fg(COLOR_MUTED)
        } else {
            Style::default().fg(COLOR_TEXT)
        };
        if let Some(place) = placeholder {
            value = place;
        }
        if is_body {
            let mut body_lines: Vec<String> = if value.is_empty() {
                vec![String::new()]
            } else {
                value.split('\n').map(|line| line.to_string()).collect()
            };
            if body_lines.is_empty() {
                body_lines.push(String::new());
            }
            for (line_idx, line) in body_lines.into_iter().enumerate() {
                let line_value = truncate_text(&line, width.saturating_sub(14));
                let label_text = if line_idx == 0 {
                    label.clone()
                } else {
                    " ".repeat(12)
                };
                let mut spans = vec![
                    Span::styled(label_text, Style::default().fg(COLOR_TEXT)),
                    Span::raw(" "),
                    Span::styled(line_value, value_style),
                ];
                if is_active {
                    for span in &mut spans {
                        span.style = span.style.add_modifier(Modifier::REVERSED);
                    }
                }
                lines.push(Line::from(spans));
            }
        } else {
            let value_width = width.saturating_sub(14);
            let mut spans = vec![
                Span::styled(label, Style::default().fg(COLOR_TEXT)),
                Span::raw(" "),
            ];
            if in_insert {
                spans.extend(value_with_caret_spans(
                    &value,
                    editor.cursor(),
                    value_width,
                    Style::default().fg(COLOR_TEXT),
                ));
            } else {
                let value = truncate_text(&value, value_width);
                spans.push(Span::styled(value, value_style));
                if is_active {
                    for span in &mut spans {
                        span.style = span.style.add_modifier(Modifier::REVERSED);
                    }
                }
            }
            lines.push(Line::from(spans));
        }
    }

    if let Some(error) = editor.error() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::default()
                .fg(COLOR_ERROR)
                .add_modifier(Modifier::BOLD),
        )));
    }

    if show_help {
        lines.push(Line::from(""));
        lines.extend(build_editor_help_lines(width));
    }
    lines
}

fn build_confirm_lines(editor: &EditorState, width: usize, show_help: bool) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Confirm task details",
        Style::default()
            .fg(COLOR_WARNING)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if let Ok(submit) = editor.build_submit() {
        lines.push(Line::from(vec![
            label_span("Title: "),
            Span::styled(
                truncate_text(&submit.title, width.saturating_sub(8)),
                id_style(),
            ),
        ]));
        let priority = submit
            .priority
            .or_else(|| editor.default_priority().map(|value| value.to_string()))
            .unwrap_or_else(|| "P2".to_string());
        lines.push(Line::from(vec![
            label_span("Priority: "),
            Span::styled(
                priority.clone(),
                Style::default().fg(priority_color(&priority)),
            ),
        ]));
        if let Some(parent) = submit.parent.as_ref() {
            lines.push(Line::from(vec![
                label_span("Parent: "),
                Span::styled(truncate_text(parent, width.saturating_sub(9)), id_style()),
            ]));
        }
        if !submit.children.is_empty() {
            lines.push(Line::from(vec![
                label_span("Children: "),
                Span::styled(
                    truncate_text(&submit.children.join(", "), width.saturating_sub(11)),
                    id_style(),
                ),
            ]));
        }
        if !submit.blocks.is_empty() {
            lines.push(Line::from(vec![
                label_span("Blocking: "),
                Span::styled(
                    truncate_text(&submit.blocks.join(", "), width.saturating_sub(11)),
                    id_style(),
                ),
            ]));
        }
        if !submit.blocked_by.is_empty() {
            lines.push(Line::from(vec![
                label_span("Blocked by: "),
                Span::styled(
                    truncate_text(&submit.blocked_by.join(", "), width.saturating_sub(13)),
                    id_style(),
                ),
            ]));
        }
        if submit.body.trim().is_empty() {
            lines.push(Line::from(vec![
                label_span("Body: "),
                Span::styled("(none)".to_string(), Style::default().fg(COLOR_MUTED_DARK)),
            ]));
        } else {
            let body_preview = submit.body.replace('\n', " ");
            lines.push(Line::from(vec![
                label_span("Body: "),
                Span::styled(
                    truncate_text(&body_preview, width.saturating_sub(8)),
                    Style::default().fg(COLOR_TEXT),
                ),
            ]));
        }
    }

    if let Some(error) = editor.error() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::default()
                .fg(COLOR_ERROR)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter confirm  esc/q cancel",
        Style::default().fg(COLOR_MUTED_DARK),
    )));

    if show_help {
        lines.push(Line::from(""));
        lines.extend(build_editor_help_lines(width));
    }
    lines
}

fn build_list_help_lines(width: usize) -> Vec<Line<'static>> {
    vec![
        help_header("More commands"),
        help_line("j/k or up/down", "move selection", width),
        help_line("n", "new task", width),
        help_line("e", "edit task", width),
        help_line("d", "delete task", width),
        help_line("p", "change priority", width),
        help_line("s", "change status", width),
        help_line("b", "blocked by", width),
        help_line("/", "filter tasks", width),
        help_line("x", "epic filter", width),
        help_line("y", "project filter", width),
        help_line("1/2/3", "switch tasks/epics/projects view", width),
        help_line("v", "toggle tasks/epics/projects view", width),
        help_line("tab", "status filter while filtering", width),
        help_line("r", "reload tasks", width),
        help_line("ctrl+d/u", "page down/up", width),
        help_line("enter", "toggle details in narrow view", width),
        help_line("q/esc", "quit", width),
        help_line("?", "hide help", width),
    ]
}

fn build_editor_help_lines(width: usize) -> Vec<Line<'static>> {
    vec![
        help_header("More commands"),
        help_line("enter", "edit field or open picker", width),
        help_line("c", "review details / confirm", width),
        help_line("ctrl+enter", "confirm from editor", width),
        help_line("tab/shift+tab", "next or previous field", width),
        help_line("j/k", "move field selection", width),
        help_line("ctrl+u", "clear field in insert mode", width),
        help_line("y/enter", "submit on confirm screen", width),
        help_line("backspace", "return to edit screen", width),
        help_line("esc/q", "cancel editor", width),
        help_line("?", "hide help", width),
    ]
}

fn help_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD),
    ))
}

fn help_line(keys: &str, desc: &str, width: usize) -> Line<'static> {
    let key_text = pad_text(keys, HELP_KEY_WIDTH.min(width));
    let desc_width = width.saturating_sub(HELP_KEY_WIDTH + 1);
    let desc_text = truncate_text(desc, desc_width);
    Line::from(vec![
        Span::styled(
            key_text,
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(desc_text, Style::default().fg(COLOR_MUTED)),
    ])
}

fn value_with_caret_spans(
    value: &str,
    cursor: usize,
    width: usize,
    style: Style,
) -> Vec<Span<'static>> {
    if width == 0 {
        return vec![Span::raw("")];
    }
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let cursor = cursor.min(len);
    if len == 0 {
        return vec![Span::styled(
            " ".to_string(),
            style.add_modifier(Modifier::REVERSED),
        )];
    }

    let caret_at_end = cursor == len;
    let available = if caret_at_end {
        width.saturating_sub(1)
    } else {
        width
    };
    let mut start = 0usize;
    if len > available {
        if cursor > available {
            start = cursor.saturating_sub(available);
        }
        if start + available > len {
            start = len.saturating_sub(available);
        }
    }
    let end = (start + available).min(len);
    let window = &chars[start..end];

    if caret_at_end {
        let text: String = window.iter().collect();
        let mut spans = Vec::new();
        if !text.is_empty() {
            spans.push(Span::styled(text, style));
        }
        spans.push(Span::styled(
            " ".to_string(),
            style.add_modifier(Modifier::REVERSED),
        ));
        return spans;
    }

    let caret_index = cursor.saturating_sub(start);
    let before: String = window[..caret_index].iter().collect();
    let caret_char = window.get(caret_index).copied().unwrap_or(' ');
    let after: String = window[caret_index.saturating_add(1)..].iter().collect();

    let mut spans = Vec::new();
    if !before.is_empty() {
        spans.push(Span::styled(before, style));
    }
    spans.push(Span::styled(
        caret_char.to_string(),
        style.add_modifier(Modifier::REVERSED),
    ));
    if !after.is_empty() {
        spans.push(Span::styled(after, style));
    }
    spans
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn render_list_row(
    task: &TaskRecord,
    selected: bool,
    ready: bool,
    depth: usize,
    width: usize,
    is_epic: bool,
) -> Line<'static> {
    let status_label = format_status_label(&task.status);
    let status_text = pad_text_center(&status_label, STATUS_WIDTH);
    let id_text = pad_text(&task.id, ID_WIDTH);
    let priority_text = pad_text(&task.priority, PRIORITY_WIDTH);
    let indent_prefix = if depth > 0 {
        format!("{}- ", "  ".repeat(depth))
    } else {
        String::new()
    };
    let indent_width = indent_prefix.len();
    let epic_marker_width = if is_epic { 2 } else { 0 };
    let used = STATUS_WIDTH
        + READY_WIDTH
        + ID_WIDTH
        + PRIORITY_WIDTH
        + 5
        + indent_width
        + epic_marker_width;
    let title_width = width.saturating_sub(used);
    let title = truncate_text(&task.title, title_width);

    let prefix = " ";
    let ready_marker = if ready { "." } else { " " };
    let status_span = Span::styled(
        status_text,
        status_style(&task.status).add_modifier(Modifier::BOLD),
    );
    let ready_span = Span::styled(ready_marker, Style::default().fg(COLOR_SUCCESS));
    let id_span = Span::styled(id_text, id_style());
    let priority_span = Span::styled(
        priority_text,
        Style::default()
            .fg(priority_color(&task.priority))
            .add_modifier(Modifier::BOLD),
    );
    let mut spans = vec![
        Span::raw(prefix),
        Span::raw(" "),
        status_span,
        Span::raw(" "),
        ready_span,
        Span::raw(" "),
        id_span,
        Span::raw(" "),
        priority_span,
        Span::raw(" "),
        Span::styled(indent_prefix, Style::default().fg(COLOR_MUTED_DARK)),
    ];
    if is_epic {
        spans.push(Span::styled(
            "E ",
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::raw(title));

    if selected {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }

    Line::from(spans)
}

fn render_project_group_row(title: &str, width: usize) -> Line<'static> {
    let header = format!("-- {title}");
    let fill_len = width.saturating_sub(header.len());
    let mut text = header;
    text.push_str(&"-".repeat(fill_len));
    Line::from(Span::styled(
        truncate_text(&text, width),
        Style::default()
            .fg(COLOR_SUCCESS)
            .add_modifier(Modifier::BOLD),
    ))
}

fn build_detail_lines(app: &mut AppState, width: usize) -> Vec<Line<'static>> {
    let Some(task) = app.selected_task() else {
        return vec![Line::from("No task selected")];
    };

    let cache_key = (task.id.clone(), width as u16);
    if let Some(lines) = app.cache.detail.get(&cache_key) {
        return lines.clone();
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("# ", Style::default().fg(COLOR_MUTED_DARK)),
        Span::styled(task.id.clone(), id_style().add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(
            task.title.clone(),
            Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        label_span("Status: "),
        Span::styled(
            display_status_text(&task.status),
            status_style(&task.status).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        label_span("Priority: "),
        Span::styled(
            task.priority.clone(),
            Style::default()
                .fg(priority_color(&task.priority))
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    if let Some(epic) = task.epic.as_deref() {
        lines.push(Line::from(vec![
            label_span("Epic: "),
            Span::styled(epic.to_string(), id_style()),
        ]));
    }
    if let Some(project) = task.project.as_deref() {
        lines.push(Line::from(vec![
            label_span("Project: "),
            Span::styled(project.to_string(), id_style()),
        ]));
    }
    lines.push(Line::from(vec![
        label_span("Updated: "),
        Span::styled(
            format_timestamp(task.updated_at),
            Style::default().fg(COLOR_WARNING),
        ),
        Span::raw("  "),
        label_span("Created: "),
        Span::styled(
            format_timestamp(task.created_at),
            Style::default().fg(COLOR_WARNING),
        ),
    ]));
    if let Some(workspace) = task.workspace.as_deref() {
        lines.push(Line::from(vec![
            label_span("Workspace: "),
            Span::styled(workspace.to_string(), Style::default().fg(COLOR_INFO)),
        ]));
    }
    if let Some(branch) = task.branch.as_deref() {
        lines.push(Line::from(vec![
            label_span("Branch: "),
            Span::styled(branch.to_string(), Style::default().fg(COLOR_INFO)),
        ]));
    }
    lines.push(Line::from(""));

    lines.push(section_header("## Body"));
    let body = task
        .body
        .as_deref()
        .map(|value| value.trim_end())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("No body.");
    for line in body.lines() {
        lines.push(Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(COLOR_TEXT),
        )));
    }

    if let Some(details) = app.selected_details() {
        append_relations(&mut lines, &details.relations);
        append_comments(&mut lines, details);
    } else if task.comments_count > 0 {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("## Comments: {}", task.comments_count),
                Style::default()
                    .fg(COLOR_MAGENTA)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" (loading...)", Style::default().fg(COLOR_MUTED_DARK)),
        ]));
    }

    app.cache.detail.insert(cache_key, lines.clone());
    lines
}

fn append_relations(lines: &mut Vec<Line<'static>>, relations: &crate::task::TaskRelations) {
    lines.push(Line::from(""));
    lines.push(section_header("## Relations"));
    let mut any = false;

    if let Some(parent) = relations.parent.as_deref() {
        any = true;
        lines.push(Line::from(vec![
            label_span("Parent: "),
            Span::styled(parent.to_string(), id_style()),
        ]));
    }
    if !relations.epic_tasks.is_empty() {
        any = true;
        lines.push(Line::from(vec![
            label_span("Epic tasks: "),
            Span::styled(relations.epic_tasks.join(", "), id_style()),
        ]));
    }
    if !relations.project_tasks.is_empty() {
        any = true;
        lines.push(Line::from(vec![
            label_span("Project members: "),
            Span::styled(relations.project_tasks.join(", "), id_style()),
        ]));
    }
    if !relations.children.is_empty() {
        any = true;
        lines.push(Line::from(vec![
            label_span("Children: "),
            Span::styled(relations.children.join(", "), id_style()),
        ]));
    }
    if !relations.blocks.is_empty() {
        any = true;
        lines.push(Line::from(vec![
            label_span("Blocks: "),
            Span::styled(relations.blocks.join(", "), id_style()),
        ]));
    }
    if !relations.blocked_by.is_empty() {
        any = true;
        lines.push(Line::from(vec![
            label_span("Blocked by: "),
            Span::styled(relations.blocked_by.join(", "), id_style()),
        ]));
    }
    if !relations.relates.is_empty() {
        any = true;
        for relation in &relations.relates {
            lines.push(Line::from(vec![
                label_span("Relates: "),
                Span::styled(relation.id.clone(), id_style()),
                Span::raw(" - "),
                Span::styled(
                    relation.description.clone(),
                    Style::default().fg(COLOR_TEXT),
                ),
            ]));
        }
    }
    if !any {
        lines.push(Line::from(Span::styled(
            "None",
            Style::default().fg(COLOR_MUTED_DARK),
        )));
    }
}

fn append_comments(lines: &mut Vec<Line<'static>>, details: &TaskDetails) {
    if details.comments.is_empty() {
        return;
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("## Comments: {}", details.comments.len()),
        Style::default()
            .fg(COLOR_MAGENTA)
            .add_modifier(Modifier::BOLD),
    )));
    for comment in &details.comments {
        let actor = comment.actor.as_deref().unwrap_or("unknown");
        let timestamp = format_timestamp(comment.timestamp);
        lines.push(Line::from(vec![
            Span::styled("- ", Style::default().fg(COLOR_MUTED_DARK)),
            Span::styled(timestamp, Style::default().fg(COLOR_WARNING)),
            Span::raw(" "),
            Span::styled(actor.to_string(), id_style()),
            Span::styled(": ", Style::default().fg(COLOR_MUTED_DARK)),
            Span::styled(comment.comment.clone(), Style::default().fg(COLOR_TEXT)),
        ]));
    }
}

fn list_window(total: usize, selected: Option<usize>, height: usize) -> (usize, usize) {
    if total == 0 || height == 0 {
        return (0, 0);
    }
    if total <= height {
        return (0, total);
    }
    let selected = selected.unwrap_or(0);
    let mut start = selected.saturating_sub(height / 2);
    if start + height > total {
        start = total - height;
    }
    (start, start + height)
}

fn format_status_label(status: &str) -> String {
    match normalize_status(status).as_str() {
        "open" => "OPEN".to_string(),
        "in_progress" => "PROG".to_string(),
        "closed" => "DONE".to_string(),
        value => truncate_text(value, 6),
    }
}

fn status_style(status: &str) -> Style {
    let normalized = normalize_status(status);
    let (fg, bg) = status_colors(&normalized);
    Style::default().fg(fg).bg(bg)
}

fn display_status_text(status: &str) -> String {
    match normalize_status(status).as_str() {
        "closed" => "done".to_string(),
        _ => status.trim().to_string(),
    }
}

fn status_colors(status: &str) -> (Color, Color) {
    match normalize_status(status).as_str() {
        "open" => (Color::Rgb(80, 250, 123), Color::Rgb(26, 61, 42)),
        "in_progress" => (Color::Rgb(139, 233, 253), Color::Rgb(26, 51, 68)),
        "closed" => (Color::Rgb(98, 114, 164), Color::Rgb(42, 42, 61)),
        _ => (COLOR_TEXT, COLOR_BG_MUTED),
    }
}

fn priority_color(priority: &str) -> Color {
    match priority.trim().to_ascii_uppercase().as_str() {
        "P0" => Color::Rgb(255, 87, 87),
        "P1" => Color::Rgb(255, 147, 112),
        "P2" => COLOR_WARNING,
        "P3" => COLOR_ACCENT,
        "P4" => COLOR_MUTED_DARK,
        _ => COLOR_INFO,
    }
}

fn normalize_status(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn pad_text(value: &str, width: usize) -> String {
    let mut text = value.to_string();
    if text.len() > width {
        text = truncate_text(&text, width);
    }
    format!("{text:width$}")
}

fn pad_text_center(value: &str, width: usize) -> String {
    let mut text = value.to_string();
    if text.len() > width {
        text = truncate_text(&text, width);
    }
    let len = text.chars().count();
    if len >= width {
        return text;
    }
    let total_pad = width - len;
    let left = total_pad / 2;
    let right = total_pad - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn truncate_text(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max {
        return value.to_string();
    }
    if max <= 3 {
        return chars[..max].iter().collect();
    }
    let mut out: String = chars[..(max - 3)].iter().collect();
    out.push_str("...");
    out
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M").to_string()
}

fn label_span(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(COLOR_MUTED_DARK))
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(COLOR_MAGENTA)
            .add_modifier(Modifier::BOLD),
    ))
}

fn id_style() -> Style {
    Style::default()
        .fg(COLOR_MUTED)
        .add_modifier(Modifier::BOLD)
}
