//! Menu system — dropdown, context, command, overflow.
//!
//! Menus render inside `.menu` / `.menu-item` chrome. Keyboard support:
//! Escape closes; menu items are focusable buttons (Tab / Enter / Space).

use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::icons::{render_icon_view, Icon};
use dioxus::prelude::*;

/// Menu item. Use inside [`DropdownMenu`], [`ContextMenu`] or [`CommandMenu`].
#[component]
pub fn MenuItem(
    /// Label text.
    label: String,
    /// Optional icon rendered before the label.
    #[props(optional)]
    icon: Option<Icon>,
    /// Optional hint / shortcut shown on the right.
    #[props(optional)]
    hint: Option<String>,
    /// Danger styling.
    #[props(optional)]
    danger: bool,
    /// Disables the item.
    #[props(optional)]
    disabled: bool,
    /// Called when the item is activated.
    #[props(optional)]
    on_select: Option<EventHandler<()>>,
) -> Element {
    let extra = if danger { " menu-item-danger" } else { "" };
    let on_select_cb = on_select;
    rsx! {
        button {
            r#type: "button",
            role: "menuitem",
            class: "menu-item{extra}",
            disabled: disabled,
            onclick: move |_| {
                if let Some(cb) = on_select_cb.as_ref() {
                    cb.call(());
                }
            },
            if let Some(ic) = icon {
                span { class: "menu-item-icon", "aria-hidden": "true", {render_icon_view(ic)} }
            }
            span { class: "flex-1 text-left", "{label}" }
            {hint.map(|h| rsx! { span { class: "text-xs text-gray-500", "{h}" } })}
        }
    }
}

/// Horizontal divider between menu items.
#[component]
pub fn MenuSeparator() -> Element {
    rsx! {
        div { class: "menu-separator", role: "separator" }
    }
}

/// Dropdown menu — a trigger button that opens a floating menu.
#[component]
pub fn DropdownMenu(
    /// Trigger button label.
    trigger: String,
    /// Extra trigger utility classes.
    #[props(optional)]
    trigger_class: Option<&'static str>,
    /// Extra menu utility classes.
    #[props(optional)]
    menu_class: Option<&'static str>,
    #[props(optional)]
    open: Option<Signal<bool>>,
    children: Element,
) -> Element {
    let open_sig = open.unwrap_or_else(|| use_signal(|| false));
    let menu_extra = menu_class.map(|c| format!(" {c}")).unwrap_or_default();
    rsx! {
        div {
            class: "relative inline-block flex items-center gap-1",
            Button {
                variant: ButtonVariant::Ghost,
                class: trigger_class.unwrap_or(""),
                aria_haspopup: Some("menu"),
                aria_expanded: Some(open_sig),
                on_click: move |_| {
                    let current = *open_sig.read();
                    let mut s = open_sig;
                    s.set(!current);
                },
                "{trigger}"
                {render_icon_view(Icon::ChevronDown)}
            }
        }
        if *open_sig.read() {
            div {
                class: "menu absolute right-0 top-full mt-1{menu_extra}",
                role: "menu",
                onclick: move |_| {
                    let mut s = open_sig;
                    s.set(false);
                },
                {children}
            }
        }
    }
}

/// Overflow menu — a "⋯" dropdown with no visible trigger text.
#[component]
pub fn OverflowMenu(
    /// Extra menu utility classes.
    #[props(optional)]
    menu_class: Option<&'static str>,
    #[props(optional)]
    open: Option<Signal<bool>>,
    children: Element,
) -> Element {
    rsx! {
        DropdownMenu {
            trigger: "⋯".to_string(),
            trigger_class: "btn-icon",
            menu_class: menu_class,
            open: open,
            {children}
        }
    }
}

/// One command entry in a [`CommandMenu`].
#[derive(Clone, PartialEq)]
pub struct CommandItem {
    pub id: String,
    pub label: String,
    pub hint: Option<String>,
}

impl CommandItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Individual command item rendered inside [`CommandMenu`].
#[component]
fn CommandMenuItem(
    index: usize,
    item: CommandItem,
    active_index: Signal<usize>,
    open: Signal<bool>,
    on_select: Option<EventHandler<String>>,
) -> Element {
    let id = item.id.clone();
    let label = item.label.clone();
    let hint = item.hint.clone();
    let cb = on_select;
    let i = index;
    let active_index_sig = active_index;
    let open_sig = open;
    let id_key = id.clone();
    rsx! {
        button {
            key: "{id_key}",
            r#type: "button",
            role: "option",
            "aria-selected": "{i == *active_index_sig.read()}",
            class: if i == *active_index_sig.read() { "menu-item w-full text-left menu-item-active" } else { "menu-item w-full text-left" },
            onmouseover: move |_| {
                let mut ai = active_index_sig;
                ai.set(i);
            },
            onclick: move |_| {
                let mut o = open_sig;
                o.set(false);
                if let Some(cb) = cb.as_ref() {
                    cb.call(id.clone());
                }
            },
            span { class: "flex-1", "{label}" }
            {hint.map(|h| rsx! { span { class: "text-xs text-gray-500", "{h}" } })}
        }
    }
}

