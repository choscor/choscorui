//! The AppKit side of the accessibility bridge: one native
//! `NSAccessibilityElement` per projected semantic node, the
//! assistive-technology action callbacks, and the notifications.

use super::*;
use crate::ffi::Obj;
use crate::ffi::appkit::{
    CONTENT_VIEW, CONVERT_RECT_TO_SCREEN, CONVERT_RECT_TO_VIEW, ChoscoruiV2AccessibilityElement,
    ELEMENT_SET_EXPANDED, ELEMENT_SET_FOCUSED, ELEMENT_SET_VALUE, ELEMENT_WITH_ROLE,
    NSAccessibility, NSAccessibilityButtonRole, NSAccessibilityCellRole,
    NSAccessibilityCheckBoxRole, NSAccessibilityComboBoxRole,
    NSAccessibilityFocusedUIElementChangedNotification, NSAccessibilityGroupRole,
    NSAccessibilityHelpTagRole, NSAccessibilityImageRole, NSAccessibilityIncrementorRole,
    NSAccessibilityLayoutChangedNotification, NSAccessibilityLinkRole, NSAccessibilityListRole,
    NSAccessibilityMenuItemRole, NSAccessibilityMenuRole, NSAccessibilityOutlineRole,
    NSAccessibilityPostNotification, NSAccessibilityProgressIndicatorRole,
    NSAccessibilityRadioButtonRole, NSAccessibilityRowRole, NSAccessibilityScrollAreaRole,
    NSAccessibilityScrollBarRole, NSAccessibilitySearchFieldSubrole,
    NSAccessibilitySecureTextFieldSubrole, NSAccessibilitySelectedTextChangedNotification,
    NSAccessibilitySliderRole, NSAccessibilityStaticTextRole, NSAccessibilityTableRole,
    NSAccessibilityTextAreaRole, NSAccessibilityTextFieldRole,
    NSAccessibilityTitleChangedNotification, NSAccessibilityToolbarRole,
    NSAccessibilityValueChangedNotification, NSAccessibilityWindowRole, NSView, NSWindow,
    SET_ACCESSIBILITY_CHILDREN, SET_ACCESSIBILITY_COLUMN_COUNT, SET_ACCESSIBILITY_ENABLED,
    SET_ACCESSIBILITY_EXPANDED, SET_ACCESSIBILITY_FOCUSED, SET_ACCESSIBILITY_FRAME,
    SET_ACCESSIBILITY_HELP, SET_ACCESSIBILITY_INDEX, SET_ACCESSIBILITY_LABEL,
    SET_ACCESSIBILITY_LINKED_UI_ELEMENTS, SET_ACCESSIBILITY_ROLE, SET_ACCESSIBILITY_ROW_COUNT,
    SET_ACCESSIBILITY_SELECTED, SET_ACCESSIBILITY_SELECTED_TEXT_RANGE, SET_ACCESSIBILITY_SUBROLE,
    SET_ACCESSIBILITY_TITLE_UI_ELEMENT, SET_ACCESSIBILITY_VALUE,
};
use crate::ffi::encode::Bool;
use crate::ffi::foundation::{
    DESCRIPTION, NSObject, NSString, NUMBER_WITH_BOOL, ns_array, ns_string, string,
};
use crate::ffi::retained::Retained;

/// The native elements of one window and their projection.
pub(crate) struct NativeAccessibilityState {
    pub(crate) projection: AccessibilityProjection,
    pub(crate) elements: HashMap<AccessibilityIdentity, Retained<ChoscoruiV2AccessibilityElement>>,
    pub(crate) last_announcement: Option<(String, choscorui_core::AnnouncementPriority)>,
}

impl NativeAccessibilityState {
    pub(crate) fn new() -> Self {
        Self {
            projection: AccessibilityProjection::new(),
            elements: HashMap::new(),
            last_announcement: None,
        }
    }
}

pub(crate) type AccessibilityActionQueue = Arc<Mutex<VecDeque<AccessibilityActionRequest>>>;

#[derive(Clone)]
struct NativeAccessibilityBinding {
    node: choscorui_core::NodeId,
    actions: Vec<SemanticAction>,
    queue: AccessibilityActionQueue,
    updating: bool,
    text: Option<SemanticText>,
}

