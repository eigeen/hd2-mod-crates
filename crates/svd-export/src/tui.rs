use iocraft::prelude::*;
use std::path::PathBuf;
use svd_core::error::Result;
use svd_core::export::{ExportMode, ExportPackageSummary, load_export_package_summary};

#[derive(Debug, Default)]
struct TuiOutcome {
    confirmed: bool,
    selected_variants: Vec<String>,
}

#[derive(Debug, Clone)]
struct TuiContext {
    package: PathBuf,
    display_name: Option<String>,
    summary: ExportPackageSummary,
}

#[derive(Debug, Clone)]
struct VariantRow {
    index: usize,
    name: String,
    selected: bool,
    shortcut: Option<char>,
}

const GROUP_TARGETS: usize = 0;
const GROUP_ACTIONS: usize = 1;
const GROUP_COUNT: usize = 2;

const ACTION_TOGGLE_ALL: usize = 0;
const ACTION_OK: usize = 1;
const ACTION_CANCEL: usize = 2;
const ACTION_COUNT: usize = 3;

#[derive(Debug, Clone, Copy)]
enum ActionStyle {
    Neutral,
    Primary,
    Danger,
}

/// Runs the fullscreen export target picker and returns the selected export mode.
pub fn run_export_tui(
    package: PathBuf,
    _output: PathBuf,
    display_name: Option<String>,
) -> Result<Option<ExportMode>> {
    let summary = load_export_package_summary(&package)?;
    let context = TuiContext {
        package,
        display_name,
        summary,
    };
    let mut outcome = TuiOutcome::default();
    smol::block_on(
        element!(ExporterApp(context: context.clone(), outcome: &mut outcome)).fullscreen(),
    )?;
    Ok(outcome.confirmed.then(|| export_mode(&context, outcome)))
}

/// Shows a blocking fullscreen error message for interactive startup failures.
pub fn run_error_tui(title: impl Into<String>, message: impl Into<String>) -> Result<()> {
    smol::block_on(element!(ErrorApp(title: title.into(), message: message.into())).fullscreen())?;
    Ok(())
}

#[derive(Default, Props)]
struct ErrorAppProps {
    title: String,
    message: String,
}

