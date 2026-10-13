use super::{
    Id, MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuSpec, Pool, Sel,
    class, class_addMethod, objc_allocateClassPair, objc_registerClassPair, selector,
};
use crate::ffi::Obj;
use crate::ffi::appkit::{
    ADD_ITEM, ChoscorMenuTarget, INIT_WITH_TITLE, INIT_WITH_TITLE_ACTION_KEY, MAIN_MENU,
    MENU_ITEM_ACTIVATED, NSMenu, NSMenuItem, NUMBER_OF_ITEMS, SEPARATOR_ITEM, SET_ACTION,
    SET_AUTOENABLES_ITEMS, SET_ENABLED, SET_ITEM_TITLE, SET_KEY_EQUIVALENT,
    SET_KEY_EQUIVALENT_MODIFIER_MASK, SET_MAIN_MENU, SET_STATE, SET_SUBMENU, SET_TARGET,
    SHARED_APPLICATION,
};
use crate::ffi::class::Sel as Selector;
use crate::ffi::foundation::{NSString, TextError};
use crate::ffi::retained::{Retained, alloc, alloc_init};
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

/// The action that every command item sends to its target.
const ACTION: &std::ffi::CStr = c"choscorMenuItemActivated:";

struct TargetBinding {
    commands: HashMap<usize, MenuCommandId>,
    activations: Arc<Mutex<VecDeque<MenuCommandId>>>,
    panics: Arc<Mutex<usize>>,
}

fn bindings() -> &'static Mutex<HashMap<usize, TargetBinding>> {
    static BINDINGS: OnceLock<Mutex<HashMap<usize, TargetBinding>>> = OnceLock::new();
    BINDINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

unsafe extern "C" fn activate_menu_item(target: Id, _: Sel, sender: Id) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let bindings = bindings().lock().unwrap();
        let Some(binding) = bindings.get(&(target as usize)) else {
            return;
        };
        let Some(command) = binding.commands.get(&(sender as usize)).copied() else {
            return;
        };
        binding.activations.lock().unwrap().push_back(command);
    }));
    if result.is_err()
        && let Ok(bindings) = bindings().lock()
        && let Some(binding) = bindings.get(&(target as usize))
        && let Ok(mut count) = binding.panics.lock()
    {
        *count += 1;
    }
}

/// Registers the target class on first use.
pub(crate) fn menu_target_class() -> Result<Id, MenuError> {
    static CLASS: OnceLock<Result<usize, MenuError>> = OnceLock::new();
    CLASS
        .get_or_init(|| unsafe {
            let superclass = class(c"NSObject").map_err(|_| MenuError::AppKitUnavailable)?;
            let class = objc_allocateClassPair(superclass, c"ChoscorMenuTarget".as_ptr(), 0);
            if class.is_null() {
                return super::class(c"ChoscorMenuTarget")
                    .map(|existing| existing as usize)
                    .map_err(|_| MenuError::AppKitUnavailable);
            }
            let action = selector(ACTION);
            if class_addMethod(
                class,
                action,
                activate_menu_item as *const c_void,
                c"v@:@".as_ptr(),
            ) == 0
            {
                return Err(MenuError::AppKitUnavailable);
            }
            objc_registerClassPair(class);
            Ok(class as usize)
        })
        .as_ref()
        .map(|&class| class as Id)
        .map_err(|error| *error)
}

/// Main-thread AppKit menu ownership and command activation queue.
pub struct MainMenu {
    root: Retained<NSMenu>,
    target: Retained<ChoscorMenuTarget>,
    items: HashMap<MenuCommandId, Retained<NSMenuItem>>,
    model: MenuBarModel,
    activations: Arc<Mutex<VecDeque<MenuCommandId>>>,
    panics: Arc<Mutex<usize>>,
    _main_thread: PhantomData<Rc<()>>,
}

impl MainMenu {
    pub(super) fn install(model: MenuBarModel) -> Result<Self, MenuError> {
        // setMainMenu: autoreleases objects that retain every open window;
        // without a pool here they were never released.
        let _pool = Pool::new();
        let root = owned_menu("")?;
        let target = new_target()?;
        let mut items = HashMap::new();
        let mut commands = HashMap::new();
        for menu in model.menus() {
            let submenu = owned_menu(&menu.title)?;
            let parent_item = owned_menu_item(&menu.title, None)?;
            SET_SUBMENU.send(&parent_item, (Some(&submenu),));
            ADD_ITEM.send(&root, (&parent_item,));
            populate(&submenu, &menu.entries, &target, &mut items, &mut commands)?;
        }
        let activations = Arc::new(Mutex::new(VecDeque::new()));
        let panics = Arc::new(Mutex::new(0));
        bindings().lock().unwrap().insert(
            target.addr(),
            TargetBinding {
                commands,
                activations: Arc::clone(&activations),
                panics: Arc::clone(&panics),
            },
        );
        SET_MAIN_MENU.send(shared_application()?, (Some(&root),));
        Ok(Self {
            root,
            target,
            items,
            model,
            activations,
            panics,
            _main_thread: PhantomData,
        })
    }