static ACCESSIBILITY_BINDINGS: OnceLock<Mutex<HashMap<usize, NativeAccessibilityBinding>>> =
    OnceLock::new();

/// The bindings of the live elements, keyed by element address.
fn accessibility_bindings() -> &'static Mutex<HashMap<usize, NativeAccessibilityBinding>> {
    ACCESSIBILITY_BINDINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Forgets the binding of an element that the window releases.
pub(crate) fn unbind(element: &Obj<ChoscoruiV2AccessibilityElement>) {
    accessibility_bindings()
        .lock()
        .unwrap()
        .remove(&element.addr());
}

/// The binding of a live element.
fn binding(element: &Obj<ChoscoruiV2AccessibilityElement>) -> Option<NativeAccessibilityBinding> {
    accessibility_bindings()
        .lock()
        .unwrap()
        .get(&element.addr())
        .cloned()
}

pub(crate) fn perform_press(element: &Obj<ChoscoruiV2AccessibilityElement>) -> bool {
    queue_accessibility_action(element, SemanticAction::Activate)
}

fn queue_accessibility_action(
    element: &Obj<ChoscoruiV2AccessibilityElement>,
    action: SemanticAction,
) -> bool {
    let Some(binding) = binding(element) else {
        return false;
    };
    if !binding.actions.contains(&action) {
        return false;
    }
    binding
        .queue
        .lock()
        .unwrap()
        .push_back(AccessibilityActionRequest {
            node: binding.node,
            action,
            value: None,
        });
    true
}

pub(crate) fn perform_increment(element: &Obj<ChoscoruiV2AccessibilityElement>) -> bool {
    queue_accessibility_action(element, SemanticAction::Increment)
}

pub(crate) fn perform_decrement(element: &Obj<ChoscoruiV2AccessibilityElement>) -> bool {
    queue_accessibility_action(element, SemanticAction::Decrement)
}

pub(crate) fn perform_show_menu(element: &Obj<ChoscoruiV2AccessibilityElement>) -> bool {
    queue_accessibility_action(element, SemanticAction::ShowMenu)
}

pub(crate) fn perform_cancel(element: &Obj<ChoscoruiV2AccessibilityElement>) -> bool {
    queue_accessibility_action(element, SemanticAction::Dismiss)
}

pub(crate) fn set_expanded(element: &Obj<ChoscoruiV2AccessibilityElement>, expanded: Bool) {
    let expanded = expanded.get();
    if binding(element).is_some_and(|binding| binding.updating) {
        let _ = ELEMENT_SET_EXPANDED.super_send(element, (expanded,));
    } else {
        queue_accessibility_action(
            element,
            if expanded {
                SemanticAction::Expand
            } else {
                SemanticAction::Collapse
            },
        );
    }
}

pub(crate) fn set_value(
    element: &Obj<ChoscoruiV2AccessibilityElement>,
    value: Option<&Obj<NSObject>>,
) {
    let Some(binding) = binding(element) else {
        return;
    };
    if binding.updating {
        let _ = ELEMENT_SET_VALUE.super_send(element, (value,));
        return;
    }
    if !binding.actions.contains(&SemanticAction::SetValue) {
        return;
    }
    let Some(value) = value
        .and_then(|value| DESCRIPTION.send(value, ()))
        .and_then(|description| string(&description))
    else {
        return;
    };
    binding
        .queue
        .lock()
        .unwrap()
        .push_back(AccessibilityActionRequest {
            node: binding.node,
            action: SemanticAction::SetValue,
            value: Some(value),
        });
}

pub(crate) fn set_focused(element: &Obj<ChoscoruiV2AccessibilityElement>, focused: Bool) {
    let Some(binding) = binding(element) else {
        return;
    };
    if binding.updating {
        let _ = ELEMENT_SET_FOCUSED.super_send(element, (focused.get(),));
        return;
    }
    if !focused.get() || !binding.actions.contains(&SemanticAction::Focus) {
        return;
    }
    binding
        .queue
        .lock()
        .unwrap()
        .push_back(AccessibilityActionRequest {
            node: binding.node,
            action: SemanticAction::Focus,
            value: None,
        });
}

pub(crate) fn is_editable(element: &Obj<ChoscoruiV2AccessibilityElement>) -> bool {
    binding(element).is_some_and(|binding| {
        binding.text.is_some() && binding.actions.contains(&SemanticAction::SetValue)
    })
}

