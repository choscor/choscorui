//! The AT-SPI roles, states, and action names of the semantic nodes.
//! The numbers are the values of `AtspiRole` and `AtspiStateType` in
//! `atspi-constants.h`.

use choscorui_core::{SemanticAction, SemanticEntry, SemanticRole};

/// One AT-SPI role: its number and its name, as `GetRoleName` gives it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Role {
    pub number: u32,
    pub name: &'static str,
}

const fn role(number: u32, name: &'static str) -> Role {
    Role { number, name }
}

pub const CHECK_BOX: Role = role(7, "check box");
pub const COLUMN_HEADER: Role = role(10, "column header");
pub const COMBO_BOX: Role = role(11, "combo box");
pub const DIALOG: Role = role(16, "dialog");
pub const FRAME: Role = role(23, "frame");
pub const IMAGE: Role = role(27, "image");
pub const LABEL: Role = role(29, "label");
pub const LIST: Role = role(31, "list");
pub const LIST_ITEM: Role = role(32, "list item");
pub const MENU: Role = role(33, "menu");
pub const MENU_BAR: Role = role(34, "menu bar");
pub const MENU_ITEM: Role = role(35, "menu item");
pub const PAGE_TAB: Role = role(37, "page tab");
pub const PAGE_TAB_LIST: Role = role(38, "page tab list");
pub const PANEL: Role = role(39, "panel");
pub const PASSWORD_TEXT: Role = role(40, "password text");
pub const PROGRESS_BAR: Role = role(42, "progress bar");
pub const PUSH_BUTTON: Role = role(43, "push button");
pub const RADIO_BUTTON: Role = role(44, "radio button");
pub const SCROLL_BAR: Role = role(48, "scroll bar");
pub const SCROLL_PANE: Role = role(49, "scroll pane");
pub const SLIDER: Role = role(51, "slider");
pub const SPIN_BUTTON: Role = role(52, "spin button");
pub const STATUS_BAR: Role = role(54, "status bar");
pub const TABLE: Role = role(55, "table");
pub const TABLE_CELL: Role = role(56, "table cell");
pub const TEXT: Role = role(61, "text");
pub const TOGGLE_BUTTON: Role = role(62, "toggle button");
pub const TOOL_BAR: Role = role(63, "tool bar");
pub const TOOL_TIP: Role = role(64, "tool tip");
pub const TREE: Role = role(65, "tree");
pub const WINDOW: Role = role(69, "window");
pub const APPLICATION: Role = role(75, "application");
pub const ENTRY: Role = role(79, "entry");
pub const LINK: Role = role(88, "link");
pub const TABLE_ROW: Role = role(90, "table row");
pub const TREE_ITEM: Role = role(91, "tree item");

/// The AT-SPI role of a semantic role. A multi-line text field is a
/// `text` object; other text fields are entries.
pub fn atspi_role(role: SemanticRole, multiline: bool) -> Role {
    match role {
        SemanticRole::Application => APPLICATION,
        SemanticRole::Window => WINDOW,
        SemanticRole::Group | SemanticRole::RadioGroup | SemanticRole::TabPanel => PANEL,
        SemanticRole::Button => PUSH_BUTTON,
        SemanticRole::Checkbox => CHECK_BOX,
        SemanticRole::RadioButton => RADIO_BUTTON,
        SemanticRole::TextField if multiline => TEXT,
        SemanticRole::TextField | SemanticRole::SearchField => ENTRY,
        SemanticRole::PasswordField => PASSWORD_TEXT,
        SemanticRole::StaticText => LABEL,
        SemanticRole::List => LIST,
        SemanticRole::ListItem => LIST_ITEM,
        SemanticRole::ScrollArea => SCROLL_PANE,
        SemanticRole::Slider => SLIDER,
        SemanticRole::SpinButton => SPIN_BUTTON,
        SemanticRole::ProgressIndicator => PROGRESS_BAR,
        SemanticRole::Tab => PAGE_TAB,
        SemanticRole::TabList => PAGE_TAB_LIST,
        SemanticRole::Table => TABLE,
        SemanticRole::Row => TABLE_ROW,
        SemanticRole::Cell => TABLE_CELL,
        SemanticRole::ColumnHeader => COLUMN_HEADER,
        SemanticRole::Tree => TREE,
        SemanticRole::TreeItem => TREE_ITEM,
        SemanticRole::Menu | SemanticRole::MenuBarItem => MENU,
        SemanticRole::MenuItem => MENU_ITEM,
        SemanticRole::ComboBox | SemanticRole::PopUpButton => COMBO_BOX,
        SemanticRole::Tooltip => TOOL_TIP,
        SemanticRole::Dialog => DIALOG,
        SemanticRole::Toolbar => TOOL_BAR,
        SemanticRole::StatusBar => STATUS_BAR,
        SemanticRole::Image => IMAGE,
        SemanticRole::Link => LINK,
        SemanticRole::ScrollBar => SCROLL_BAR,
        SemanticRole::MenuBar => MENU_BAR,
        SemanticRole::Switch => TOGGLE_BUTTON,
    }
}