#[component]
fn ErrorApp(props: &ErrorAppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let close_requested = hooks.use_state(|| false);
    let mut button_close_requested = close_requested;

    hooks.use_terminal_events({
        let mut close_requested = close_requested;
        move |event| {
            if is_error_close_event(event) {
                close_requested.set(true);
            }
        }
    });

    if close_requested.get() {
        system.exit();
    }

    let (width, height) = hooks.use_terminal_size();
    element! {
        View(width, height, padding: 1) {
            View(
                width: 100pct,
                height: 100pct,
                border_style: BorderStyle::Round,
                border_color: Color::Red,
                flex_direction: FlexDirection::Column,
                padding: 1,
                gap: 1,
            ) {
                Text(
                    content: &props.title,
                    color: Color::Red,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
                Text(content: &props.message, color: Color::White)
                View(flex_grow: 1.0)
                Button(handler: move |_| button_close_requested.set(true)) {
                    Text(
                        content: "关闭 [Enter / Esc]",
                        color: Color::Grey,
                        weight: Weight::Bold,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
        }
    }
}

#[derive(Default, Props)]
struct ExporterAppProps<'a> {
    context: Option<TuiContext>,
    outcome: Option<&'a mut TuiOutcome>,
}

#[component]
fn ExporterApp<'a>(
    props: &mut ExporterAppProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let context = props.context.clone().expect("context is required");
    let target_count = context.summary.variants.len();
    let selected = hooks.use_state(|| vec![true; target_count]);
    let active_group = hooks.use_state(|| GROUP_TARGETS);
    let targets_focus = hooks.use_state(|| 0usize);
    let actions_focus = hooks.use_state(|| ACTION_OK);
    let submitted = hooks.use_state(|| false);
    let cancelled = hooks.use_state(|| false);
    let search_active = hooks.use_state(|| false);
    let search_query = hooks.use_state(String::new);
    let mut system = hooks.use_context_mut::<SystemContext>();

    let current_selection = selected.read().clone();
    let query_string = search_query.read().clone();
    let visible = visible_indices(&context.summary.variants, &query_string);
    let visible_for_events = visible.clone();

    // Keep targets_focus on a visible row whenever the filter changes.
    hooks.use_effect(
        {
            let mut targets_focus = targets_focus;
            let visible_clone = visible.clone();
            move || {
                let current = targets_focus.get();
                if let Some(&first) = visible_clone.first()
                    && !visible_clone.contains(&current)
                {
                    targets_focus.set(first);
                }
            }
        },
        query_string.clone(),
    );

    hooks.use_terminal_events({
        let mut selected = selected;
        let mut active_group = active_group;
        let mut targets_focus = targets_focus;
        let mut actions_focus = actions_focus;
        let mut submitted = submitted;
        let mut cancelled = cancelled;
        let mut search_active = search_active;
        let mut search_query = search_query;
        let visible = visible_for_events;
        move |event| {
            let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }
            if search_active.get() {
                match code {
                    KeyCode::Esc => {
                        search_active.set(false);
                        search_query.set(String::new());
                    }
                    KeyCode::Enter => {
                        search_active.set(false);
                    }
                    _ => {}
                }
                return;
            }
            if matches!(code, KeyCode::Char('f') | KeyCode::Char('F')) {
                search_active.set(true);
                return;
            }
            let mut controls = KeyEventControls {
                active_group: &mut active_group,
                targets_focus: &mut targets_focus,
                actions_focus: &mut actions_focus,
                selected: &mut selected,
                submitted: &mut submitted,
                cancelled: &mut cancelled,
            };
            handle_key_event(code, target_count, &visible, &mut controls);
        }
    });

    if submitted.get() || cancelled.get() {
        write_outcome(props.outcome.as_mut(), &context, &selected, submitted.get());
        system.exit();
    }

    let (width, height) = hooks.use_terminal_size();
    let filter_active = !query_string.is_empty();
    let rows = variant_rows(
        &context.summary.variants,
        &current_selection,
        &visible,
        filter_active,
    );
    let all_selected = is_all_selected(&current_selection);
    let group = active_group.get();
    let toggle_text = if all_selected {
        "取消全选 [A]"
    } else {
        "全选 [A]"
    };
    let popover_visible = search_active.get();
    let match_count = visible.len();
    let popover_query = query_string.clone();

    element! {
        View(
            width,
            height,
            flex_direction: FlexDirection::Column,
        ) {
            View(
                width: 100pct,
                height: 100pct,
                border_style: BorderStyle::Round,
                border_color: Color::Cyan,
                flex_direction: FlexDirection::Column,
                padding_left: 1,
                padding_right: 1,
            ) {
                Header(
                    context: context.clone(),
                    search_active: search_active,
                    search_query: search_query,
                )
                FocusGroup(flex_grow: 1.0) {
                    TargetList(
                        rows: rows,
                        selected: selected,
                        focused: targets_focus,
                        group_active: group == GROUP_TARGETS && !popover_visible,
                    )
                }
                FocusGroup(flex_grow: 0.0) {
                    KeyHints
                    ActionRow(
                        toggle_text: toggle_text.to_string(),
                        toggle_target: !all_selected,
                        actions_focus: actions_focus,
                        group_active: group == GROUP_ACTIONS && !popover_visible,
                        selected: selected,
                        submitted: submitted,
                        cancelled: cancelled,
                    )
                }
            }
            #(popover_visible.then(|| element! {
                SearchPopover(
                    query: popover_query,
                    query_state: search_query,
                    match_count: match_count,
                    total: target_count,
                )
            }))
        }
    }
}

#[derive(Default, Props)]
struct HeaderProps {
    context: Option<TuiContext>,
    search_active: Option<State<bool>>,
    search_query: Option<State<String>>,
}

#[component]
fn Header(props: &HeaderProps) -> impl Into<AnyElement<'static>> {
    let context = props.context.clone().expect("context is required");
    let title = project_title(&context);
    let description = package_description(&context);
    let mut search_active = props
        .search_active
        .expect("search_active state is required");
    let search_query = props.search_query.expect("search_query state is required");
    let query_snapshot = search_query.read().clone();
    let filter_active = !query_snapshot.is_empty();

    element! {
        View(
            flex_direction: FlexDirection::Column,
        ) {
            View(
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: 1,
            ) {
                Text(
                    content: title,
                    color: Color::White,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
                #(description.map(|content| element! {
                    Text(
                        content: format!("· {content}"),
                        color: Color::Grey,
                        wrap: TextWrap::NoWrap,
                    )
                }))
                View(flex_grow: 1.0)
                Button(handler: move |_| search_active.set(true)) {
                    Text(
                        content: if filter_active {
                            format!("🔍 搜索: {} [F]", query_snapshot)
                        } else {
                            "🔍 搜索 [F]".to_string()
                        },
                        color: if filter_active { Color::Yellow } else { Color::Cyan },
                        weight: Weight::Bold,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            Text(
                content: "超级模组分包",
                color: Color::White,
                weight: Weight::Bold,
                wrap: TextWrap::NoWrap,
            )
            Text(
                content: "选择需要导出的装备类型，将在当前目录生成包含所选装备的模组包，导入模组管理器即可使用。",
                color: Color::White,
            )
        }
    }
}