/// The UTF-16 length of the element's text. AppKit declares the count as
/// `NSInteger`.
pub(crate) fn character_count(element: &Obj<ChoscoruiV2AccessibilityElement>) -> isize {
    binding(element)
        .and_then(|binding| binding.text)
        .map_or(0, |text| {
            isize::try_from(text.character_count_utf16).unwrap_or(isize::MAX)
        })
}

/// The accessibility attributes of `object`.
fn accessibility(object: &Obj<NSObject>) -> Result<&Obj<NSAccessibility>, Error> {
    object
        .conforming::<NSAccessibility>()
        .ok_or(Error::AppKitUnavailable)
}

impl Window {
    /// Reconciles the retained semantic snapshot into stable AppKit
    /// accessibility elements and posts the corresponding OS notifications.
    pub fn set_accessibility(
        &self,
        snapshot: &[SemanticEntry],
        changes: &[SemanticChange],
    ) -> Result<(), Error> {
        let _pool = Pool::new()?;
        let window = self.ns_window();
        let view = CONTENT_VIEW
            .send(window, ())
            .ok_or(Error::AppKitUnavailable)?;
        let mut state = self.accessibility.borrow_mut();
        let update = state.projection.reconcile(snapshot, changes);
        for identity in &update.retired {
            if let Some(element) = state.elements.remove(identity) {
                unbind(&element);
            }
        }
        ChoscoruiV2AccessibilityElement::register()?;
        for element in &update.elements {
            if state.elements.contains_key(&element.identity) {
                continue;
            }
            let label = ns_string(&element.semantic.name)?;
            let parent: &Obj<NSObject> = match element
                .parent
                .and_then(|identity| state.elements.get(&identity))
            {
                Some(parent) => parent.upcast(),
                None => view.upcast(),
            };
            let frame = accessibility_screen_rect(view, window, element.bounds);
            let native = ELEMENT_WITH_ROLE
                .subclass_send::<ChoscoruiV2AccessibilityElement>((
                    native_accessibility_role(element.semantic.role),
                    frame,
                    Some(&label),
                    Some(parent),
                ))?
                .ok_or(Error::AppKitUnavailable)?;
            accessibility_bindings().lock().unwrap().insert(
                native.addr(),
                NativeAccessibilityBinding {
                    node: element.node,
                    actions: element.semantic.actions.clone(),
                    queue: Arc::clone(&self.accessibility_actions),
                    updating: true,
                    text: element.semantic.text.clone(),
                },
            );
            state.elements.insert(element.identity, native);
        }
        for element in &update.elements {
            let native = &state.elements[&element.identity];
            if let Some(binding) = accessibility_bindings()
                .lock()
                .unwrap()
                .get_mut(&native.addr())
            {
                binding.actions.clone_from(&element.semantic.actions);
                binding.updating = true;
                binding.text.clone_from(&element.semantic.text);
            }
            apply_attributes(accessibility(native.upcast())?, element, view, window)?;
            if let Some(binding) = accessibility_bindings()
                .lock()
                .unwrap()
                .get_mut(&native.addr())
            {
                binding.updating = false;
            }
        }
        let by_node: HashMap<_, _> = update
            .elements
            .iter()
            .map(|element| (element.node, &state.elements[&element.identity]))
            .collect();
        for element in &update.elements {
            let native = accessibility(state.elements[&element.identity].upcast())?;
            let mut linked: Vec<&Obj<NSObject>> = Vec::new();
            for relation in &element.semantic.relations {
                match *relation {
                    SemanticRelation::LabelledBy(target) => {
                        if let Some(target) = by_node.get(&target) {
                            SET_ACCESSIBILITY_TITLE_UI_ELEMENT
                                .send(native, (Some(target.upcast()),));
                        }
                    }
                    SemanticRelation::DescribedBy(target) | SemanticRelation::Controls(target) => {
                        if let Some(target) = by_node.get(&target) {
                            linked.push(target.upcast());
                        }
                    }
                }
            }
            if !linked.is_empty() {
                SET_ACCESSIBILITY_LINKED_UI_ELEMENTS.send(native, (Some(&*ns_array(&linked)?),));
            }
        }
        let mut children: HashMap<Option<AccessibilityIdentity>, Vec<&Obj<NSObject>>> =
            HashMap::new();
        for element in &update.elements {
            children
                .entry(element.parent)
                .or_default()
                .push(state.elements[&element.identity].upcast());
        }
        for element in &update.elements {
            let native = accessibility(state.elements[&element.identity].upcast())?;
            let values = children
                .get(&Some(element.identity))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            SET_ACCESSIBILITY_CHILDREN.send(native, (Some(&*ns_array(values)?),));
        }
        let roots = children.get(&None).map(Vec::as_slice).unwrap_or(&[]);
        SET_ACCESSIBILITY_CHILDREN.send(accessibility(view.upcast())?, (Some(&*ns_array(roots)?),));
        let text_elements: HashSet<_> = update
            .elements
            .iter()
            .filter(|element| element.semantic.text.is_some())
            .map(|element| element.identity)
            .collect();
        let post = |identity: AccessibilityIdentity, notification| {
            if let Some(element) = state.elements.get(&identity) {
                NSAccessibilityPostNotification(element.upcast(), notification);
            }
        };
        for notification in update.notifications {
            match notification {
                AccessibilityNotification::LayoutChanged => NSAccessibilityPostNotification(
                    view.upcast(),
                    NSAccessibilityLayoutChangedNotification,
                ),
                AccessibilityNotification::ValueChanged(identity)
                | AccessibilityNotification::StateChanged(identity) => {
                    post(identity, NSAccessibilityValueChangedNotification);
                    if text_elements.contains(&identity) {
                        post(identity, NSAccessibilitySelectedTextChangedNotification);
                    }
                }
                AccessibilityNotification::LabelChanged(identity) => {
                    post(identity, NSAccessibilityTitleChangedNotification);
                }
                AccessibilityNotification::FocusChanged(identity) => {
                    post(identity, NSAccessibilityFocusedUIElementChangedNotification);
                }
                AccessibilityNotification::FocusCleared => NSAccessibilityPostNotification(
                    view.upcast(),
                    NSAccessibilityFocusedUIElementChangedNotification,
                ),
            }
        }
        Ok(())
    }

