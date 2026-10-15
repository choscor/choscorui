//! Every public hosted control has a view. The control catalog that the
//! controls contract tests share is the list of hosted controls. `VIEWS`
//! names the view of each catalog entry, and the compiler checks that
//! each named view exists. A new catalog entry without a view, or a view
//! entry without a catalog entry, fails the test.

#[path = "../../choscorui-controls/tests/catalog/mod.rs"]
mod catalog;

use choscorui_compose::view::{
    Accordion, Alert, AlertDialog, AspectRatio, Attachment, Avatar, AvatarGroup, Badge, Breadcrumb,
    Button, ButtonGroup, Calendar, Canvas, Card, Carousel, Chart, Checkbox, Collapsible,
    ColorPicker, ColumnBrowser, Combobox, Command, CommandLink, ContextMenu, DataTable, DatePicker,
    DateTimeEdit, DecimalSpinBox, Dial, Dialog, Divider, Dock, DropdownMenu, Empty, EmptyState,
    FieldText, FontChooser, GroupBox, HeaderView, Input, InputGroup, InputOtp, ItemGroup, Kbd,
    KeySequenceEdit, Label, List, Marker, Menubar, Message, MessageScroller, NavigationMenu,
    NumericDisplay, Pagination, Popover, Progress, Questionnaire, RadioGroup, RangeSlider,
    Resizable, ScrollBar, ScrollView, Select, Sidebar, SizeGrip, Skeleton, Slider, SpinBox,
    Spinner, StackedContainer, StatusBar, Switch, Tabs, TextEditor, TextField, Textarea, TitleBar,
    Toaster, Toggle, ToggleGroup, ToolBox, ToolButton, Toolbar,
};
use std::any::type_name;
use std::collections::BTreeSet;

/// Gives the type name of a view.
type ViewName = fn() -> &'static str;

/// The catalog name of each hosted control, and the type name of its
/// view. One view can cover more than one entry through its
/// constructors, for example `Select::native` and `Popover::hover_card`.
const VIEWS: &[(&str, ViewName)] = &[
    ("accordion", type_name::<Accordion>),
    ("alert", type_name::<Alert>),
    ("alert-dialog", type_name::<AlertDialog>),
    ("aspect-ratio", type_name::<AspectRatio>),
    ("attachment", type_name::<Attachment>),
    ("avatar", type_name::<Avatar>),
    ("avatar-group", type_name::<AvatarGroup>),
    ("badge", type_name::<Badge>),
    ("breadcrumb", type_name::<Breadcrumb>),
    ("button", type_name::<Button>),
    ("button-group", type_name::<ButtonGroup>),
    ("calendar", type_name::<Calendar>),
    ("canvas", type_name::<Canvas>),
    ("card", type_name::<Card>),
    ("carousel", type_name::<Carousel>),
    ("chart", type_name::<Chart>),
    ("checkbox", type_name::<Checkbox>),
    ("collapsible", type_name::<Collapsible>),
    ("color-picker", type_name::<ColorPicker>),
    ("column-browser", type_name::<ColumnBrowser>),
    ("combobox", type_name::<Combobox>),
    ("command", type_name::<Command>),
    ("command-link", type_name::<CommandLink>),
    ("context-menu", type_name::<ContextMenu>),
    ("data-table", type_name::<DataTable>),
    ("date-picker", type_name::<DatePicker>),
    ("date-time-edit", type_name::<DateTimeEdit>),
    ("decimal-spin-box", type_name::<DecimalSpinBox>),
    ("dial", type_name::<Dial>),
    ("dialog", type_name::<Dialog>),
    ("dock", type_name::<Dock>),
    ("drawer", type_name::<Dialog>),
    ("dropdown-menu", type_name::<DropdownMenu>),
    ("empty", type_name::<EmptyState>),
    ("field-text", type_name::<FieldText>),
    ("font-chooser", type_name::<FontChooser>),
    ("group-box", type_name::<GroupBox<Empty>>),
    ("header-view", type_name::<HeaderView>),
    ("hover-card", type_name::<Popover>),
    ("input", type_name::<Input>),
    ("input-group", type_name::<InputGroup>),
    ("input-otp", type_name::<InputOtp>),
    ("item-group", type_name::<ItemGroup>),
    ("item-view", type_name::<List>),
    ("kbd", type_name::<Kbd>),
    ("key-sequence-edit", type_name::<KeySequenceEdit>),
    ("label", type_name::<Label>),
    ("marker", type_name::<Marker>),
    ("menubar", type_name::<Menubar>),
    ("message", type_name::<Message>),
    ("message-scroller", type_name::<MessageScroller>),
    ("native-select", type_name::<Select>),
    ("navigation-menu", type_name::<NavigationMenu>),
    ("numeric-display", type_name::<NumericDisplay>),
    ("pagination", type_name::<Pagination>),
    ("popover", type_name::<Popover>),
    ("progress", type_name::<Progress>),
    ("questionnaire", type_name::<Questionnaire>),
    ("radio-group", type_name::<RadioGroup>),
    ("range-slider", type_name::<RangeSlider>),
    ("resizable", type_name::<Resizable<Empty>>),
    ("scroll-area", type_name::<ScrollView<Empty>>),
    ("scroll-bar", type_name::<ScrollBar>),
    ("select", type_name::<Select>),
    ("separator", type_name::<Divider>),
    ("sheet", type_name::<Dialog>),
    ("sidebar", type_name::<Sidebar>),
    ("size-grip", type_name::<SizeGrip>),
    ("skeleton", type_name::<Skeleton>),
    ("slider", type_name::<Slider>),
    ("spin-box", type_name::<SpinBox>),
    ("spinner", type_name::<Spinner>),
    ("stack", type_name::<StackedContainer<fn(usize) -> Empty>>),
    ("status-bar", type_name::<StatusBar>),
    ("title-bar", type_name::<TitleBar>),
    ("switch", type_name::<Switch>),
    ("tabs", type_name::<Tabs>),
    ("text-editor", type_name::<TextEditor>),
    ("text-field", type_name::<TextField>),
    ("textarea", type_name::<Textarea>),
    ("toaster", type_name::<Toaster>),
    ("toggle", type_name::<Toggle>),
    ("toggle-group", type_name::<ToggleGroup>),
    ("tool-box", type_name::<ToolBox>),
    ("tool-button", type_name::<ToolButton>),
    ("toolbar", type_name::<Toolbar>),
];

#[test]
fn every_hosted_control_has_a_view() {
    let controls: BTreeSet<&str> = catalog::every_control()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let views: BTreeSet<&str> = VIEWS.iter().map(|(name, _)| *name).collect();
    assert_eq!(views.len(), VIEWS.len(), "a control is listed twice");
    let missing: Vec<_> = controls.difference(&views).collect();
    assert!(
        missing.is_empty(),
        "hosted controls without a view (add a view in choscorui-compose and list it in VIEWS): {missing:?}"
    );
    let stale: Vec<_> = views.difference(&controls).collect();
    assert!(
        stale.is_empty(),
        "VIEWS entries that are not in the control catalog: {stale:?}"
    );
}