#[derive(Default, Props)]
struct SearchPopoverProps {
    query: String,
    query_state: Option<State<String>>,
    match_count: usize,
    total: usize,
}

#[component]
fn SearchPopover(props: &mut SearchPopoverProps) -> impl Into<AnyElement<'static>> {
    let mut query_state = props.query_state.expect("query_state is required");
    let query = props.query.clone();
    let match_count = props.match_count;
    let total = props.total;

    element! {
        View(
            position: Position::Absolute,
            top: 2,
            left: 4,
            right: 4,
            flex_direction: FlexDirection::Column,
            background_color: Color::Reset,
            border_style: BorderStyle::Round,
            border_color: Color::Yellow,
            padding_left: 1,
            padding_right: 1,
            padding_top: 0,
            padding_bottom: 0,
        ) {
            View(
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: 1,
                width: 100pct,
            ) {
                Text(
                    content: "🔍 搜索:",
                    color: Color::Yellow,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
                View(
                    background_color: Color::DarkGrey,
                    flex_grow: 1.0,
                    height: 1,
                ) {
                    TextInput(
                        has_focus: true,
                        value: query,
                        on_change: move |new_value| query_state.set(new_value),
                    )
                }
                Text(
                    content: format!("{match_count}/{total}"),
                    color: if match_count == 0 { Color::Red } else { Color::Green },
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
            }
            Text(
                content: "Enter 应用 · Esc 取消",
                color: Color::Grey,
                wrap: TextWrap::NoWrap,
            )
        }
    }
}

#[component]
fn KeyHints() -> impl Into<AnyElement<'static>> {
    element! {
        View(
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            gap: 2,
            padding_top: 0,
            padding_bottom: 0,
        ) {
            Text(content: "Tab", color: Color::Cyan, weight: Weight::Bold, wrap: TextWrap::NoWrap)
            Text(content: "切换分组", color: Color::Grey, wrap: TextWrap::NoWrap)
            Text(content: "↑↓←→", color: Color::Cyan, weight: Weight::Bold, wrap: TextWrap::NoWrap)
            Text(content: "组内移动", color: Color::Grey, wrap: TextWrap::NoWrap)
            Text(content: "Space/Enter", color: Color::Cyan, weight: Weight::Bold, wrap: TextWrap::NoWrap)
            Text(content: "勾选/确认", color: Color::Grey, wrap: TextWrap::NoWrap)
            Text(content: "A", color: Color::Cyan, weight: Weight::Bold, wrap: TextWrap::NoWrap)
            Text(content: "全选", color: Color::Grey, wrap: TextWrap::NoWrap)
            Text(content: "F", color: Color::Cyan, weight: Weight::Bold, wrap: TextWrap::NoWrap)
            Text(content: "搜索", color: Color::Grey, wrap: TextWrap::NoWrap)
            View(flex_grow: 1.0)
            Text(
                content: "支持鼠标点击",
                color: Color::Yellow,
                weight: Weight::Bold,
                wrap: TextWrap::NoWrap,
            )
        }
    }
}

#[derive(Default, Props)]
struct FocusGroupProps<'a> {
    children: Vec<AnyElement<'a>>,
    flex_grow: f32,
}

/// An invisible div-like wrapper that carries the "focus group" concept —
/// arrow keys stay inside the group while Tab moves between groups. It
/// intentionally has no border or visual marker; activity is conveyed by
/// the focused child (row/button) styling itself.
#[component]
fn FocusGroup<'a>(props: &mut FocusGroupProps<'a>) -> impl Into<AnyElement<'a>> {
    let flex_grow = props.flex_grow;

    element! {
        View(
            flex_direction: FlexDirection::Column,
            flex_grow: flex_grow,
            min_height: 0,
        ) {
            #(props.children.drain(..))
        }
    }
}