    pub fn take_accessibility_actions(&self) -> Vec<AccessibilityActionRequest> {
        let mut actions: Vec<_> = self
            .accessibility_actions
            .lock()
            .unwrap()
            .drain(..)
            .collect();
        actions.extend(self.accessibility.borrow_mut().projection.take_actions());
        actions
    }
}

/// Sets the role, frame, state, text, value, and collection attributes of
/// one element.
fn apply_attributes(
    native: &Obj<NSAccessibility>,
    element: &AccessibilityElement,
    view: &Obj<NSView>,
    window: &Obj<NSWindow>,
) -> Result<(), Error> {
    let semantic = &element.semantic;
    let multiline = semantic.text.as_ref().is_some_and(|text| text.multiline);
    let role = if multiline && semantic.role == SemanticRole::TextField {
        NSAccessibilityTextAreaRole
    } else {
        native_accessibility_role(semantic.role)
    };
    SET_ACCESSIBILITY_ROLE.send(native, (Some(role),));
    let subrole = match semantic.role {
        SemanticRole::PasswordField => Some(NSAccessibilitySecureTextFieldSubrole),
        SemanticRole::SearchField => Some(NSAccessibilitySearchFieldSubrole),
        _ => None,
    };
    SET_ACCESSIBILITY_SUBROLE.send(native, (subrole,));
    SET_ACCESSIBILITY_FRAME.send(
        native,
        (accessibility_screen_rect(view, window, element.bounds),),
    );
    SET_ACCESSIBILITY_ENABLED.send(native, (!semantic.state.disabled,));
    SET_ACCESSIBILITY_FOCUSED.send(native, (element.focused,));
    if let Some(expanded) = semantic.state.expanded {
        SET_ACCESSIBILITY_EXPANDED.send(native, (expanded,));
    }
    if let Some(selected) = semantic.state.selected {
        SET_ACCESSIBILITY_SELECTED.send(native, (selected,));
    }
    SET_ACCESSIBILITY_LABEL.send(native, (Some(&*ns_string(&semantic.name)?),));
    SET_ACCESSIBILITY_HELP.send(native, (Some(&*ns_string(&semantic.description)?),));
    let value: Option<Retained<NSObject>> = if let Some(checked) = semantic.state.checked {
        let number = NUMBER_WITH_BOOL
            .class_send((checked,))?
            .ok_or(Error::AppKitUnavailable)?;
        Some(number.into_super())
    } else if let Some(value) = &semantic.value {
        Some(ns_string(value)?.into_super())
    } else {
        None
    };
    SET_ACCESSIBILITY_VALUE.send(native, (value.as_deref(),));
    if let Some(text) = &semantic.text {
        SET_ACCESSIBILITY_SELECTED_TEXT_RANGE.send(
            native,
            (NSRange {
                location: text.selection_utf16.start,
                length: text.selection_utf16.end - text.selection_utf16.start,
            },),
        );
    }
    // Virtualized collections report their full size; realized items
    // report their absolute index.
    if let Some(collection) = element.collection {
        SET_ACCESSIBILITY_ROW_COUNT.send(
            native,
            (isize::try_from(collection.item_count).unwrap_or(isize::MAX),),
        );
        if let Some(columns) = collection.column_count {
            SET_ACCESSIBILITY_COLUMN_COUNT
                .send(native, (isize::try_from(columns).unwrap_or(isize::MAX),));
        }
    }
    if let Some(index) = element.item_index {
        SET_ACCESSIBILITY_INDEX.send(native, (isize::try_from(index).unwrap_or(isize::MAX),));
    }
    Ok(())
}

