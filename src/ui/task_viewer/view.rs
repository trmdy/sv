use chrono::{DateTime, Utc};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::task::{TaskDetails, TaskRecord};

use super::app::{AppState, StatusKind};
use super::editor::{
    EditorFieldId, EditorMode, EditorState, MultiTaskPicker, PriorityPicker, StatusPicker,
    TaskPicker,
};

const STATUS_WIDTH: usize = 7;
const READY_WIDTH: usize = 1;
const ID_WIDTH: usize = 12;
const PRIORITY_WIDTH: usize = 3;

pub fn render(frame: &mut Frame, app: &mut AppState) {
    let area = frame.size();
    let footer_height = 3u16;
    let main_height = area.height.saturating_sub(footer_height);
    let main = Rect::new(area.x, area.y, area.width, main_height);
    let footer = Rect::new(area.x, area.y + main_height, area.width, footer_height);

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
        render_task_picker_modal(frame, area, picker);
    }
    if let Some(picker) = app.children_picker.as_ref() {
        render_children_picker_modal(frame, area, picker);
    }
    if let Some(state) = app.status_picker.as_ref() {
        let title = match state.mode {
            super::app::StatusPickerMode::Filter => "Status Filter",
            super::app::StatusPickerMode::Change => "Status",
        };
        render_status_modal(frame, area, &state.picker, title);
    }
}