#[derive(Default, Props)]
struct TargetListProps {
    rows: Vec<VariantRow>,
    selected: Option<State<Vec<bool>>>,
    focused: Option<State<usize>>,
    group_active: bool,
}

#[component]
fn TargetList(props: &TargetListProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let selected = props.selected.expect("selected state is required");
    let focused = props.focused.expect("focused state is required");
    let group_active = props.group_active;
    let scroll_handle = hooks.use_ref_default::<ScrollViewHandle>();
    let focused_index = focused.get();

    hooks.use_effect(
        {
            let mut scroll_handle = scroll_handle;
            move || scroll_focused_row(&mut scroll_handle, focused_index)
        },
        focused_index,
    );

    element! {
        View(
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            min_height: 0,
        ) {
            ScrollView(
                handle: scroll_handle,
                keyboard_scroll: Some(false),
                scroll_step: Some(3),
            ) {
                View(flex_direction: FlexDirection::Column, width: 100pct) {
                    #(props.rows.iter().map(|row| element! {
                        TargetRow(
                            row: row.clone(),
                            selected: selected,
                            focused: focused,
                            group_active: group_active,
                        )
                    }))
                }
            }
        }
    }
}

#[derive(Default, Props)]
struct TargetRowProps {
    row: Option<VariantRow>,
    selected: Option<State<Vec<bool>>>,
    focused: Option<State<usize>>,
    group_active: bool,
}

#[component]
fn TargetRow(props: &TargetRowProps) -> impl Into<AnyElement<'static>> {
    let row = props.row.clone().expect("row is required");
    let mut selected = props.selected.expect("selected state is required");
    let mut focused = props.focused.expect("focused state is required");
    let is_focused = props.group_active && focused.get() == row.index;
    let label = target_label(&row);
    let mark = if row.selected { "[x]" } else { "[ ]" };

    element! {
        Button(handler: move |_| focus_and_toggle_variant(&mut selected, &mut focused, row.index)) {
            View(
                width: 100pct,
                padding_left: 1,
                padding_right: 1,
                background_color: row_color(row.index, is_focused),
            ) {
                View(width: 6) {
                    Text(content: mark, color: if row.selected { Color::Green } else { Color::DarkGrey })
                }
                Text(
                    content: label,
                    color: Color::White,
                    weight: if is_focused { Weight::Bold } else { Weight::Normal },
                    wrap: TextWrap::NoWrap,
                )
            }
        }
    }
}

#[derive(Default, Props)]
struct ActionRowProps {
    toggle_text: String,
    toggle_target: bool,
    actions_focus: Option<State<usize>>,
    group_active: bool,
    selected: Option<State<Vec<bool>>>,
    submitted: Option<State<bool>>,
    cancelled: Option<State<bool>>,
}