/// The `xml-roles` attribute that tells a role apart when AT-SPI has no
/// role of its own for it.
pub fn xml_role(role: SemanticRole) -> Option<&'static str> {
    match role {
        SemanticRole::SearchField => Some("searchbox"),
        SemanticRole::Switch => Some("switch"),
        SemanticRole::PopUpButton => Some("button"),
        SemanticRole::MenuBarItem => Some("menuitem"),
        SemanticRole::TabPanel => Some("tabpanel"),
        SemanticRole::RadioGroup => Some("radiogroup"),
        _ => None,
    }
}

/// The AT-SPI states, by number.
pub mod state {
    pub const ACTIVE: u32 = 1;
    pub const BUSY: u32 = 3;
    pub const CHECKED: u32 = 4;
    pub const COLLAPSED: u32 = 5;
    pub const EDITABLE: u32 = 7;
    pub const ENABLED: u32 = 8;
    pub const EXPANDABLE: u32 = 9;
    pub const EXPANDED: u32 = 10;
    pub const FOCUSABLE: u32 = 11;
    pub const FOCUSED: u32 = 12;
    pub const MODAL: u32 = 16;
    pub const MULTI_LINE: u32 = 17;
    pub const PRESSED: u32 = 20;
    pub const RESIZABLE: u32 = 21;
    pub const SELECTABLE: u32 = 22;
    pub const SELECTED: u32 = 23;
    pub const SENSITIVE: u32 = 24;
    pub const SHOWING: u32 = 25;
    pub const SINGLE_LINE: u32 = 26;
    pub const VISIBLE: u32 = 30;
    pub const INVALID_ENTRY: u32 = 36;
    pub const SELECTABLE_TEXT: u32 = 38;
    pub const CHECKABLE: u32 = 41;
    pub const HAS_POPUP: u32 = 42;
    pub const READ_ONLY: u32 = 43;

    /// The event detail of a state, as in `object:state-changed:focused`.
    pub fn name(state: u32) -> &'static str {
        match state {
            ACTIVE => "active",
            BUSY => "busy",
            CHECKED => "checked",
            COLLAPSED => "collapsed",
            EDITABLE => "editable",
            ENABLED => "enabled",
            EXPANDABLE => "expandable",
            EXPANDED => "expanded",
            FOCUSABLE => "focusable",
            FOCUSED => "focused",
            MODAL => "modal",
            MULTI_LINE => "multi-line",
            PRESSED => "pressed",
            RESIZABLE => "resizable",
            SELECTABLE => "selectable",
            SELECTED => "selected",
            SENSITIVE => "sensitive",
            SHOWING => "showing",
            SINGLE_LINE => "single-line",
            VISIBLE => "visible",
            INVALID_ENTRY => "invalid-entry",
            SELECTABLE_TEXT => "selectable-text",
            CHECKABLE => "checkable",
            HAS_POPUP => "has-popup",
            READ_ONLY => "read-only",
            _ => "",
        }
    }
}

/// A set of AT-SPI states: bit `n` is state `n`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StateSet(pub u64);

impl StateSet {
    pub fn insert(&mut self, state: u32) {
        self.0 |= 1 << state;
    }

    pub fn contains(self, state: u32) -> bool {
        self.0 & (1 << state) != 0
    }

    /// The two 32-bit words of `GetState`.
    pub fn words(self) -> [u32; 2] {
        [(self.0 & 0xffff_ffff) as u32, (self.0 >> 32) as u32]
    }