fn native_accessibility_role(role: SemanticRole) -> &'static Obj<NSString> {
    match role {
        SemanticRole::Window => NSAccessibilityWindowRole,
        SemanticRole::Button => NSAccessibilityButtonRole,
        SemanticRole::Checkbox => NSAccessibilityCheckBoxRole,
        SemanticRole::RadioButton | SemanticRole::Tab => NSAccessibilityRadioButtonRole,
        SemanticRole::RadioGroup | SemanticRole::TabList | SemanticRole::TabPanel => {
            NSAccessibilityGroupRole
        }
        SemanticRole::TextField | SemanticRole::PasswordField | SemanticRole::SearchField => {
            NSAccessibilityTextFieldRole
        }
        SemanticRole::StaticText => NSAccessibilityStaticTextRole,
        SemanticRole::List => NSAccessibilityListRole,
        SemanticRole::ScrollArea => NSAccessibilityScrollAreaRole,
        SemanticRole::Slider => NSAccessibilitySliderRole,
        SemanticRole::SpinButton => NSAccessibilityIncrementorRole,
        SemanticRole::ProgressIndicator => NSAccessibilityProgressIndicatorRole,
        SemanticRole::Table => NSAccessibilityTableRole,
        SemanticRole::Row | SemanticRole::TreeItem => NSAccessibilityRowRole,
        SemanticRole::Cell => NSAccessibilityCellRole,
        SemanticRole::ColumnHeader => NSAccessibilityButtonRole,
        SemanticRole::Tree => NSAccessibilityOutlineRole,
        SemanticRole::Menu => NSAccessibilityMenuRole,
        SemanticRole::MenuItem => NSAccessibilityMenuItemRole,
        SemanticRole::ComboBox => NSAccessibilityComboBoxRole,
        SemanticRole::Tooltip => NSAccessibilityHelpTagRole,
        SemanticRole::Toolbar => NSAccessibilityToolbarRole,
        SemanticRole::Image => NSAccessibilityImageRole,
        SemanticRole::Link => NSAccessibilityLinkRole,
        SemanticRole::ScrollBar => NSAccessibilityScrollBarRole,
        SemanticRole::Application
        | SemanticRole::Group
        | SemanticRole::ListItem
        | SemanticRole::Dialog
        | SemanticRole::StatusBar => NSAccessibilityGroupRole,
    }
}

/// `bounds` in the view, in screen coordinates.
pub(crate) fn accessibility_screen_rect(
    view: &Obj<NSView>,
    window: &Obj<NSWindow>,
    bounds: choscorui_core::Rect,
) -> NSRect {
    let window_rect = CONVERT_RECT_TO_VIEW.send(view, (native_rect(bounds), None));
    CONVERT_RECT_TO_SCREEN.send(window, (window_rect,))
}