fn render_list(frame: &mut Frame, app: &mut AppState, area: Rect) {
    let mut lines = Vec::new();
    let content_width = area.width.saturating_sub(2) as usize;

    if app.filter_active || !app.filter.is_empty() || app.status_filter.is_some() {
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
        lines.push(Line::from(vec![
            Span::styled(filter_label, Style::default().fg(Color::LightCyan)),
            Span::raw("  "),
            Span::styled(status_label, Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::from(""));
    }

    if app.filtered.is_empty() {
        if !app.filter.is_empty() || app.status_filter.is_some() {
            lines.push(Line::from("No matches"));
        } else {
            lines.push(Line::from("No tasks"));
        }
    } else {
        let list_height = area
            .height
            .saturating_sub(2)
            .saturating_sub(lines.len() as u16) as usize;
        let selected_pos = app
            .selected
            .and_then(|idx| app.filtered.iter().position(|candidate| *candidate == idx));
        let (start, end) = list_window(app.filtered.len(), selected_pos, list_height);
        for pos in start..end {
            let idx = app.filtered[pos];
            if let Some(task) = app.tasks.get(idx) {
                let selected = app.selected == Some(idx);
                let ready = app.is_task_ready(task);
                let depth = app.task_depths.get(idx).copied().unwrap_or(0);
                lines.push(render_list_row(task, selected, ready, depth, content_width));
            }
        }
    }

    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Tasks")
                .border_style(Style::default().fg(Color::LightBlue)),
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
        (title, build_editor_lines(editor, content_width))
    } else {
        ("Details", build_detail_lines(app, content_width))
    };
    let widget = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::LightYellow)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_footer(frame: &mut Frame, app: &AppState, area: Rect) {
    let hint = app.footer_hint();
    let hint_span = Span::styled(hint, Style::default().fg(Color::LightCyan));
    let line = if let Some((status, kind)) = app.status_line() {
        let status_style = match kind {
            StatusKind::Error => Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
            StatusKind::Info => Style::default().fg(Color::Yellow),
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
        Style::default().fg(Color::LightBlue),
    ));
    let widget = Paragraph::new(vec![line, counts_line])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::LightBlue)),
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
        Style::default().fg(Color::DarkGray),
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
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD)
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
        Style::default().fg(Color::DarkGray),
    )));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn render_task_picker_modal(frame: &mut Frame, area: Rect, picker: &TaskPicker) {
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
        Span::styled("search: ", Style::default().fg(Color::DarkGray)),
        Span::styled(query, Style::default().fg(Color::LightCyan)),
    ]));
    lines.push(Line::from(""));

    let filtered = picker.filtered_indices();
    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matches",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let selected = Some(picker.selected_index());
        let (start, end) = list_window(filtered.len(), selected, list_height.max(1));
        let id_width = ID_WIDTH.min((content_width as usize).saturating_sub(6));
        let title_width = (content_width as usize)
            .saturating_sub(id_width)
            .saturating_sub(3);
        for pos in start..end {
            let idx = filtered[pos];
            if let Some(option) = picker.options().get(idx) {
                let id_text = pad_text(&option.id, id_width);
                let title_text = truncate_text(&option.title, title_width);
                let mut spans = vec![
                    Span::styled(id_text, id_style()),
                    Span::raw(" "),
                    Span::styled(title_text, Style::default().fg(Color::White)),
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
        Style::default().fg(Color::DarkGray),
    )));
    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Parent"))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn render_children_picker_modal(frame: &mut Frame, area: Rect, picker: &MultiTaskPicker) {
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
        Span::styled("search: ", Style::default().fg(Color::DarkGray)),
        Span::styled(query, Style::default().fg(Color::LightCyan)),
    ]));
    lines.push(Line::from(""));

    let filtered = picker.filtered_indices();
    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matches",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let selected = Some(picker.selected_index());
        let (start, end) = list_window(filtered.len(), selected, list_height.max(1));
        let marker_width = 3usize;
        let id_width = ID_WIDTH.min((content_width as usize).saturating_sub(marker_width + 6));
        let title_width = (content_width as usize).saturating_sub(marker_width + id_width + 4);
        for pos in start..end {
            let idx = filtered[pos];
            if let Some(option) = picker.options().get(idx) {
                let marker = if picker.is_selected(idx) {
                    "[x]"
                } else {
                    "[ ]"
                };
                let marker_style = if picker.is_selected(idx) {
                    Style::default().fg(Color::LightGreen)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let mut spans = vec![
                    Span::styled(marker, marker_style),
                    Span::raw(" "),
                    Span::styled(pad_text(&option.id, id_width), id_style()),
                    Span::raw(" "),
                    Span::styled(
                        truncate_text(&option.title, title_width),
                        Style::default().fg(Color::White),
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
        Style::default().fg(Color::DarkGray),
    )));
    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Children"))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, modal);
}

fn build_editor_lines(editor: &EditorState, width: usize) -> Vec<Line<'static>> {
    if editor.confirming() {
        return build_confirm_lines(editor, width);
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, field) in editor.fields().iter().enumerate() {
        let is_body = field.id == EditorFieldId::Body;
        let label = format!("{:<12}", field.label);
        let mut value = field.value.clone();
        let placeholder = if value.trim().is_empty() {
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
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
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
                    Span::styled(label_text, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(line_value, value_style),
                ];
                if idx == editor.active_index() {
                    for span in &mut spans {
                        span.style = span.style.add_modifier(Modifier::REVERSED);
                    }
                }
                lines.push(Line::from(spans));
            }
        } else {
            let value = truncate_text(&value, width.saturating_sub(14));
            let mut spans = vec![
                Span::styled(label, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(value, value_style),
            ];
            if idx == editor.active_index() {
                for span in &mut spans {
                    span.style = span.style.add_modifier(Modifier::REVERSED);
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
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(""));
    let body_active = matches!(editor.active_field_id(), Some(EditorFieldId::Body));
    let hint = match editor.mode() {
        EditorMode::Normal => {
            "enter edit  tab next  shift+tab prev  ctrl+enter confirm  esc cancel"
        }
        EditorMode::Insert => {
            if body_active {
                "enter newline  tab next  ctrl+u clear  ctrl+enter confirm  esc cancel"
            } else {
                "enter/tab next  ctrl+u clear  ctrl+enter confirm  esc cancel"
            }
        }
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

fn build_confirm_lines(editor: &EditorState, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Confirm task details",
        Style::default()
            .fg(Color::LightYellow)
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
        if submit.body.trim().is_empty() {
            lines.push(Line::from(vec![
                label_span("Body: "),
                Span::styled("(none)".to_string(), Style::default().fg(Color::DarkGray)),
            ]));
        } else {
            let body_preview = submit.body.replace('\n', " ");
            lines.push(Line::from(vec![
                label_span("Body: "),
                Span::styled(
                    truncate_text(&body_preview, width.saturating_sub(8)),
                    Style::default().fg(Color::White),
                ),
            ]));
        }
    }

    if let Some(error) = editor.error() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "y confirm  backspace edit  esc cancel",
        Style::default().fg(Color::DarkGray),
    )));
    lines
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
    let used = STATUS_WIDTH + READY_WIDTH + ID_WIDTH + PRIORITY_WIDTH + 5 + indent_width;
    let title_width = width.saturating_sub(used);
    let title = truncate_text(&task.title, title_width);

    let prefix = " ";
    let ready_marker = if ready { "." } else { " " };
    let status_span = Span::styled(
        status_text,
        status_style(&task.status).add_modifier(Modifier::BOLD),
    );
    let ready_span = Span::styled(ready_marker, Style::default().fg(Color::LightGreen));
    let id_span = Span::styled(id_text, Style::default().fg(Color::LightBlue));
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
        Span::styled(indent_prefix, Style::default().fg(Color::DarkGray)),
        Span::raw(title),
    ];

    if selected {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }

    Line::from(spans)
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
        Span::styled("# ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            task.id.clone(),
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            task.title.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
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
    lines.push(Line::from(vec![
        label_span("Updated: "),
        Span::styled(
            format_timestamp(task.updated_at),
            Style::default().fg(Color::LightYellow),
        ),
        Span::raw("  "),
        label_span("Created: "),
        Span::styled(
            format_timestamp(task.created_at),
            Style::default().fg(Color::LightYellow),
        ),
    ]));
    if let Some(workspace) = task.workspace.as_deref() {
        lines.push(Line::from(vec![
            label_span("Workspace: "),
            Span::styled(workspace.to_string(), Style::default().fg(Color::LightCyan)),
        ]));
    }
    if let Some(branch) = task.branch.as_deref() {
        lines.push(Line::from(vec![
            label_span("Branch: "),
            Span::styled(branch.to_string(), Style::default().fg(Color::LightCyan)),
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
            Style::default().fg(Color::White),
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
                    .fg(Color::LightMagenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" (loading...)", Style::default().fg(Color::DarkGray)),
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
                    Style::default().fg(Color::White),
                ),
            ]));
        }
    }
    if !any {
        lines.push(Line::from(Span::styled(
            "None",
            Style::default().fg(Color::DarkGray),
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
            .fg(Color::LightMagenta)
            .add_modifier(Modifier::BOLD),
    )));
    for comment in &details.comments {
        let actor = comment.actor.as_deref().unwrap_or("unknown");
        let timestamp = format_timestamp(comment.timestamp);
        lines.push(Line::from(vec![
            Span::styled("- ", Style::default().fg(Color::DarkGray)),
            Span::styled(timestamp, Style::default().fg(Color::LightYellow)),
            Span::raw(" "),
            Span::styled(actor.to_string(), id_style()),
            Span::styled(": ", Style::default().fg(Color::DarkGray)),
            Span::styled(comment.comment.clone(), Style::default().fg(Color::White)),
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
        _ => (Color::White, Color::DarkGray),
    }
}

fn priority_color(priority: &str) -> Color {
    match priority.trim().to_ascii_uppercase().as_str() {
        "P0" => Color::Red,
        "P1" => Color::LightRed,
        "P2" => Color::Yellow,
        "P3" => Color::LightBlue,
        "P4" => Color::DarkGray,
        _ => Color::LightCyan,
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
    Span::styled(label.to_string(), Style::default().fg(Color::DarkGray))
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(Color::LightMagenta)
            .add_modifier(Modifier::BOLD),
    ))
}

fn id_style() -> Style {
    Style::default()
        .fg(Color::LightBlue)
        .add_modifier(Modifier::BOLD)
}