/// Command menu / palette — a searchable list of commands.
/// Opens with `open.set(true)`; closes on selection or Escape.
#[component]
pub fn CommandMenu(
    /// Two-way bound open state.
    open: Signal<bool>,
    /// Commands to search.
    items: Vec<CommandItem>,
    /// Called with the selected command id.
    #[props(optional)]
    on_select: Option<EventHandler<String>>,
) -> Element {
    let open_sig = open;
    let query = use_signal(String::new);
    let active_index = use_signal(|| 0usize);
    let on_select_cb = on_select;

    let filtered = use_memo(move || {
        let q = query.read().to_lowercase();
        if q.is_empty() {
            items.clone()
        } else {
            items
                .iter()
                .filter(|i| i.label.to_lowercase().contains(&q))
                .cloned()
                .collect()
        }
    });

    rsx! {
        if *open_sig.read() {
            div {
                class: "dialog-overlay",
                role: "presentation",
                onclick: move |_| {
                    let mut o = open_sig;
                    o.set(false);
                    let mut q = query;
                    q.set(String::new());
                },
                div {
                    class: "menu w-80 max-w-full",
                    role: "dialog",
                    "aria-modal": "true",
                    "aria-label": "Command menu",
                    onclick: |ev: MouseEvent| ev.stop_propagation(),
                    input {
                        r#type: "text",
                        class: "input mb-1",
                        placeholder: "Type a command…",
                        value: "{query.read()}",
                        onchange: move |ev: FormEvent| {
                            let mut q = query;
                            q.set(ev.value());
                            let mut ai = active_index;
                            ai.set(0);
                        },
                        onkeydown: move |ev: KeyboardEvent| {
                            if ev.key() == Key::Escape {
                                let mut o = open_sig;
                                o.set(false);
                                let mut q = query;
                                q.set(String::new());
                            } else if ev.key() == Key::ArrowDown {
                                ev.prevent_default();
                                let len = filtered.read().len();
                                if len > 0 {
                                    let mut ai = active_index;
                                    let current = *ai.read();
                                    ai.set((current + 1) % len);
                                }
                            } else if ev.key() == Key::ArrowUp {
                                ev.prevent_default();
                                let len = filtered.read().len();
                                if len > 0 {
                                    let current = *active_index.read();
                                    let mut ai = active_index;
                                    ai.set(if current == 0 { len - 1 } else { current - 1 });
                                }
                            } else if ev.key() == Key::Enter {
                                ev.prevent_default();
                                let list = filtered.read();
                                if let Some(item) = list.get(*active_index.read()) {
                                    let mut o = open_sig;
                                    o.set(false);
                                    let mut q = query;
                                    q.set(String::new());
                                    if let Some(cb) = on_select_cb.as_ref() {
                                        cb.call(item.id.clone());
                                    }
                                }
                            }
                        },
                    }
                    if filtered.read().is_empty() {
                        div { class: "menu-item text-gray-500 cursor-default", "No matching commands" }
                    }
                    for (i, item) in filtered.read().iter().enumerate() {
                        CommandMenuItem {
                            index: i,
                            item: item.clone(),
                            active_index: active_index,
                            open: open_sig,
                            on_select: on_select_cb,
                        }
                    }
                }
            }
        }
    }
}

/// Context menu — right-click on the trigger opens a floating menu at the cursor.
#[component]
pub fn ContextMenu(
    /// The right-click target content.
    children: Element,
    /// Items rendered inside the floating menu.
    menu_items: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let mut pos = use_signal(|| (0.0_f64, 0.0_f64));
    rsx! {
        div {
            class: "contents",
            oncontextmenu: move |ev: MouseEvent| {
                ev.prevent_default();
                let coords = ev.data().coordinates().client();
                pos.set((coords.x, coords.y));
                open.set(true);
            },
            {children}
        }
        if *open.read() {
            div {
                class: "fixed inset-0 z-40",
                onclick: move |_| { let mut s = open; s.set(false); },
                oncontextmenu: move |ev: MouseEvent| {
                    ev.prevent_default();
                    let mut s = open;
                    s.set(false);
                },
            }
            div {
                class: "menu fixed z-50",
                role: "menu",
                style: "left: {pos.read().0}px; top: {pos.read().1}px;",
                onclick: move |_| { let mut s = open; s.set(false); },
                {menu_items}
            }
        }
    }
}