    /// The states in `self` or `other` but not in both.
    pub fn changed(self, other: Self) -> impl Iterator<Item = u32> {
        let diff = self.0 ^ other.0;
        (0..64).filter(move |bit| diff & (1 << bit) != 0)
    }
}

/// The states of a semantic node.
pub fn states(entry: &SemanticEntry) -> StateSet {
    let node = &entry.node;
    let state = &node.state;
    let mut set = StateSet::default();
    if !state.disabled {
        set.insert(state::ENABLED);
        set.insert(state::SENSITIVE);
    }
    if !state.hidden {
        set.insert(state::VISIBLE);
        set.insert(state::SHOWING);
    }
    let interactive = node.actions.iter().any(|action| {
        matches!(
            action,
            SemanticAction::Activate | SemanticAction::Focus | SemanticAction::SetValue
        )
    });
    if interactive || entry.focused {
        set.insert(state::FOCUSABLE);
    }
    if entry.focused {
        set.insert(state::FOCUSED);
    }
    if let Some(checked) = state.checked {
        set.insert(state::CHECKABLE);
        if checked {
            set.insert(state::CHECKED);
            if node.role == SemanticRole::Switch {
                set.insert(state::PRESSED);
            }
        }
    }
    if let Some(selected) = state.selected {
        set.insert(state::SELECTABLE);
        if selected {
            set.insert(state::SELECTED);
        }
    }
    if let Some(expanded) = state.expanded {
        set.insert(state::EXPANDABLE);
        set.insert(if expanded {
            state::EXPANDED
        } else {
            state::COLLAPSED
        });
    }
    if state.read_only {
        set.insert(state::READ_ONLY);
    }
    if state.invalid {
        set.insert(state::INVALID_ENTRY);
    }
    if state.modal {
        set.insert(state::MODAL);
    }
    if state.busy {
        set.insert(state::BUSY);
    }
    if let Some(text) = &node.text {
        set.insert(state::SELECTABLE_TEXT);
        set.insert(if text.multiline {
            state::MULTI_LINE
        } else {
            state::SINGLE_LINE
        });
        if node.actions.contains(&SemanticAction::SetValue) && !state.read_only {
            set.insert(state::EDITABLE);
        }
    }
    if node.actions.contains(&SemanticAction::ShowMenu)
        || matches!(
            node.role,
            SemanticRole::ComboBox | SemanticRole::PopUpButton | SemanticRole::MenuBarItem
        )
    {
        set.insert(state::HAS_POPUP);
    }
    set
}

/// The actions of the `Action` interface, in the order of the node's
/// declaration. `SetValue` goes through `Value` and `EditableText`, and
/// `Focus` through `Component.GrabFocus`, so they are not in the list.
pub fn action_name(action: SemanticAction) -> Option<&'static str> {
    match action {
        SemanticAction::Activate => Some("click"),
        SemanticAction::Increment => Some("increment"),
        SemanticAction::Decrement => Some("decrement"),
        SemanticAction::Expand => Some("expand"),
        SemanticAction::Collapse => Some("collapse"),
        SemanticAction::ShowMenu => Some("show-menu"),
        SemanticAction::Dismiss => Some("dismiss"),
        SemanticAction::SetValue | SemanticAction::Focus => None,
    }
}

/// The description of an action name.
pub fn action_description(name: &str) -> &'static str {
    match name {
        "click" => "Activates the object",
        "increment" => "Increases the value",
        "decrement" => "Decreases the value",
        "expand" => "Shows the child objects",
        "collapse" => "Hides the child objects",
        "show-menu" => "Opens the menu of the object",
        "dismiss" => "Closes the object",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_words_split_the_set() {
        let mut set = StateSet::default();
        set.insert(state::FOCUSED);
        set.insert(state::READ_ONLY);
        assert_eq!(set.words(), [1 << 12, 1 << (43 - 32)]);
        let mut other = set;
        other.insert(state::CHECKED);
        assert_eq!(set.changed(other).collect::<Vec<_>>(), vec![state::CHECKED]);
    }

    #[test]
    fn text_fields_follow_their_line_mode() {
        assert_eq!(atspi_role(SemanticRole::TextField, false), ENTRY);
        assert_eq!(atspi_role(SemanticRole::TextField, true), TEXT);
        assert_eq!(atspi_role(SemanticRole::Switch, false), TOGGLE_BUTTON);
    }
}
