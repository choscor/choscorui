use super::{
    Application, Id, MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuSpec,
    Pool, Sel, class, class_addMethod, objc_allocateClassPair, objc_registerClassPair, selector,
    send_id, send_id_cstr, send_void, send_void_id,
};
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString, c_void};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

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

fn menu_target_class() -> Result<Id, MenuError> {
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
            let action = selector(c"choscorMenuItemActivated:");
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
    application: NonNull<c_void>,
    root: NonNull<c_void>,
    target: NonNull<c_void>,
    items: HashMap<MenuCommandId, NonNull<c_void>>,
    model: MenuBarModel,
    activations: Arc<Mutex<VecDeque<MenuCommandId>>>,
    panics: Arc<Mutex<usize>>,
    _main_thread: PhantomData<Rc<()>>,
}

impl MainMenu {
    pub(super) fn install(
        application: &Application,
        model: MenuBarModel,
    ) -> Result<Self, MenuError> {
        // setMainMenu: autoreleases objects that retain every open window;
        // without a pool here they were never released.
        let _pool = Pool::new();
        unsafe {
            let root = owned_menu("")?;
            let target = NonNull::new(send_id(send_id(menu_target_class()?, c"alloc"), c"init"))
                .ok_or(MenuError::AppKitUnavailable)?;
            let mut items = HashMap::new();
            let mut commands = HashMap::new();
            for menu in model.menus() {
                let submenu = owned_menu(&menu.title)?;
                let parent_item = owned_menu_item(&menu.title, None)?;
                send_void_id(parent_item.as_ptr(), c"setSubmenu:", submenu.as_ptr());
                send_void_id(root.as_ptr(), c"addItem:", parent_item.as_ptr());
                populate(
                    submenu.as_ptr(),
                    &menu.entries,
                    target.as_ptr(),
                    &mut items,
                    &mut commands,
                )?;
                send_void(submenu.as_ptr(), c"release");
                send_void(parent_item.as_ptr(), c"release");
            }
            let activations = Arc::new(Mutex::new(VecDeque::new()));
            let panics = Arc::new(Mutex::new(0));
            bindings().lock().unwrap().insert(
                target.as_ptr() as usize,
                TargetBinding {
                    commands,
                    activations: Arc::clone(&activations),
                    panics: Arc::clone(&panics),
                },
            );
            send_void_id(application.raw.as_ptr(), c"setMainMenu:", root.as_ptr());
            Ok(Self {
                application: application.raw,
                root,
                target,
                items,
                model,
                activations,
                panics,
                _main_thread: PhantomData,
            })
        }
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
        let item = self.items[&command];
        unsafe { apply_state(item.as_ptr(), &state) }
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
            .copied()
            .ok_or(MenuError::UnknownCommand(command))?;
        unsafe {
            send_void_id(
                self.target.as_ptr(),
                c"choscorMenuItemActivated:",
                item.as_ptr(),
            )
        };
        Ok(true)
    }

    pub fn callback_panic_count(&self) -> usize {
        *self.panics.lock().unwrap()
    }
}

/// Adds `entries` to `menu`, wiring command items to `target`.
unsafe fn populate(
    menu: Id,
    entries: &[MenuEntry],
    target: Id,
    items: &mut HashMap<MenuCommandId, NonNull<c_void>>,
    commands: &mut HashMap<usize, MenuCommandId>,
) -> Result<(), MenuError> {
    for entry in entries {
        match entry {
            MenuEntry::Separator => unsafe {
                let separator = send_id(
                    class(c"NSMenuItem").map_err(|_| MenuError::AppKitUnavailable)?,
                    c"separatorItem",
                );
                send_void_id(menu, c"addItem:", separator);
            },
            MenuEntry::Command { command, state } => unsafe {
                let item = owned_menu_item(
                    &state.title,
                    state.shortcut.as_ref().map(|s| s.key.as_str()),
                )?;
                send_void_id(item.as_ptr(), c"setTarget:", target);
                set_action(item.as_ptr(), c"choscorMenuItemActivated:");
                apply_state(item.as_ptr(), state)?;
                send_void_id(menu, c"addItem:", item.as_ptr());
                commands.insert(item.as_ptr() as usize, *command);
                items.insert(*command, item);
                // The menu retains the item; `items` holds a weak pointer.
                send_void(item.as_ptr(), c"release");
            },
        }
    }
    Ok(())
}