    pub fn model(&self) -> &MenuBarModel {
        &self.model
    }

    pub fn set_state(
        &mut self,
        command: MenuCommandId,
        state: MenuItemState,
    ) -> Result<(), MenuError> {
        let _pool = Pool::new();
        self.model.set_state(command, state.clone())?;
        apply_state(&self.items[&command], &state)
    }

    pub fn take_activations(&self) -> Vec<MenuCommandId> {
        self.activations.lock().unwrap().drain(..).collect()
    }

    /// Invokes the same Objective-C target/action path as a native selection.
    /// This is useful for application automation and native smoke tests.
    pub fn perform_action(&self, command: MenuCommandId) -> Result<bool, MenuError> {
        let _pool = Pool::new();
        if !self.model.state(command)?.enabled {
            return Ok(false);
        }
        let item = self
            .items
            .get(&command)
            .ok_or(MenuError::UnknownCommand(command))?;
        MENU_ITEM_ACTIVATED.send(&self.target, (item,));
        Ok(true)
    }

    pub fn callback_panic_count(&self) -> usize {
        *self.panics.lock().unwrap()
    }
}

/// The shared `NSApplication`.
fn shared_application() -> Result<&'static Obj<crate::ffi::appkit::NSApplication>, MenuError> {
    SHARED_APPLICATION
        .send_class(())
        .ok()
        .flatten()
        .ok_or(MenuError::AppKitUnavailable)
}

/// A new instance of the target class.
fn new_target() -> Result<Retained<ChoscorMenuTarget>, MenuError> {
    menu_target_class()?;
    alloc_init::<ChoscorMenuTarget>().map_err(|_| MenuError::AppKitUnavailable)
}

/// Adds `entries` to `menu`, wiring command items to `target`.
fn populate(
    menu: &Obj<NSMenu>,
    entries: &[MenuEntry],
    target: &Obj<ChoscorMenuTarget>,
    items: &mut HashMap<MenuCommandId, Retained<NSMenuItem>>,
    commands: &mut HashMap<usize, MenuCommandId>,
) -> Result<(), MenuError> {
    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                let separator = SEPARATOR_ITEM
                    .send_class(())
                    .ok()
                    .flatten()
                    .ok_or(MenuError::AppKitUnavailable)?;
                ADD_ITEM.send(menu, (&separator,));
            }
            MenuEntry::Command { command, state } => {
                let item = owned_menu_item(
                    &state.title,
                    state.shortcut.as_ref().map(|s| s.key.as_str()),
                )?;
                SET_TARGET.send(&item, (Some(target.upcast()),));
                SET_ACTION.send(&item, (Some(Selector::register(ACTION)),));
                apply_state(&item, state)?;
                ADD_ITEM.send(menu, (&item,));
                commands.insert(item.addr(), *command);
                items.insert(*command, item);
            }
        }
    }
    Ok(())
}

/// A native `NSMenu` shown at a point in a window (right click, ⌃-click,
/// Shift-F10, or the accessibility ShowMenu action). Activations use the
/// same target/action path as the main menu.
pub struct ContextMenu {
    menu: Retained<NSMenu>,
    target: Retained<ChoscorMenuTarget>,
    items: HashMap<MenuCommandId, Retained<NSMenuItem>>,
    model: MenuBarModel,
    activations: Arc<Mutex<VecDeque<MenuCommandId>>>,
    _main_thread: PhantomData<Rc<()>>,
}

