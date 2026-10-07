# Desktop capability support matrix

Reference: [Qt 6.11 widget catalog](https://doc.qt.io/qt-6/widget-classes.html) and [Qt 6.11 module catalog](https://doc.qt.io/qt-6/qtmodules.html), checked 2026-09-20. This compares *capabilities*, not Rust type names. `In scope` means planned for desktop 1.0, not implemented today. `Deferred` needs a later product decision. `Out` means applications use other libraries or OS APIs directly.

Milestones refer to [the roadmap](ROADMAP.md). For every in-scope row, the acceptance scenario runs on macOS, Windows, Wayland, and X11 before that backend is called supported. Native menus, dialogs, tray, IME, accessibility, and notifications also require platform-specific manual checks. Headless core tests are insufficient for those capabilities.

| Family and Qt comparison points | Decision | Equivalent or reason; milestone | Acceptance scenario / backend limit |
| --- | --- | --- | --- |
| Generic widget/container (`QWidget`) | In scope | Retained `Node` plus custom `Control`; 2, 6 | Nested control retains identity through updates; native embedding is not implied. |
| Buttons (`QPushButton`, `QCommandLinkButton`, `QToolButton`, `QAbstractButton`) | In scope | Shared command and activation behavior with visual recipes; 6, 12 | Keyboard, pointer, and accessibility activation fire the same enabled command. |
| Check/radio (`QCheckBox`, `QRadioButton`, `QButtonGroup`) | In scope | Toggle and exclusive-group semantics; 6, 12 | Arrow and space navigation maintains exactly one selected radio. |
| Labels and numeric display (`QLabel`, `QLCDNumber`) | In scope | Text/image label and numeric visual recipe; 6, 12 | Accessible text updates with displayed value. |
| Single-line and rich/plain editing (`QLineEdit`, text documents) | In scope | Native IME-backed text model; 7, 12 | Vietnamese, CJK, emoji, RTL selection, password masking, undo. |
| Combo and completion (`QComboBox`, completer) | In scope | Popup selection and suggestions; 12 | Keyboard selection works when items update asynchronously. |
| Date/time/calendar (`QDateEdit`, `QTimeEdit`, `QDateTimeEdit`, `QCalendarWidget`) | In scope | Locale-aware structured input and date picker; 7, 12 | Invalid intermediate input and calendar choice produce correct accessible values. |
| Numeric input (`QSpinBox`, `QDoubleSpinBox`, `QAbstractSpinBox`) | In scope | Validated integer/decimal fields with step actions; 7, 12 | Locale switch updates formatting without changing numeric value. |
| Range controls (`QDial`, `QSlider`, `QAbstractSlider`) | In scope | Semantic range value; 6, 12 | Keyboard increments and assistive actions match pointer drag. |
| Progress (`QProgressBar`) | In scope | Determinate and indeterminate state; 12 | Value is announced; indeterminate animation stops when hidden. |
| Scroll (`QScrollArea`, `QScrollBar`, `QAbstractScrollArea`) | In scope | Viewport and standalone bar; 3, 6, 12 | Wheel and keyboard scrolling preserve logical coordinates and focus. |
| Tabs and stacked containers (`QTabBar`, `QTabWidget`, `QToolBox`, `QStackedWidget`) | In scope | Tab, accordion, and stack controls; 12 | Keyboard selection updates visible panel and semantic relationships. |
| Focus frame and size grip (`QFocusFrame`, `QSizeGrip`) | In scope | Focus visual and resize affordance; 12 | Focus remains visible; resize respects OS window constraints. |
| Font chooser (`QFontComboBox`) | In scope | Installed-font selector; 12 | Missing font fallback and selection remain accessible. |
| Menus (`QMenu`) | In scope | Native menu or platform-compliant popup backed by commands; 6, 12 | Dismissal, focus restoration, shortcuts, and menu accessibility. |
| Calendar/list/column/tree/table/undo views (`QCalendarWidget`, `QListView`, `QColumnView`, `QTreeView`, `QTableView`, `QUndoView`) | In scope | Model-backed views and undo history; 12 | Large datasets virtualize while selection identity survives updates. |
| Data-widget mapper (`QDataWidgetMapper`) | In scope | Field bindings to a selected model item; 12 | Switching rows updates editors without losing pending validation state. |
| Shared dialog/frame behavior (`QDialog`, `QFrame`) | In scope | Modal/modeless window and decorative frame contracts; 12 | Modality, button ordering, focus restoration, and semantics. |
| Group and splitter (`QGroupBox`, `QSplitter`, `QSplitterHandle`) | In scope | Titled group and keyboard-resizable divider; 3, 12 | Divider movement persists and reports accessible range value. |
| Docking (`QDockWidget`, `QMainWindow`) | In scope | Main shell with floating/docked panels; 12 | Dock layout persists across restart and keyboard focus survives reparenting. |
| MDI/subwindows (`QMdiArea`, `QMdiSubWindow`) | Deferred | Desktop 1.0 favors multiple native windows; later demand may justify canvas subwindows. | No MDI API or support claim in 1.0. |
| Main-window menu, toolbar, status (`QMenuBar`, `QToolBar`, `QStatusBar`, `QWidgetAction`) | In scope | Shared commands and embedded controls; 12 | Command state agrees across menu/toolbar; status updates are announced. |
| Box/grid/form/stack layouts (`QBoxLayout`, `QHBoxLayout`, `QVBoxLayout`, `QGridLayout`, `QFormLayout`, `QStackedLayout`) | In scope | Measure/arrange layouts; 3 | Tiny and huge windows, long labels, RTL, and dynamic text size. |
| Layout items, spacers, policy (`QLayout`, `QLayoutItem`, `QWidgetItem`, `QSpacerItem`, `QSizePolicy`) | In scope | Custom measure item, spacer, min/max/flex policy; 3 | Nested flexible children meet constraints or fail predictably. |
| Appearance (`QColor`, `QColorSpace`, `QColorTransform`, `QPalette`, `QStyle`, `QCommonStyle`, `QStyleFactory`, `QStyleOption`, `QStylePainter`, style hints) | In scope | Theme tokens, color-space-aware display list, recipes; 4, 8 | Two structurally distinct themes hot-swap without altering behavior. |
| Fonts and cursors (`QFont`, `QFontDatabase`, `QFontInfo`, `QCursor`) | In scope | Platform font lookup/fallback and cursor assets; 5, 8 | Mixed-script font fallback and cursor changes across scales. |
| Abstract models/indexes/roles (`QAbstractItemModel`, `QAbstractListModel`, `QAbstractTableModel`, `QModelIndex`, `QPersistentModelIndex`, `QModelRoleData`, `QModelRoleDataSpan`) | In scope | Stable item keys and role access; 0, 12 | Selection points to the same item after insertion/filtering. |
| Range model and adapter helpers (`QRangeModel`, `QRangeModelAdapter` and nested access/iterator/reference types) | In scope | Rust iterator/range adapters, without one-to-one helper types; 12 | A borrowed data source renders and updates without copying all rows. |
| Selection (`QItemSelection`, `QItemSelectionModel`, `QItemSelectionRange`) | In scope | Shared keyed selection and ranges; 12 | Multi-range selection survives model reorder. |
| Proxy models (`QAbstractProxyModel`, `QSortFilterProxyModel`, `QIdentityProxyModel`, `QConcatenateTablesProxyModel`) | In scope | Sort, filter, identity, and concatenation adapters; 12 | Selected key survives composed sort/filter/concat changes. |
| Generic and convenience models (`QStandardItem`, `QStandardItemModel`, `QStringListModel`, `QListWidget`, `QTableWidget`, `QTreeWidget` and item/range/iterator helpers) | In scope | Owned item models and convenience constructors; 12 | Insert/delete/edit rows via the public view API with stable identity. |
| Filesystem model (`QFileSystemModel`) | In scope | Lazy filesystem browser adapter; 12 | Directory changes update view without blocking the UI thread. |
| Headers and delegates (`QHeaderView`, `QAbstractItemDelegate`, `QItemDelegate`, `QStyledItemDelegate`, `QItemEditorFactory`, creator types) | In scope | Header control and safe editor factory; 12 | Resize/sort header and edit cells with validation and accessible state. |
| 2D scene/items (`QGraphicsScene`, `QGraphicsView`, `QGraphicsItem`, `QGraphicsObject`, primitive line/rect/ellipse/polygon/path/pixmap/text/SVG items, item groups) | In scope | Interactive canvas and scene graph; 12 | Zoomed large scene culls offscreen items and selects transformed item. |
| Scene events (`QGraphicsSceneEvent` and context/help/hover/mouse/wheel/drag-drop/move/resize variants) | In scope | Unified canvas event routing; 12 | Pointer, wheel, tooltip, drag, and keyboard selection share transforms. |
| Scene transforms/effects (`QGraphicsTransform`, `QGraphicsEffect`, `QStyleOptionGraphicsItem`) | In scope | Transform and bounded visual effects; 4, 12 | Hit test and export agree with transformed pixels. |
| Scene layouts and widget proxy (`QGraphicsAnchor`, anchor/grid/linear layouts, `QGraphicsLayout`, `QGraphicsLayoutItem`, `QGraphicsWidget`, `QGraphicsProxyWidget`) | Deferred | Canvas layout and embedded *custom* controls after scene basics; native-control proxy is out of scope. | Explicit capability query returns unsupported until implemented. |
| Dialogs/utilities (file, color, font, message, input dialogs; validators, input masks, key capture, undo stacks, icons, desktop services) | In scope | OS dialogs where available; custom validation, undo, resources; 7, 12 | Open/save, validation, undo history, and URL handoff work with accessibility. |
| Native control embedding | Out | Custom-drawn controls are foundation; host controls only in native menus/dialogs. | Safe API offers no arbitrary native-handle insertion. |
| Declarative composition, bindings, states and transitions (`Qt Qml`, `Qt Quick`, Controls, Dialogs, Layouts, Models) | Deferred | Rust builder/reconciliation may follow retained tree; no QML or JS runtime. | No declarative parity claim until dynamic replacement preserves node state. |
| QML infrastructure (`Qt Qml Core`, WorkerScript, Compiler, Quick Widgets, Qml XmlListModel) | Out | No QML engine, JavaScript worker, QML compiler, or embedded Quick surface; equivalent UI features use Rust APIs. | No QML interoperability claim. |
| Animation (`Qt Quick Timeline`) | In scope | Basic transitions and reduced-motion clocks; 8. | Animation frames stop when hidden or reduced motion is enabled. |
| Particle and Lottie animation (`Qt Quick Particles`, `Qt Lottie Animation`) | Out | Specialized animation runtimes are outside desktop UI 1.0. | No file-format or particle API claim. |
| UI core (`Qt Core`, `Qt GUI`) | In scope | UI dispatch, timers, notifications, geometry, and display list; 1–14. | Posted work runs on UI thread in deterministic order. |
| General concurrency/testing (`Qt Concurrent`, `Qt Test`, `Qt Quick Test`) | Out | Application concurrency and generic tests use Rust `std` and Cargo; UI compatibility fixtures are still in scope. | No general concurrency or test-framework API. |
| Settings and resources | In scope | Versioned per-user settings and packaged assets; 4, 12 | Restart restores geometry; missing asset returns error. |
| Desktop integration (tray/status item, notifications, URL opening, activation/reopen) | In scope | Backend services with capability queries and permission/portal errors; 1, 12 | Tray menu, notification action, URL handoff, and reopen work on each claimed backend. Wayland support depends on desktop protocols/portal availability. |
| Validated and structured inputs | In scope | Validation state separate from editing, locale-aware number/date parsing, masks; 7, 12 | Intermediate text stays editable; commit error is announced without losing the value. |
| Persistent app settings and window state | In scope | Versioned per-user storage with defaults, migrations, and explicit I/O errors; 12 | Geometry and dock state restore after relaunch on each backend. |
| Plugins | Deferred | No dynamic ABI in 1.0; Rust composition is extension route. | No plugin binary compatibility claim. |
| IPC, D-Bus, remote objects (`Qt D-Bus`, `Qt Remote Objects`) | Out | Application transport concern; portals may use private backend D-Bus. | Toolkit API exposes portal results, not generic D-Bus. |
| State machine/SCXML (`Qt State Machine`, `Qt SCXML`) | Out | Application concern; controls implement their own transitions. | No SCXML runtime. |
| Structured data (`Qt XML`, JSON/CBOR, `Qt Qml XmlListModel`) | Out | Application serialization concern; settings use a documented private format. | No general parser API. |
| SVG/vector (`Qt SVG`) | In scope | SVG icons and paths; 4. | Shared render fixtures at 1x/2x and clear unsupported-element error. |
| Common raster image formats | In scope | OS-backed PNG/JPEG and platform-native formats; 4. | Decode/render fixtures agree at 1x/2x. |
| Extra formats (`Qt Image Formats`: TIFF/MNG/TGA/WBMP) | Deferred | Demand and SDK availability determine future support. | Unsupported format returns an explicit error. |
| Graphics effects, offscreen/export (`Qt Quick Effects`, `Qt 5 Compatibility Graphical Effects`, `Qt Canvas Painter`) | In scope | Bounded effects and offscreen image export; 4, 12 | Export matches on-screen clipping, alpha, and transforms. |
| 2D charts/graphs (`Qt Graphs`, `Qt Charts`, `Qt Data Visualization`) | Deferred | Canvas primitives permit app-defined charts; toolkit chart types need demand. | No chart API claim. |
| OpenGL/shaders (`Qt OpenGL`, `Qt Shader Tools`) | Deferred | Renderer-neutral display list; backend GPU choice is private. | Apps cannot rely on a GPU API through 1.0 facade. |
| 3D/physics (`Qt Quick 3D`, `Qt Quick 3D Physics`, `Qt 3D`) | Out | Separate product domain. | No 3D scene API. |
| Printing (`Qt Print Support`) | Deferred | Needs a separate cross-platform print contract. | No print support claim in 1.0. |
| PDF rendering (`Qt PDF`) | Out | Separate document product domain. | No PDF rendering API. |
| Network/protocols (`Qt Network`, WebSockets, CoAP, MQTT, GRPC, Protobuf, Network Authorization, HTTP Server, OpenAPI) | Out | Application networking concern. | No protocol API in toolkit. |
| SQL (`Qt SQL`) | Out | Application persistence concern. | No database API. |
| Web content (`Qt WebEngine`, `Qt WebView`, `Qt WebChannel`, `Qt Qml Network`) | Out | No web runtime or HTML bridge in 1.0. | No webview API. |
| Device interfaces (`Qt Bluetooth`, NFC, Serial Port, Serial Bus, Sensors, Positioning, Location, OPC UA) | Out | Application hardware/data concern. | No device API. |
| Audio/video/speech (`Qt Multimedia`, Spatial Audio, TextToSpeech) | Out | Separate product domain. | No media API. |
| Help/designer (`Qt Help`, `Qt UI Tools`, Designer) | Out | Documentation and visual design tools are application/tooling choices. | No UI form loader. |
| ActiveX/COM (`Active Qt`) | Out | Windows backend uses private COM where required; no public ActiveX host. | No ActiveX API. |
| Virtual keyboard (`Qt Virtual Keyboard`) | Out | Use OS IME and system keyboard; no toolkit keyboard UI. | Platform IME still required. |
| Wayland compositor (`Qt Wayland Compositor`) | Out | Toolkit is a client, not a compositor. | Wayland client gate remains in scope. |
| Mobile/embedded targets | Out | Desktop OS support only. | No mobile or embedded support claim. |
| Preview platform/style modules (`Qt Labs Platform`, `Qt Labs StyleKit`) | Deferred | Stable desktop platform and theme contracts take priority. | No preview-module parity claim. |
| Task workflows (`Qt TaskTree`) | Out | Generic application workflow concern. | No task-workflow API. |
| Qt 5 compatibility modules | Out | No compatibility layer for obsolete Qt APIs. | No Qt binary/source compatibility claim. |