/// A native `NSMenu` shown at a point in a window (right click, ⌃-click,
/// Shift-F10, or the accessibility ShowMenu action). Activations use the
/// same target/action path as the main menu.
pub struct ContextMenu {
    menu: NonNull<c_void>,
    target: NonNull<c_void>,
    items: HashMap<MenuCommandId, NonNull<c_void>>,
    model: MenuBarModel,
    activations: Arc<Mutex<VecDeque<MenuCommandId>>>,
    _main_thread: PhantomData<Rc<()>>,
}

impl ContextMenu {
    /// Builds the menu. Call on the main thread after `Application::new`.
    pub fn new(spec: MenuSpec) -> Result<Self, MenuError> {
        let _pool = Pool::new();
        let model = MenuBarModel::new(vec![spec])?;
        unsafe {
            let menu = owned_menu(&model.menus()[0].title)?;
            // Items keep their own enabled state instead of AppKit's
            // responder-chain validation.
            send_void_bool(menu.as_ptr(), c"setAutoenablesItems:", 0);
            let target = NonNull::new(send_id(send_id(menu_target_class()?, c"alloc"), c"init"))
                .ok_or(MenuError::AppKitUnavailable)?;
            let mut items = HashMap::new();
            let mut commands = HashMap::new();
            populate(
                menu.as_ptr(),
                &model.menus()[0].entries,
                target.as_ptr(),
                &mut items,
                &mut commands,
            )?;
            let activations = Arc::new(Mutex::new(VecDeque::new()));
            bindings().lock().unwrap().insert(
                target.as_ptr() as usize,
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
    }

    pub fn spec(&self) -> &MenuSpec {
        &self.model.menus()[0]
    }

    /// Number of native items, separators included.
    pub fn item_count(&self) -> usize {
        let _pool = Pool::new();
        unsafe { super::send_usize(self.menu.as_ptr(), c"numberOfItems") }
    }

    pub fn set_state(
        &mut self,
        command: MenuCommandId,
        state: MenuItemState,
    ) -> Result<(), MenuError> {
        let _pool = Pool::new();
        self.model.set_state(command, state.clone())?;
        let item = self.items[&command];
        unsafe { apply_state(item.as_ptr(), &state) }
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
        let item = self.items[&command];
        unsafe {
            send_void_id(
                self.target.as_ptr(),
                c"choscorMenuItemActivated:",
                item.as_ptr(),
            )
        };
        Ok(true)
    }

    pub(super) fn raw(&self) -> Id {
        self.menu.as_ptr()
    }
}

impl Drop for ContextMenu {
    fn drop(&mut self) {
        let _pool = Pool::new();
        bindings()
            .lock()
            .unwrap()
            .remove(&(self.target.as_ptr() as usize));
        unsafe {
            send_void(self.target.as_ptr(), c"release");
            send_void(self.menu.as_ptr(), c"release");
        }
    }
}

impl Drop for MainMenu {
    fn drop(&mut self) {
        let _pool = Pool::new();
        bindings()
            .lock()
            .unwrap()
            .remove(&(self.target.as_ptr() as usize));
        unsafe {
            if send_id(self.application.as_ptr(), c"mainMenu") == self.root.as_ptr() {
                send_void_id(
                    self.application.as_ptr(),
                    c"setMainMenu:",
                    std::ptr::null_mut(),
                );
            }
            send_void(self.target.as_ptr(), c"release");
            send_void(self.root.as_ptr(), c"release");
        }
    }
}

unsafe fn owned_menu(title: &str) -> Result<NonNull<c_void>, MenuError> {
    let title = ns_string(title)?;
    unsafe {
        NonNull::new(send_id_id_cstr(
            send_id(
                class(c"NSMenu").map_err(|_| MenuError::AppKitUnavailable)?,
                c"alloc",
            ),
            c"initWithTitle:",
            title,
        ))
        .ok_or(MenuError::AppKitUnavailable)
    }
}

unsafe fn owned_menu_item(
    title: &str,
    shortcut: Option<&str>,
) -> Result<NonNull<c_void>, MenuError> {
    let title = ns_string(title)?;
    let shortcut = ns_string(shortcut.unwrap_or(""))?;
    unsafe {
        NonNull::new(send_menu_item_init(
            send_id(
                class(c"NSMenuItem").map_err(|_| MenuError::AppKitUnavailable)?,
                c"alloc",
            ),
            c"initWithTitle:action:keyEquivalent:",
            title,
            std::ptr::null_mut(),
            shortcut,
        ))
        .ok_or(MenuError::AppKitUnavailable)
    }
}

unsafe fn apply_state(item: Id, state: &MenuItemState) -> Result<(), MenuError> {
    unsafe {
        send_void_id(item, c"setTitle:", ns_string(&state.title)?);
        send_void_bool(item, c"setEnabled:", i8::from(state.enabled));
        send_void_isize(item, c"setState:", isize::from(state.checked));
        let key = state
            .shortcut
            .as_ref()
            .map_or("", |shortcut| shortcut.key.as_str());
        send_void_id(item, c"setKeyEquivalent:", ns_string(key)?);
        let mask = state.shortcut.as_ref().map_or(0, |shortcut| {
            let modifiers = shortcut.modifiers;
            (usize::from(modifiers.shift) << 17)
                | (usize::from(modifiers.control) << 18)
                | (usize::from(modifiers.option) << 19)
                | (usize::from(modifiers.command) << 20)
        });
        send_void_usize(item, c"setKeyEquivalentModifierMask:", mask);
    }
    Ok(())
}

fn ns_string(value: &str) -> Result<Id, MenuError> {
    let value = CString::new(value).map_err(|_| MenuError::InvalidText)?;
    unsafe {
        NonNull::new(send_id_cstr(
            class(c"NSString").map_err(|_| MenuError::AppKitUnavailable)?,
            c"stringWithUTF8String:",
            value.as_ptr(),
        ))
        .map(NonNull::as_ptr)
        .ok_or(MenuError::AppKitUnavailable)
    }
}

unsafe fn send_id_id_cstr(receiver: Id, name: &CStr, value: Id) -> Id {
    let function: unsafe extern "C" fn(Id, Sel, Id) -> Id =
        unsafe { std::mem::transmute(super::objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}

unsafe fn send_menu_item_init(receiver: Id, name: &CStr, title: Id, action: Sel, key: Id) -> Id {
    let function: unsafe extern "C" fn(Id, Sel, Id, Sel, Id) -> Id =
        unsafe { std::mem::transmute(super::objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), title, action, key) }
}

unsafe fn set_action(receiver: Id, action: &CStr) {
    let function: unsafe extern "C" fn(Id, Sel, Sel) =
        unsafe { std::mem::transmute(super::objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(c"setAction:"), selector(action)) }
}

unsafe fn send_void_bool(receiver: Id, name: &CStr, value: i8) {
    let function: unsafe extern "C" fn(Id, Sel, i8) =
        unsafe { std::mem::transmute(super::objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}

unsafe fn send_void_isize(receiver: Id, name: &CStr, value: isize) {
    let function: unsafe extern "C" fn(Id, Sel, isize) =
        unsafe { std::mem::transmute(super::objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}

unsafe fn send_void_usize(receiver: Id, name: &CStr, value: usize) {
    let function: unsafe extern "C" fn(Id, Sel, usize) =
        unsafe { std::mem::transmute(super::objc_msgSend as *const ()) };
    unsafe { function(receiver, selector(name), value) }
}