impl ContextMenu {
    /// Builds the menu. Call on the main thread after `Application::new`.
    pub fn new(spec: MenuSpec) -> Result<Self, MenuError> {
        let _pool = Pool::new();
        let model = MenuBarModel::new(vec![spec])?;
        let menu = owned_menu(&model.menus()[0].title)?;
        // Items keep their own enabled state instead of AppKit's
        // responder-chain validation.
        SET_AUTOENABLES_ITEMS.send(&menu, (false,));
        let target = new_target()?;
        let mut items = HashMap::new();
        let mut commands = HashMap::new();
        populate(
            &menu,
            &model.menus()[0].entries,
            &target,
            &mut items,
            &mut commands,
        )?;
        let activations = Arc::new(Mutex::new(VecDeque::new()));
        bindings().lock().unwrap().insert(
            target.addr(),
            TargetBinding {
                commands,
                activations: Arc::clone(&activations),
                panics: Arc::new(Mutex::new(0)),
            },
        );
        Ok(Self {
            menu,
            target,
            items,
            model,
            activations,
            _main_thread: PhantomData,
        })
    }

    pub fn spec(&self) -> &MenuSpec {
        &self.model.menus()[0]
    }

    /// Number of native items, separators included.
    pub fn item_count(&self) -> usize {
        let _pool = Pool::new();
        usize::try_from(NUMBER_OF_ITEMS.send(&self.menu, ())).unwrap_or(0)
    }

    pub fn set_state(
        &mut self,
        command: MenuCommandId,
        state: MenuItemState,
    ) -> Result<(), MenuError> {
        let _pool = Pool::new();
        self.model.set_state(command, state.clone())?;
        apply_state(&self.items[&command], &state)
    }

    pub fn take_activations(&self) -> Vec<MenuCommandId> {
        self.activations.lock().unwrap().drain(..).collect()
    }

    /// Activates `command` as if chosen (automation and smoke tests).
    pub fn perform_action(&self, command: MenuCommandId) -> Result<bool, MenuError> {
        let _pool = Pool::new();
        if !self.model.state(command)?.enabled {
            return Ok(false);
        }
        MENU_ITEM_ACTIVATED.send(&self.target, (&*self.items[&command],));
        Ok(true)
    }

    pub(super) fn menu(&self) -> &Obj<NSMenu> {
        &self.menu
    }
}

impl Drop for ContextMenu {
    fn drop(&mut self) {
        let _pool = Pool::new();
        bindings().lock().unwrap().remove(&self.target.addr());
    }
}

impl Drop for MainMenu {
    fn drop(&mut self) {
        let _pool = Pool::new();
        bindings().lock().unwrap().remove(&self.target.addr());
        if let Ok(application) = shared_application()
            && MAIN_MENU
                .send(application, ())
                .is_some_and(|menu| menu.addr() == self.root.addr())
        {
            SET_MAIN_MENU.send(application, (None,));
        }
    }
}

fn owned_menu(title: &str) -> Result<Retained<NSMenu>, MenuError> {
    let title = ns_string(title)?;
    let menu = alloc::<NSMenu>().map_err(|_| MenuError::AppKitUnavailable)?;
    INIT_WITH_TITLE
        .init(menu, (&title,))
        .ok_or(MenuError::AppKitUnavailable)
}

fn owned_menu_item(title: &str, shortcut: Option<&str>) -> Result<Retained<NSMenuItem>, MenuError> {
    let title = ns_string(title)?;
    let shortcut = ns_string(shortcut.unwrap_or(""))?;
    let item = alloc::<NSMenuItem>().map_err(|_| MenuError::AppKitUnavailable)?;
    INIT_WITH_TITLE_ACTION_KEY
        .init(item, (&title, None, &shortcut))
        .ok_or(MenuError::AppKitUnavailable)
}

fn apply_state(item: &Obj<NSMenuItem>, state: &MenuItemState) -> Result<(), MenuError> {
    SET_ITEM_TITLE.send(item, (&*ns_string(&state.title)?,));
    SET_ENABLED.send(item, (state.enabled,));
    SET_STATE.send(item, (isize::from(state.checked),));
    let key = state
        .shortcut
        .as_ref()
        .map_or("", |shortcut| shortcut.key.as_str());
    SET_KEY_EQUIVALENT.send(item, (&*ns_string(key)?,));
    let mask = state.shortcut.as_ref().map_or(0, |shortcut| {
        let modifiers = shortcut.modifiers;
        (usize::from(modifiers.shift) << 17)
            | (usize::from(modifiers.control) << 18)
            | (usize::from(modifiers.option) << 19)
            | (usize::from(modifiers.command) << 20)
    });
    SET_KEY_EQUIVALENT_MODIFIER_MASK.send(item, (mask,));
    Ok(())
}

fn ns_string(value: &str) -> Result<Retained<NSString>, MenuError> {
    crate::ffi::foundation::ns_string(value).map_err(|error| match error {
        TextError::InteriorNul => MenuError::InvalidText,
        TextError::Unavailable => MenuError::AppKitUnavailable,
    })
}