#[component]
fn ActionRow(props: &ActionRowProps) -> impl Into<AnyElement<'static>> {
    let mut selected = props.selected.expect("selected state is required");
    let actions_focus = props
        .actions_focus
        .expect("actions_focus state is required");
    let mut submitted = props.submitted.expect("submitted state is required");
    let mut cancelled = props.cancelled.expect("cancelled state is required");
    let group_active = props.group_active;
    let focus = actions_focus.get();
    let toggle_focused = group_active && focus == ACTION_TOGGLE_ALL;
    let ok_focused = group_active && focus == ACTION_OK;
    let cancel_focused = group_active && focus == ACTION_CANCEL;
    let toggle_text = props.toggle_text.clone();
    let toggle_target = props.toggle_target;

    element! {
        View(width: 100pct, justify_content: JustifyContent::SpaceBetween) {
            View {
                Button(handler: move |_| {
                    let mut value = selected.write();
                    for v in value.iter_mut() {
                        *v = toggle_target;
                    }
                }) {
                    Text(
                        content: toggle_text,
                        color: action_color(ActionStyle::Neutral),
                        weight: Weight::Bold,
                        invert: toggle_focused,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            View(gap: 1) {
                Button(handler: move |_| submitted.set(true)) {
                    Text(
                        content: "确定 [Enter]",
                        color: action_color(ActionStyle::Primary),
                        weight: Weight::Bold,
                        invert: ok_focused,
                        wrap: TextWrap::NoWrap,
                    )
                }
                Button(handler: move |_| cancelled.set(true)) {
                    Text(
                        content: "取消 [Esc]",
                        color: action_color(ActionStyle::Danger),
                        weight: Weight::Bold,
                        invert: cancel_focused,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
        }
    }
}

fn handle_key_event(
    code: KeyCode,
    target_count: usize,
    visible: &[usize],
    controls: &mut KeyEventControls<'_>,
) {
    let group = controls.active_group.get();
    match code {
        KeyCode::Tab => switch_group(controls.active_group, 1),
        KeyCode::BackTab => switch_group(controls.active_group, GROUP_COUNT - 1),
        KeyCode::Up => arrow_within_group(
            group,
            true,
            visible,
            controls.targets_focus,
            controls.actions_focus,
        ),
        KeyCode::Down => arrow_within_group(
            group,
            false,
            visible,
            controls.targets_focus,
            controls.actions_focus,
        ),
        KeyCode::Left => arrow_horizontal_within_group(
            group,
            true,
            target_count,
            controls.targets_focus,
            controls.actions_focus,
        ),
        KeyCode::Right => arrow_horizontal_within_group(
            group,
            false,
            target_count,
            controls.targets_focus,
            controls.actions_focus,
        ),
        KeyCode::Enter => {
            activate_focused_control(
                group,
                target_count,
                controls.selected,
                controls.targets_focus,
                controls.actions_focus,
                controls.submitted,
                controls.cancelled,
            );
        }
        KeyCode::Char(' ') => {
            activate_focused_control(
                group,
                target_count,
                controls.selected,
                controls.targets_focus,
                controls.actions_focus,
                controls.submitted,
                controls.cancelled,
            );
        }
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => controls.cancelled.set(true),
        KeyCode::Char('a') | KeyCode::Char('A') => toggle_all_variants(controls.selected),
        KeyCode::Char(key) => {
            if visible.len() == target_count
                && let Some(index) = shortcut_index(key)
                && index < target_count
            {
                controls.active_group.set(GROUP_TARGETS);
                focus_and_toggle_variant(controls.selected, controls.targets_focus, index);
            }
        }
        _ => {}
    }
}

struct KeyEventControls<'a> {
    active_group: &'a mut State<usize>,
    targets_focus: &'a mut State<usize>,
    actions_focus: &'a mut State<usize>,
    selected: &'a mut State<Vec<bool>>,
    submitted: &'a mut State<bool>,
    cancelled: &'a mut State<bool>,
}

fn is_error_close_event(event: TerminalEvent) -> bool {
    match event {
        TerminalEvent::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) => matches!(
            code,
            KeyCode::Enter
                | KeyCode::Esc
                | KeyCode::Char(' ')
                | KeyCode::Char('q')
                | KeyCode::Char('Q')
        ),
        _ => false,
    }
}

fn switch_group(active_group: &mut State<usize>, delta: usize) {
    let next = (active_group.get() + delta) % GROUP_COUNT;
    active_group.set(next);
}

fn arrow_within_group(
    group: usize,
    up: bool,
    visible: &[usize],
    targets_focus: &mut State<usize>,
    _actions_focus: &mut State<usize>,
) {
    if group != GROUP_TARGETS || visible.is_empty() {
        return;
    }
    let current = targets_focus.get();
    let pos = visible.iter().position(|&i| i == current).unwrap_or(0);
    let next_pos = if up {
        pos.checked_sub(1).unwrap_or(visible.len() - 1)
    } else {
        (pos + 1) % visible.len()
    };
    targets_focus.set(visible[next_pos]);
}

fn arrow_horizontal_within_group(
    group: usize,
    left: bool,
    _target_count: usize,
    _targets_focus: &mut State<usize>,
    actions_focus: &mut State<usize>,
) {
    if group != GROUP_ACTIONS {
        return;
    }
    let current = actions_focus.get();
    let next = if left {
        current
            .checked_sub(1)
            .unwrap_or(ACTION_COUNT.saturating_sub(1))
    } else {
        (current + 1) % ACTION_COUNT
    };
    actions_focus.set(next);
}

fn activate_focused_control(
    group: usize,
    target_count: usize,
    selected: &mut State<Vec<bool>>,
    targets_focus: &mut State<usize>,
    actions_focus: &mut State<usize>,
    submitted: &mut State<bool>,
    cancelled: &mut State<bool>,
) {
    match group {
        GROUP_TARGETS => {
            let index = targets_focus.get();
            if index < target_count {
                toggle_variant(selected, index);
            }
        }
        GROUP_ACTIONS => match actions_focus.get() {
            ACTION_TOGGLE_ALL => toggle_all_variants(selected),
            ACTION_OK => submitted.set(true),
            _ => cancelled.set(true),
        },
        _ => {}
    }
}

fn write_outcome(
    outcome: Option<&mut &mut TuiOutcome>,
    context: &TuiContext,
    selected: &State<Vec<bool>>,
    confirmed: bool,
) {
    let Some(outcome) = outcome else {
        return;
    };
    outcome.confirmed = confirmed;
    outcome.selected_variants = selected_variant_names(&context.summary.variants, &selected.read());
}

fn selected_variant_names(variants: &[String], selected: &[bool]) -> Vec<String> {
    variants
        .iter()
        .zip(selected.iter())
        .filter_map(|(name, selected)| selected.then_some(name.clone()))
        .collect()
}

fn export_mode(context: &TuiContext, outcome: TuiOutcome) -> ExportMode {
    if outcome.selected_variants.len() == context.summary.variants.len() {
        return ExportMode::All;
    }
    ExportMode::Variants(outcome.selected_variants)
}

fn scroll_focused_row(scroll_handle: &mut Ref<ScrollViewHandle>, focused_index: usize) {
    let mut handle = scroll_handle.write();
    let viewport = handle.viewport_height() as i32;
    if viewport <= 0 {
        return;
    }
    let offset = handle.scroll_offset();
    let target = focused_index as i32;
    if target < offset {
        handle.scroll_to(target);
    } else if target >= offset + viewport {
        handle.scroll_to(target - viewport + 1);
    }
}

fn focus_and_toggle_variant(
    selected: &mut State<Vec<bool>>,
    focused: &mut State<usize>,
    index: usize,
) {
    focused.set(index);
    toggle_variant(selected, index);
}

fn toggle_variant(selected: &mut State<Vec<bool>>, index: usize) {
    let mut value = selected.write();
    if let Some(slot) = value.get_mut(index) {
        *slot = !*slot;
    }
}

fn toggle_all_variants(selected: &mut State<Vec<bool>>) {
    let mut value = selected.write();
    let next = !value.iter().all(|v| *v);
    for v in value.iter_mut() {
        *v = next;
    }
}

fn is_all_selected(selected: &[bool]) -> bool {
    selected.iter().all(|value| *value)
}

fn variant_rows(
    variants: &[String],
    selected: &[bool],
    visible: &[usize],
    filter_active: bool,
) -> Vec<VariantRow> {
    visible
        .iter()
        .filter_map(|&index| {
            let name = variants.get(index)?.clone();
            let sel = selected.get(index).copied().unwrap_or(false);
            Some(VariantRow {
                index,
                name,
                selected: sel,
                shortcut: if filter_active {
                    None
                } else {
                    shortcut_label(index)
                },
            })
        })
        .collect()
}

fn visible_indices(variants: &[String], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..variants.len()).collect();
    }
    let needle = query.to_lowercase();
    variants
        .iter()
        .enumerate()
        .filter_map(|(i, name)| name.to_lowercase().contains(&needle).then_some(i))
        .collect()
}

fn shortcut_label(index: usize) -> Option<char> {
    match index {
        0..=8 => char::from_digit(index as u32 + 1, 10),
        9 => Some('0'),
        _ => None,
    }
}

fn shortcut_index(key: char) -> Option<usize> {
    match key {
        '1'..='9' => key.to_digit(10).map(|value| value as usize - 1),
        '0' => Some(9),
        _ => None,
    }
}

fn target_label(row: &VariantRow) -> String {
    match row.shortcut {
        Some(shortcut) => format!("{} [{}]", row.name, shortcut),
        None => row.name.clone(),
    }
}

fn project_title(context: &TuiContext) -> String {
    context
        .summary
        .mod_info
        .name
        .clone()
        .or_else(|| context.display_name.clone())
        .unwrap_or_else(|| package_title(&context.package))
}

fn package_description(context: &TuiContext) -> Option<String> {
    context
        .summary
        .mod_info
        .description
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
}

fn package_title(package: &std::path::Path) -> String {
    package
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("超级模组分包")
        .to_owned()
}

fn row_color(index: usize, focused: bool) -> Option<Color> {
    if focused {
        return Some(Color::AnsiValue(24));
    }
    (index % 2 == 1).then_some(Color::AnsiValue(238))
}

fn action_color(style: ActionStyle) -> Color {
    match style {
        ActionStyle::Neutral => Color::Grey,
        ActionStyle::Primary => Color::Green,
        ActionStyle::Danger => Color::Red,
    }
}
